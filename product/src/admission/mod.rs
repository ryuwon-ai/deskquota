//! Joint concurrency/RPM/TPM ownership. No quota or execution hold exists while waiting.
mod queue;
pub mod quota;
pub mod retry;
use crate::config::{Config, Endpoint};
use crate::protocol::ObservedUsage;
use queue::{Queue, ResultCell};
use quota::{Ledger, RequestCost, ReservationId, Snapshot};
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tokio::sync::Notify;
use tokio::time::Instant;

#[derive(Debug, PartialEq, Eq)]
pub enum AcquireError {
    EstimateExceedsBudget,
    QueueFull,
    InvalidRoot,
    Deadline,
}

/// Experiment-only policies; normal construction always selects request RR.
#[cfg(feature = "bench-harness")]
#[derive(Clone, Copy, PartialEq, Eq, clap::ValueEnum)]
pub enum BenchmarkPolicy {
    Rr,
    Fifo,
    Backfill,
}

#[derive(serde::Serialize)]
pub(crate) struct AdmissionStatus {
    startup_hold_ms: u64,
    shared_cooldown_ms: u64,
    queue_length: usize,
    active: usize,
    retained: usize,
    roots: Vec<RootStatus>,
    blocked_reason: Option<&'static str>,
    barrier_root: Option<String>,
    estimate_mode: &'static str,
    rpm_mode: &'static str,
    tpm_mode: &'static str,
    rpm_capacity: Option<String>,
    tpm_capacity: Option<String>,
    accounting: &'static str,
    rpm_debited: String,
    tpm_debited: String,
    tpm_held: String,
}
#[derive(serde::Serialize)]
struct RootStatus {
    id: String,
    queue_length: usize,
}
fn limit_mode(limit: &crate::config::Limit) -> &'static str {
    match limit {
        crate::config::Limit::Known(_) => "known",
        crate::config::Limit::Unknown => "unknown",
        crate::config::Limit::Unlimited => "unlimited",
    }
}
fn capacity(limit: &crate::config::Limit) -> Option<String> {
    match limit {
        crate::config::Limit::Known(n) => Some(n.to_string()),
        _ => None,
    }
}

#[derive(Clone)]
enum Clock {
    Real(Instant),
    Manual(ManualClock),
}
impl Clock {
    fn now(&self) -> Duration {
        match self {
            Self::Real(origin) => origin.elapsed(),
            Self::Manual(clock) => *clock.now.lock().expect("clock lock"),
        }
    }
    async fn wait_until(&self, at: Duration) {
        match self {
            Self::Real(origin) => match origin.checked_add(at) {
                Some(at) => tokio::time::sleep_until(at).await,
                None => std::future::pending::<()>().await,
            },
            Self::Manual(_) => std::future::pending::<()>().await,
        }
    }
}
/// Internal deterministic clock; no CLI, environment or HTTP control can select it.
#[doc(hidden)]
#[derive(Clone)]
pub struct ManualClock {
    now: Arc<Mutex<Duration>>,
    notify: Arc<Notify>,
}
impl Default for ManualClock {
    fn default() -> Self {
        Self {
            now: Arc::new(Mutex::new(Duration::ZERO)),
            notify: Arc::new(Notify::new()),
        }
    }
}
impl ManualClock {
    pub fn advance_to(&self, now: Duration) {
        let mut current = self.now.lock().expect("clock lock");
        assert!(now >= *current);
        *current = now;
        drop(current);
        self.notify.notify_waiters();
    }
}
#[doc(hidden)]
#[derive(Clone)]
pub struct Admission {
    inner: Arc<Inner>,
}
struct Inner {
    queue: Mutex<Queue>,
    root_ids: Vec<String>,
    estimate_mode: &'static str,
    quota: crate::config::Quota,
    accounting: crate::config::Accounting,
    clock: Clock,
    notify: Arc<Notify>,
}
impl Admission {
    pub fn new(config: &Config, manual: Option<ManualClock>) -> Self {
        assert!(
            crate::config::registered_root_ids(&config.roots).is_ok(),
            "validated bounded roots required"
        );
        let (clock, notify) = match manual {
            Some(manual) => {
                let notify = manual.notify.clone();
                (Clock::Manual(manual), notify)
            }
            None => (Clock::Real(Instant::now()), Arc::new(Notify::new())),
        };
        let ledger = Ledger::new(
            config.quota.clone(),
            config.accounting,
            config.concurrency,
            clock.now(),
            std::time::Duration::from_secs(config.startup_hold_secs),
        );
        Self {
            inner: Arc::new(Inner {
                queue: Mutex::new(Queue::new(ledger, config.roots.len())),
                root_ids: config.roots.iter().map(|r| r.id.clone()).collect(),
                estimate_mode: if matches!(config.quota.tpm, crate::config::Limit::Known(_)) {
                    "json_utf8_bytes_plus_output_reservation"
                } else {
                    "tpm_unenforced"
                },
                quota: config.quota.clone(),
                accounting: config.accounting,
                clock,
                notify,
            }),
        }
    }
    #[cfg(feature = "bench-harness")]
    pub fn benchmark(
        config: &Config,
        manual: Option<ManualClock>,
        policy: BenchmarkPolicy,
    ) -> Self {
        let admission = Self::new(config, manual);
        admission.inner.queue.lock().expect("queue lock").policy = policy;
        admission
    }
    pub async fn acquire(
        &self,
        root: usize,
        cost: RequestCost,
        endpoint: Endpoint,
        wait: Duration,
    ) -> Result<Hold, AcquireError> {
        let now = self.inner.clock.now();
        let queue_deadline = now.saturating_add(wait.min(Duration::from_secs(120)));
        self.acquire_at(root, cost, endpoint, now, queue_deadline)
            .await
    }
    async fn acquire_at(
        &self,
        root: usize,
        cost: RequestCost,
        endpoint: Endpoint,
        now: Duration,
        queue_deadline: Duration,
    ) -> Result<Hold, AcquireError> {
        if self.inner.clock.now() >= queue_deadline {
            return Err(AcquireError::Deadline);
        }
        let result = {
            let mut queue = self.inner.queue.lock().expect("queue lock");
            let before = queue.len();
            queue.drive(self.inner.clock.now());
            if queue.len() != before {
                self.inner.notify.notify_waiters();
            }
            queue.enqueue(root, cost, now, queue_deadline)?
        };
        let mut ticket = Ticket {
            admission: self.clone(),
            root,
            result,
            armed: true,
        };
        self.inner.notify.notify_waiters();
        loop {
            let notified = self.inner.notify.notified();
            tokio::pin!(notified);
            notified.as_mut().enable();
            let (outcome, wake, changed) = {
                let mut queue = self.inner.queue.lock().expect("queue lock");
                let before = queue.len();
                let wake = queue.drive(self.inner.clock.now());
                let outcome = ticket.result.lock().expect("ticket lock").take();
                (outcome, wake, queue.len() != before)
            };
            if changed {
                self.inner.notify.notify_waiters();
            }
            // A drive may admit or expire another waiter's ticket.
            if let Some(outcome) = outcome {
                ticket.armed = false;
                self.inner.notify.notify_waiters();
                return outcome.map(|id| Hold {
                    admission: self.clone(),
                    id,
                    started: false,
                    usage: None,
                    endpoint,
                    wait_started: now,
                    queue_deadline,
                    root,
                    cost,
                });
            }
            match wake {
                Some(at) => {
                    tokio::select! {()=&mut notified=>{},()=self.inner.clock.wait_until(at)=>{}}
                }
                None => notified.await,
            }
        }
    }
    pub fn cooldown(&self, delay: Duration) {
        let until = self.inner.clock.now().saturating_add(delay);
        let mut queue = self.inner.queue.lock().expect("queue lock");
        queue.cooldown_until = queue.cooldown_until.max(until);
        drop(queue);
        self.inner.notify.notify_waiters();
    }
    pub fn snapshot(&self) -> Snapshot {
        self.inner
            .queue
            .lock()
            .expect("queue lock")
            .ledger
            .snapshot(self.inner.clock.now())
    }
    pub(crate) fn status(&self) -> AdmissionStatus {
        let now = self.inner.clock.now();
        let mut queue = self.inner.queue.lock().expect("queue lock");
        let snapshot = queue.ledger.snapshot(now);
        let roots = self
            .inner
            .root_ids
            .iter()
            .zip(queue.lengths())
            .map(|(id, queue_length)| RootStatus {
                id: id.clone(),
                queue_length,
            })
            .collect();
        AdmissionStatus {
            startup_hold_ms: queue
                .ledger
                .startup_hold(now)
                .as_millis()
                .min(u128::from(u64::MAX)) as u64,
            shared_cooldown_ms: queue
                .cooldown_until
                .saturating_sub(now)
                .as_millis()
                .min(u128::from(u64::MAX)) as u64,
            queue_length: queue.len(),
            active: snapshot.active,
            retained: snapshot.retained,
            roots,
            blocked_reason: queue.blocked_reason(now),
            barrier_root: queue
                .barrier()
                .map(|root| self.inner.root_ids[root].clone()),
            estimate_mode: self.inner.estimate_mode,
            rpm_mode: limit_mode(&self.inner.quota.rpm),
            tpm_mode: limit_mode(&self.inner.quota.tpm),
            rpm_capacity: capacity(&self.inner.quota.rpm),
            tpm_capacity: capacity(&self.inner.quota.tpm),
            accounting: match self.inner.accounting {
                crate::config::Accounting::Reserved => "reserved",
                crate::config::Accounting::Actual => "actual",
            },
            rpm_debited: snapshot.rpm_debited.to_string(),
            tpm_debited: snapshot.tpm_debited.to_string(),
            tpm_held: snapshot.tpm_held.to_string(),
        }
    }
}
struct Ticket {
    admission: Admission,
    root: usize,
    result: ResultCell,
    armed: bool,
}
impl Drop for Ticket {
    fn drop(&mut self) {
        if self.armed {
            let inner = &self.admission.inner;
            inner.queue.lock().expect("queue lock").cancel(
                inner.clock.now(),
                self.root,
                &self.result,
            );
            inner.notify.notify_waiters();
        }
    }
}
#[doc(hidden)]
pub struct Hold {
    admission: Admission,
    id: ReservationId,
    started: bool,
    usage: Option<ObservedUsage>,
    endpoint: Endpoint,
    wait_started: Duration,
    queue_deadline: Duration,
    root: usize,
    cost: RequestCost,
}
impl Hold {
    /// Settle the previous attempt before entering the same queue with its original aging/deadline.
    pub async fn retry(self) -> Result<Self, AcquireError> {
        let admission = self.admission.clone();
        let (root, cost, endpoint, started, deadline) = (
            self.root,
            self.cost,
            self.endpoint,
            self.wait_started,
            self.queue_deadline,
        );
        drop(self);
        admission
            .acquire_at(root, cost, endpoint, started, deadline)
            .await
    }
    pub(crate) fn cooldown(&self, delay: Duration) {
        self.admission.cooldown(delay);
    }
    pub fn wait_started(&self) -> Duration {
        self.wait_started
    }
    pub fn queue_deadline(&self) -> Duration {
        self.queue_deadline
    }
    pub fn start(&mut self) {
        if !self.started {
            self.started = self
                .admission
                .inner
                .queue
                .lock()
                .expect("queue lock")
                .ledger
                .start(self.admission.inner.clock.now(), self.id);
            self.admission.inner.notify.notify_waiters();
        }
    }
    pub(crate) fn usage(&mut self, usage: Option<ObservedUsage>) {
        self.usage = usage;
    }
}
impl Drop for Hold {
    fn drop(&mut self) {
        let inner = &self.admission.inner;
        let mut queue = inner.queue.lock().expect("queue lock");
        let ledger = &mut queue.ledger;
        if self.started {
            ledger.finish(
                inner.clock.now(),
                self.id,
                self.usage.and_then(|u| u.total(self.endpoint)),
            );
        } else {
            ledger.cancel(inner.clock.now(), self.id);
        }
        drop(queue);
        inner.notify.notify_waiters();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{Accounting, Auth, CancelPolicy, Limit, Quota, Upstream};
    #[tokio::test]
    async fn unpolled_worker_future_still_owns_and_releases_admission() {
        let config = Config {
            listen: "127.0.0.1:0".parse().unwrap(),
            concurrency: 1,
            startup_hold_secs: 60,
            cache: None,
            cancel_policy: CancelPolicy::Drain,
            accounting: Accounting::Reserved,
            retry_transient_429: false,
            upstream: Upstream {
                api_base: "http://127.0.0.1:9".parse().unwrap(),
                auth: Auth::None,
                proxy: None,
                ca_bundle: None,
            },
            quota: Quota {
                rpm: Limit::Unlimited,
                tpm: Limit::Unknown,
            },
            models: vec![],
            roots: vec![crate::config::Root {
                id: "test".into(),
                endpoints: vec![Endpoint::Models],
                models: vec![],
            }],
        };
        let admission = Admission::new(&config, None);
        let hold = admission
            .acquire(
                0,
                RequestCost::Metadata,
                Endpoint::Models,
                Duration::from_secs(120),
            )
            .await
            .unwrap();
        let unpolled = async move {
            drop(hold);
        };
        assert_eq!(admission.snapshot().active, 1);
        drop(unpolled);
        let s = admission.snapshot();
        assert_eq!(s.active, 0);
        assert_eq!(s.cleanups, 1);
        assert_eq!(s.starts, 0);
    }
    #[tokio::test(start_paused = true)]
    async fn starting_provisional_hold_wakes_waiters_to_schedule_its_expiry() {
        let config = Config {
            listen: "127.0.0.1:0".parse().unwrap(),
            concurrency: 2,
            startup_hold_secs: 60,
            cache: None,
            cancel_policy: CancelPolicy::Drain,
            accounting: Accounting::Reserved,
            retry_transient_429: false,
            upstream: Upstream {
                api_base: "http://127.0.0.1:9".parse().unwrap(),
                auth: Auth::None,
                proxy: None,
                ca_bundle: None,
            },
            quota: Quota {
                rpm: Limit::Known(1.try_into().unwrap()),
                tpm: Limit::Unknown,
            },
            models: vec![],
            roots: vec![crate::config::Root {
                id: "test".into(),
                endpoints: vec![Endpoint::Models],
                models: vec![],
            }],
        };
        let admission = Admission::new(&config, None);
        tokio::time::advance(Duration::from_secs(60)).await;
        let mut first = admission
            .acquire(
                0,
                RequestCost::Metadata,
                Endpoint::Models,
                Duration::from_secs(120),
            )
            .await
            .unwrap();
        let second_admission = admission.clone();
        let second = tokio::spawn(async move {
            second_admission
                .acquire(
                    0,
                    RequestCost::Metadata,
                    Endpoint::Models,
                    Duration::from_secs(120),
                )
                .await
        });
        tokio::task::yield_now().await;
        assert!(!second.is_finished());
        first.start();
        tokio::task::yield_now().await;
        tokio::time::advance(Duration::from_secs(60)).await;
        tokio::task::yield_now().await;
        assert!(
            second.is_finished(),
            "a waiter must learn the new expiry without waiting for completion"
        );
        drop(second.await.unwrap().unwrap());
    }
    #[tokio::test]
    async fn status_keeps_wide_debt_decimal_without_loss_or_exact_estimator_claim() {
        let config = Config {
            listen: "127.0.0.1:0".parse().unwrap(),
            concurrency: 2,
            startup_hold_secs: 60,
            cache: None,
            cancel_policy: CancelPolicy::Drain,
            accounting: Accounting::Actual,
            retry_transient_429: false,
            upstream: Upstream {
                api_base: "http://127.0.0.1:9".parse().unwrap(),
                auth: Auth::None,
                proxy: None,
                ca_bundle: None,
            },
            quota: Quota {
                rpm: Limit::Unknown,
                tpm: Limit::Known(u64::MAX.try_into().unwrap()),
            },
            models: vec![],
            roots: vec![crate::config::Root {
                id: "test".into(),
                endpoints: vec![Endpoint::Responses],
                models: vec![],
            }],
        };
        let clock = ManualClock::default();
        let a = Admission::new(&config, Some(clock.clone()));
        clock.advance_to(Duration::from_secs(60));
        let mut holds = Vec::new();
        for _ in 0..2 {
            let mut hold = a
                .acquire(
                    0,
                    RequestCost::exact_fixture(1),
                    Endpoint::Responses,
                    Duration::from_secs(120),
                )
                .await
                .unwrap();
            hold.start();
            hold.usage(Some(ObservedUsage {
                input_tokens: 0,
                output_tokens: u64::MAX,
                cache_creation_input_tokens: 0,
                cache_read_input_tokens: 0,
            }));
            holds.push(hold);
        }
        drop(holds);
        let status = serde_json::to_value(a.status()).unwrap();
        assert_eq!(
            status["tpm_debited"],
            (u128::from(u64::MAX) * 2).to_string()
        );
        assert_eq!(status["rpm_mode"], "unknown");
        assert_eq!(status["tpm_capacity"], u64::MAX.to_string());
        assert_eq!(
            status["estimate_mode"],
            "json_utf8_bytes_plus_output_reservation"
        );
    }
}
