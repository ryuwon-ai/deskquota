//! Bounded, process-local failure protection. No prompts or credential labels are retained.
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::{Duration, SystemTime};

use axum::body::Body;
use axum::http::{HeaderMap, Response, StatusCode};
use serde::Serialize;
use sha2::{Digest, Sha256};
use tokio::time::Instant;

use crate::config::Auth;

const MAX_SCOPES: usize = 128;
const FAILURE_THRESHOLD: u8 = 3;
const FAILURE_GAP: Duration = Duration::from_secs(60);
const COOLDOWN: Duration = Duration::from_secs(5);
type Key = [u8; 32];

#[derive(Clone, Default)]
pub(crate) struct Circuit(Arc<Mutex<State>>);

#[derive(Default)]
struct State {
    entries: HashMap<Key, Entry>,
    counts: Snapshot,
}

struct Entry {
    gate: Gate,
    generation: u64,
    in_flight: usize,
    failures: u8,
    last_failure: Option<Instant>,
    touched: Instant,
}

enum Gate {
    Closed,
    Open { since: Instant, delay: Duration },
    HalfOpen,
}

impl Entry {
    fn blocked_for(&self) -> Option<Duration> {
        match self.gate {
            Gate::Closed => None,
            Gate::Open { since, delay } => {
                delay.checked_sub(since.elapsed()).filter(|d| !d.is_zero())
            }
            Gate::HalfOpen => Some(COOLDOWN),
        }
    }
}

#[derive(Default, Serialize)]
pub(crate) struct Snapshot {
    scopes: usize,
    open: usize,
    half_open: usize,
    tracked_attempts: usize,
    failures: u64,
    opens: u64,
    rejections: u64,
    probes: u64,
    recoveries: u64,
    capacity_bypasses: u64,
}

pub(crate) struct Scope {
    circuit: Circuit,
    key: Key,
}

pub(crate) struct Attempt {
    circuit: Circuit,
    key: Key,
    generation: u64,
    tracked: bool,
    resolved: bool,
}

#[derive(Debug)]
pub(crate) struct Open(Duration);

impl Open {
    pub(crate) fn into_response(self) -> Response<Body> {
        let mut response =
            crate::protocol::error(StatusCode::SERVICE_UNAVAILABLE, "upstream_circuit_open");
        let seconds = self
            .0
            .as_secs()
            .saturating_add(u64::from(self.0.subsec_nanos() != 0))
            .max(1);
        response.headers_mut().insert(
            "retry-after",
            seconds.to_string().parse().expect("integer header"),
        );
        response
    }
}

impl Circuit {
    pub(crate) fn scope(
        &self,
        root: usize,
        url: &url::Url,
        model: Option<usize>,
        headers: &HeaderMap,
        auth: &Auth,
    ) -> Scope {
        let mut hash = Sha256::new();
        fn frame(hash: &mut Sha256, bytes: &[u8]) {
            hash.update((bytes.len() as u64).to_be_bytes());
            hash.update(bytes);
        }
        frame(&mut hash, &(root as u64).to_be_bytes());
        frame(&mut hash, url.as_str().as_bytes());
        frame(
            &mut hash,
            &model.map_or(u64::MAX, |index| index as u64).to_be_bytes(),
        );
        let mut names = vec![
            "authorization",
            "x-api-key",
            "api-key",
            "openai-organization",
            "openai-project",
        ];
        let custom;
        if let Auth::Env { header, .. } = auth {
            custom = header.to_ascii_lowercase();
            names.push(custom.as_str());
        }
        names.sort_unstable();
        names.dedup();
        for name in names {
            frame(&mut hash, name.as_bytes());
            frame(
                &mut hash,
                &(headers.get_all(name).iter().count() as u64).to_be_bytes(),
            );
            for value in headers.get_all(name) {
                frame(&mut hash, value.as_bytes());
            }
        }
        Scope {
            circuit: self.clone(),
            key: hash.finalize().into(),
        }
    }

    pub(crate) fn snapshot(&self) -> Snapshot {
        let state = self.0.lock().expect("circuit mutex");
        Snapshot {
            scopes: state.entries.len(),
            open: state
                .entries
                .values()
                .filter(|e| matches!(e.gate, Gate::Open { .. }))
                .count(),
            half_open: state
                .entries
                .values()
                .filter(|e| matches!(e.gate, Gate::HalfOpen))
                .count(),
            tracked_attempts: state.entries.values().map(|e| e.in_flight).sum(),
            ..state.counts
        }
    }
}

impl Scope {
    pub(crate) fn check(&self) -> Result<(), Open> {
        let mut state = self.circuit.0.lock().expect("circuit mutex");
        if let Some(delay) = state.entries.get(&self.key).and_then(Entry::blocked_for) {
            state.counts.rejections = state.counts.rejections.saturating_add(1);
            return Err(Open(delay));
        }
        Ok(())
    }

    /// Acquire immediately before the wire attempt, never while waiting for quota.
    pub(crate) fn start(&self) -> Result<Attempt, Open> {
        let mut state = self.circuit.0.lock().expect("circuit mutex");
        if !state.entries.contains_key(&self.key) && state.entries.len() >= MAX_SCOPES {
            // ponytail: at most 128 entries; only inactive closed scopes may be evicted.
            let oldest = state
                .entries
                .iter()
                .filter(|(_, e)| e.in_flight == 0 && matches!(e.gate, Gate::Closed))
                .min_by_key(|(_, e)| e.touched)
                .map(|(key, _)| *key);
            if let Some(key) = oldest {
                state.entries.remove(&key);
            } else {
                // Keep the bound without turning new credentials into an unrelated outage.
                state.counts.capacity_bypasses = state.counts.capacity_bypasses.saturating_add(1);
                return Ok(Attempt {
                    circuit: self.circuit.clone(),
                    key: self.key,
                    generation: 0,
                    tracked: false,
                    resolved: false,
                });
            }
        }
        let entry = state.entries.entry(self.key).or_insert_with(|| Entry {
            gate: Gate::Closed,
            generation: 0,
            in_flight: 0,
            failures: 0,
            last_failure: None,
            touched: Instant::now(),
        });
        if let Some(delay) = entry.blocked_for() {
            state.counts.rejections = state.counts.rejections.saturating_add(1);
            return Err(Open(delay));
        }
        let probe = matches!(entry.gate, Gate::Open { .. });
        if probe {
            entry.gate = Gate::HalfOpen;
            entry.generation = entry.generation.wrapping_add(1);
        }
        entry.in_flight += 1;
        entry.touched = Instant::now();
        let generation = entry.generation;
        if probe {
            state.counts.probes = state.counts.probes.saturating_add(1);
        }
        Ok(Attempt {
            circuit: self.circuit.clone(),
            key: self.key,
            generation,
            tracked: true,
            resolved: false,
        })
    }
}

impl Attempt {
    pub(crate) fn headers(&mut self, status: StatusCode, headers: &HeaderMap) {
        if matches!(status.as_u16(), 500 | 502 | 503 | 504) {
            self.failure(
                crate::admission::retry::header_delay(headers, SystemTime::now())
                    .unwrap_or_default(),
            );
        }
    }

    pub(crate) fn failure(&mut self, delay: Duration) {
        self.resolve(Some(delay));
    }

    pub(crate) fn complete(mut self) {
        self.resolve(None);
    }

    fn resolve(&mut self, failure: Option<Duration>) {
        if self.resolved || !self.tracked {
            return;
        }
        self.resolved = true;
        let mut state = self.circuit.0.lock().expect("circuit mutex");
        if failure.is_some() {
            state.counts.failures = state.counts.failures.saturating_add(1);
        }
        let entry = state
            .entries
            .get_mut(&self.key)
            .expect("active circuit entry");
        if entry.generation != self.generation {
            return;
        }
        entry.touched = Instant::now();
        if let Some(delay) = failure {
            if entry
                .last_failure
                .is_none_or(|at| at.elapsed() > FAILURE_GAP)
            {
                entry.failures = 0;
            }
            entry.failures = entry.failures.saturating_add(1);
            entry.last_failure = Some(Instant::now());
            if entry.failures >= FAILURE_THRESHOLD || matches!(entry.gate, Gate::HalfOpen) {
                entry.gate = Gate::Open {
                    since: Instant::now(),
                    delay: delay.max(COOLDOWN),
                };
                entry.generation = entry.generation.wrapping_add(1);
                state.counts.opens = state.counts.opens.saturating_add(1);
            }
        } else {
            let recovered = matches!(entry.gate, Gate::HalfOpen);
            entry.failures = 0;
            entry.last_failure = None;
            entry.gate = Gate::Closed;
            if recovered {
                state.counts.recoveries = state.counts.recoveries.saturating_add(1);
            }
        }
    }
}

impl Drop for Attempt {
    fn drop(&mut self) {
        if !self.tracked {
            return;
        }
        let mut state = self.circuit.0.lock().expect("circuit mutex");
        let entry = state
            .entries
            .get_mut(&self.key)
            .expect("active circuit entry");
        entry.in_flight -= 1;
        if !self.resolved
            && entry.generation == self.generation
            && matches!(entry.gate, Gate::HalfOpen)
        {
            entry.gate = Gate::Open {
                since: Instant::now(),
                delay: Duration::ZERO,
            };
            entry.generation = entry.generation.wrapping_add(1);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scope(circuit: &Circuit, index: usize) -> Scope {
        circuit.scope(
            0,
            &"http://localhost/v1/chat/completions".parse().unwrap(),
            Some(index),
            &HeaderMap::new(),
            &Auth::None,
        )
    }
    fn fail(scope: &Scope) {
        let mut attempt = scope.start().unwrap();
        attempt.failure(Duration::ZERO);
    }

    #[tokio::test(start_paused = true)]
    async fn streak_resets_on_completion_and_old_failures_expire() {
        let circuit = Circuit::default();
        let scope = scope(&circuit, 0);
        fail(&scope);
        fail(&scope);
        scope.start().unwrap().complete();
        fail(&scope);
        drop(scope.start().unwrap()); // Cancellation neither fails nor claims recovery.
        fail(&scope);
        assert!(scope.check().is_ok());
        tokio::time::advance(FAILURE_GAP + Duration::from_nanos(1)).await;
        fail(&scope);
        assert!(scope.check().is_ok());
        fail(&scope);
        fail(&scope);
        assert!(scope.check().is_err());
        let snapshot = circuit.snapshot();
        assert_eq!(snapshot.failures, 7);
        assert_eq!(snapshot.opens, 1);
        assert_eq!(snapshot.tracked_attempts, 0);
    }

    #[tokio::test(start_paused = true)]
    async fn exactly_one_probe_and_stale_completions_cannot_close_new_circuits() {
        let circuit = Circuit::default();
        let scope = scope(&circuit, 0);
        let old_success = scope.start().unwrap();
        let mut old_failure = scope.start().unwrap();
        for _ in 0..3 {
            fail(&scope);
        }
        old_success.complete();
        assert!(scope.check().is_err());
        tokio::time::advance(COOLDOWN).await;
        let probe = scope.start().unwrap();
        assert!(scope.start().is_err());
        old_failure.failure(Duration::ZERO);
        drop(old_failure);
        assert_eq!(circuit.snapshot().half_open, 1);
        drop(probe); // A cancelled probe must not strand half-open ownership.
        let mut probe = scope.start().unwrap();
        probe.failure(Duration::ZERO);
        drop(probe);
        assert!(scope.check().is_err());
        tokio::time::advance(COOLDOWN).await;
        scope.start().unwrap().complete();
        assert!(scope.check().is_ok());
        assert_eq!(circuit.snapshot().recoveries, 1);
        assert_eq!(circuit.snapshot().opens, 2);
        assert_eq!(circuit.snapshot().tracked_attempts, 0);
    }

    #[tokio::test(start_paused = true)]
    async fn header_failure_is_counted_once_and_retry_after_cannot_overflow_time() {
        let circuit = Circuit::default();
        let scope = scope(&circuit, 0);
        let mut headers = HeaderMap::new();
        headers.append("retry-after", "7".parse().unwrap());
        headers.append("retry-after-ms", "9000".parse().unwrap());
        for _ in 0..3 {
            let mut attempt = scope.start().unwrap();
            attempt.headers(StatusCode::SERVICE_UNAVAILABLE, &headers);
            attempt.failure(Duration::ZERO); // A truncated 503 body must not count twice.
            attempt.complete();
        }
        assert_eq!(circuit.snapshot().failures, 3);
        tokio::time::advance(Duration::from_secs(8)).await;
        assert!(scope.check().is_err());
        tokio::time::advance(Duration::from_millis(999)).await;
        let response = scope.check().unwrap_err().into_response();
        assert_eq!(response.headers()["retry-after"], "1");
        tokio::time::advance(Duration::from_millis(1)).await;
        let mut probe = scope.start().unwrap();
        headers.insert("retry-after", "999999999999999999999999".parse().unwrap());
        probe.headers(StatusCode::SERVICE_UNAVAILABLE, &headers);
        drop(probe);
        tokio::time::advance(Duration::from_secs(3600)).await;
        assert!(scope.check().is_err());
        assert!(
            scope.check().unwrap_err().into_response().headers()["retry-after"]
                .to_str()
                .unwrap()
                .parse::<u64>()
                .is_ok()
        );
    }

    #[test]
    fn table_is_bounded_and_never_evicts_an_active_or_open_scope() {
        let circuit = Circuit::default();
        let attempts: Vec<_> = (0..MAX_SCOPES)
            .map(|index| scope(&circuit, index).start().unwrap())
            .collect();
        let bypass = scope(&circuit, MAX_SCOPES).start().unwrap();
        assert!(!bypass.tracked);
        assert_eq!(circuit.snapshot().scopes, MAX_SCOPES);
        assert_eq!(circuit.snapshot().capacity_bypasses, 1);
        drop(attempts);
        assert!(scope(&circuit, MAX_SCOPES).start().unwrap().tracked);
        for index in 0..MAX_SCOPES {
            let scope = scope(&circuit, index);
            for _ in 0..3 {
                fail(&scope);
            }
        }
        assert_eq!(circuit.snapshot().open, MAX_SCOPES);
        assert!(!scope(&circuit, MAX_SCOPES + 1).start().unwrap().tracked);
        assert_eq!(circuit.snapshot().scopes, MAX_SCOPES);
    }

    #[test]
    fn scope_separates_routing_and_credentials_but_not_request_trace_headers() {
        let circuit = Circuit::default();
        let url = "http://localhost/v1/messages?api-version=one"
            .parse()
            .unwrap();
        let mut headers = HeaderMap::new();
        headers.insert("authorization", "Bearer synthetic-one".parse().unwrap());
        let key = circuit
            .scope(0, &url, Some(0), &headers, &Auth::Forward)
            .key;
        headers.insert("x-request-id", "different-trace".parse().unwrap());
        assert_eq!(
            key,
            circuit
                .scope(0, &url, Some(0), &headers, &Auth::Forward)
                .key
        );
        assert_ne!(
            key,
            circuit
                .scope(1, &url, Some(0), &headers, &Auth::Forward)
                .key
        );
        assert_ne!(
            key,
            circuit
                .scope(0, &url, Some(1), &headers, &Auth::Forward)
                .key
        );
        assert_ne!(
            key,
            circuit.scope(0, &url, None, &headers, &Auth::Forward).key
        );
        assert_ne!(
            key,
            circuit
                .scope(
                    0,
                    &"http://localhost/v1/models".parse().unwrap(),
                    Some(0),
                    &headers,
                    &Auth::Forward
                )
                .key
        );
        for name in [
            "authorization",
            "x-api-key",
            "api-key",
            "openai-organization",
            "openai-project",
        ] {
            let mut other = headers.clone();
            other.insert(name, "synthetic-other".parse().unwrap());
            assert_ne!(
                key,
                circuit.scope(0, &url, Some(0), &other, &Auth::Forward).key
            );
        }
        let auth = Auth::Env {
            header: "X-Custom-Key".into(),
            name: "SYNTHETIC".into(),
        };
        let before = circuit.scope(0, &url, Some(0), &headers, &auth).key;
        headers.insert("x-custom-key", "synthetic-custom".parse().unwrap());
        assert_ne!(before, circuit.scope(0, &url, Some(0), &headers, &auth).key);
    }
}
