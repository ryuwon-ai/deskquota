//! Single-owner local rolling ledger. Every operation receives monotonic elapsed time.
use crate::config::{Accounting, Limit, Quota};
use std::time::Duration;

pub const WINDOW: Duration = Duration::from_secs(60);
/// A fixed memory ceiling, independent of configured provider quota. When full,
/// admission waits for expiry; no unexpired debit is discarded.
pub const MAX_ENTRIES: usize = 8192;

#[derive(Clone, Copy, Debug)]
pub struct RequestCost(Cost);
#[derive(Clone, Copy, Debug)]
enum Cost {
    Metadata,
    UnmeteredGeneration,
    Estimated {
        input_tokens: u64,
        output_tokens: u64,
    },
    ExactFixture(u64),
}
impl RequestCost {
    /// Models/count_tokens only: never settles as generation TPM usage.
    #[allow(non_upper_case_globals)]
    pub const Metadata: Self = Self(Cost::Metadata);
    /// Generation with unknown/unlimited TPM still has generation semantics.
    #[allow(non_upper_case_globals)]
    pub(crate) const UnmeteredGeneration: Self = Self(Cost::UnmeteredGeneration);
    pub fn estimated(input_tokens: u64, output_tokens: u64) -> Self {
        Self(Cost::Estimated {
            input_tokens,
            output_tokens,
        })
    }
    /// Internal fixture seam. HTTP policy never reads fixture costs from headers or bodies.
    #[doc(hidden)]
    pub fn exact_fixture(n: u64) -> Self {
        Self(Cost::ExactFixture(n))
    }
    fn tokens(self) -> u128 {
        match self.0 {
            Cost::Metadata | Cost::UnmeteredGeneration => 0,
            Cost::Estimated {
                input_tokens,
                output_tokens,
            } => u128::from(input_tokens) + u128::from(output_tokens),
            Cost::ExactFixture(n) => u128::from(n),
        }
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ReservationId(u64);
#[derive(Debug, PartialEq, Eq)]
pub enum Decision {
    Admitted(ReservationId),
    Wait(Option<Duration>),
    EstimateExceedsBudget,
}
#[derive(Default, Debug)]
pub struct Snapshot {
    pub rpm_debited: u128,
    pub tpm_debited: u128,
    pub tpm_held: u128,
    pub active: usize,
    pub retained: usize,
    pub cleanups: u64,
    pub starts: u64,
    pub reservation: ReservationDiagnostics,
}

/// Finished known-TPM generation only; these differences are not refundable quota.
#[derive(Clone, Copy, Default, Debug, serde::Serialize)]
pub struct ReservationDiagnostics {
    pub samples: u64,
    pub unknown: u64,
    #[serde(serialize_with = "decimal")]
    pub reserved_tokens: u128,
    #[serde(serialize_with = "decimal")]
    pub observed_tokens: u128,
    #[serde(serialize_with = "decimal")]
    pub excess_tokens: u128,
    #[serde(serialize_with = "decimal")]
    pub shortfall_tokens: u128,
}
fn decimal<S: serde::Serializer>(value: &u128, serializer: S) -> Result<S::Ok, S::Error> {
    serializer.collect_str(value)
}
struct Entry {
    id: ReservationId,
    active: bool,
    started: bool,
    metadata: bool,
    tokens: u128,
    rpm_until: Option<Duration>,
    tpm_until: Option<Duration>,
}
pub struct Ledger {
    quota: Quota,
    accounting: Accounting,
    concurrency: usize,
    ready_at: Duration,
    now: Duration,
    next: u64,
    entries: Vec<Entry>,
    cleanups: u64,
    starts: u64,
    reservation: ReservationDiagnostics,
}
fn known(limit: &Limit) -> Option<u128> {
    if let Limit::Known(n) = limit {
        Some(u128::from(n.get()))
    } else {
        None
    }
}
fn live(until: Option<Duration>, now: Duration) -> bool {
    until.is_some_and(|until| until > now)
}
impl Ledger {
    pub fn new(
        quota: Quota,
        accounting: Accounting,
        concurrency: u8,
        now: Duration,
        startup_hold: Duration,
    ) -> Self {
        assert!((1..=16).contains(&concurrency));
        let has_known = known(&quota.rpm).is_some() || known(&quota.tpm).is_some();
        Self {
            quota,
            accounting,
            concurrency: usize::from(concurrency),
            ready_at: if has_known {
                now.saturating_add(startup_hold)
            } else {
                now
            },
            now,
            next: 0,
            entries: Vec::new(),
            cleanups: 0,
            starts: 0,
            reservation: ReservationDiagnostics::default(),
        }
    }
    fn advance(&mut self, now: Duration) {
        self.now = self.now.max(now);
        self.entries
            .retain(|e| e.active || live(e.rpm_until, self.now) || live(e.tpm_until, self.now));
    }
    pub fn snapshot(&mut self, now: Duration) -> Snapshot {
        self.advance(now);
        let mut s = Snapshot {
            retained: self.entries.len(),
            cleanups: self.cleanups,
            starts: self.starts,
            reservation: self.reservation,
            ..Snapshot::default()
        };
        for e in &self.entries {
            s.active += usize::from(e.active);
            s.rpm_debited += u128::from(live(e.rpm_until, self.now));
            if live(e.tpm_until, self.now) {
                s.tpm_debited += e.tokens;
            }
            if !e.started {
                s.tpm_held += e.tokens;
            }
        }
        s
    }
    pub(crate) fn startup_hold(&self, now: Duration) -> Duration {
        self.ready_at.saturating_sub(now.max(self.now))
    }
    pub(crate) fn total_fits(&self, cost: RequestCost) -> bool {
        known(&self.quota.tpm).is_none_or(|limit| cost.tokens() <= limit)
    }
    pub(crate) fn check(&mut self, now: Duration, cost: RequestCost) -> Result<(), &'static str> {
        let s = self.snapshot(now);
        if !self.total_fits(cost) {
            return Err("estimate_exceeds_budget");
        }
        if self.now < self.ready_at {
            return Err("startup_hold");
        }
        if s.active >= self.concurrency {
            return Err("concurrency");
        }
        let held_rpm = self.entries.iter().filter(|e| !e.started).count() as u128;
        if known(&self.quota.rpm).is_some_and(|limit| s.rpm_debited + held_rpm >= limit) {
            return Err("rpm");
        }
        if known(&self.quota.tpm)
            .is_some_and(|limit| s.tpm_debited + s.tpm_held + cost.tokens() > limit)
        {
            return Err("tpm");
        }
        if self.entries.len() >= MAX_ENTRIES {
            return Err("ledger_capacity");
        }
        if self.next == u64::MAX {
            return Err("identity_exhausted");
        }
        Ok(())
    }
    pub(crate) fn next_wake(&self) -> Option<Duration> {
        if self.now < self.ready_at {
            return Some(self.ready_at);
        }
        self.entries
            .iter()
            .flat_map(|e| [e.rpm_until, e.tpm_until])
            .flatten()
            .filter(|at| *at > self.now)
            .min()
    }
    /// Keep the protected head's next known quota opportunity and an execution slot.
    #[cfg(feature = "bench-harness")]
    pub(crate) fn can_backfill(
        &mut self,
        now: Duration,
        head: RequestCost,
        deadline: Duration,
        candidate: RequestCost,
    ) -> bool {
        if self.accounting != Accounting::Reserved || self.check(now, candidate).is_err() {
            return false;
        }
        let Some(at) = self.next_wake() else {
            return false;
        };
        if at >= deadline
            || at >= self.now.saturating_add(WINDOW)
            || self.entries.iter().filter(|e| e.active).count() + 2 > self.concurrency
            || self.entries.len() + 2 > MAX_ENTRIES
            || self.next.checked_add(2).is_none()
        {
            return false;
        }
        let tpm = known(&self.quota.tpm);
        // Only proven metadata cannot recharge positive usage without a TPM expiry.
        if tpm.is_some() && candidate.tokens() == 0 && !matches!(candidate.0, Cost::Metadata) {
            return false;
        }
        let (mut rpm_at, mut tpm_at) = (0_u128, 0_u128);
        // ponytail: one bounded 8192-entry scan, next expiry only; profile before adding an index.
        for e in &self.entries {
            if tpm.is_some()
                && e.active
                && !e.metadata
                && if e.started {
                    !live(e.tpm_until, at)
                } else {
                    e.tokens == 0
                }
            {
                return false;
            }
            rpm_at += u128::from(!e.started || live(e.rpm_until, at));
            if !e.started || live(e.tpm_until, at) {
                tpm_at += e.tokens;
            }
        }
        known(&self.quota.rpm).is_none_or(|limit| rpm_at + 2 <= limit)
            && tpm.is_none_or(|limit| tpm_at + head.tokens() + candidate.tokens() <= limit)
    }
    pub fn admit(&mut self, now: Duration, cost: RequestCost) -> Decision {
        match self.check(now, cost) {
            Err("estimate_exceeds_budget") => return Decision::EstimateExceedsBudget,
            Err("concurrency" | "identity_exhausted") => return Decision::Wait(None),
            Err(_) => return Decision::Wait(self.next_wake()),
            Ok(()) => {}
        }
        let tokens = cost.tokens();
        // Identity exhaustion never wraps into an old request's ownership.
        let Some(next) = self.next.checked_add(1) else {
            return Decision::Wait(None);
        };
        self.next = next;
        let id = ReservationId(next);
        self.entries.push(Entry {
            id,
            active: true,
            started: false,
            metadata: matches!(cost.0, Cost::Metadata),
            tokens: if known(&self.quota.tpm).is_some() {
                tokens
            } else {
                0
            },
            rpm_until: None,
            tpm_until: None,
        });
        Decision::Admitted(id)
    }
    pub fn start(&mut self, now: Duration, id: ReservationId) -> bool {
        self.advance(now);
        let Some(e) = self
            .entries
            .iter_mut()
            .find(|e| e.id == id && e.active && !e.started)
        else {
            return false;
        };
        e.started = true;
        let until = self.now.saturating_add(WINDOW);
        e.rpm_until = known(&self.quota.rpm).map(|_| until);
        e.tpm_until = known(&self.quota.tpm)
            .filter(|_| e.tokens > 0)
            .map(|_| until);
        self.starts = self.starts.saturating_add(1);
        true
    }
    pub fn cancel(&mut self, now: Duration, id: ReservationId) -> bool {
        self.advance(now);
        let Some(index) = self
            .entries
            .iter()
            .position(|e| e.id == id && e.active && !e.started)
        else {
            return false;
        };
        self.entries.swap_remove(index);
        self.cleanups = self.cleanups.saturating_add(1);
        true
    }
    pub fn finish(&mut self, now: Duration, id: ReservationId, usage: Option<u64>) -> bool {
        self.advance(now);
        let Some(e) = self
            .entries
            .iter_mut()
            .find(|e| e.id == id && e.active && e.started)
        else {
            return false;
        };
        e.active = false;
        if !e.metadata && known(&self.quota.tpm).is_some() {
            if let Some(usage) = usage {
                let d = &mut self.reservation;
                let observed = u128::from(usage);
                d.samples = d.samples.saturating_add(1);
                d.reserved_tokens = d.reserved_tokens.saturating_add(e.tokens);
                d.observed_tokens = d.observed_tokens.saturating_add(observed);
                d.excess_tokens = d
                    .excess_tokens
                    .saturating_add(e.tokens.saturating_sub(observed));
                d.shortfall_tokens = d
                    .shortfall_tokens
                    .saturating_add(observed.saturating_sub(e.tokens));
                if live(e.tpm_until, self.now) {
                    if self.accounting == Accounting::Actual {
                        e.tokens = u128::from(usage);
                    }
                } else if usage > 0 {
                    // Long generation: observed usage is charged now, never credited
                    // against a fresh window using an expired reservation.
                    e.tokens = u128::from(usage);
                    e.tpm_until = Some(self.now.saturating_add(WINDOW));
                }
            } else {
                self.reservation.unknown = self.reservation.unknown.saturating_add(1);
            }
        }
        self.cleanups = self.cleanups.saturating_add(1);
        self.advance(now);
        true
    }
}

#[cfg(test)]
mod diagnostics_tests {
    use super::*;

    #[test]
    fn cumulative_diagnostics_saturate_without_wrapping() {
        let mut ledger = Ledger::new(
            Quota {
                rpm: Limit::Unlimited,
                tpm: Limit::Known(100.try_into().unwrap()),
            },
            Accounting::Actual,
            2,
            Duration::ZERO,
            Duration::ZERO,
        );
        ledger.reservation = ReservationDiagnostics {
            samples: u64::MAX,
            unknown: u64::MAX,
            reserved_tokens: u128::MAX - 1,
            observed_tokens: u128::MAX - 1,
            excess_tokens: u128::MAX - 1,
            shortfall_tokens: u128::MAX - 1,
        };
        for (at, cost, usage) in [(0, 100, Some(2)), (60, 0, Some(100)), (120, 0, None)] {
            let now = Duration::from_secs(at);
            let Decision::Admitted(id) = ledger.admit(now, RequestCost::exact_fixture(cost)) else {
                panic!("fixture admission")
            };
            ledger.start(now, id);
            ledger.finish(now, id, usage);
        }
        let d = ledger.snapshot(Duration::from_secs(120)).reservation;
        assert_eq!((d.samples, d.unknown), (u64::MAX, u64::MAX));
        assert_eq!(
            (
                d.reserved_tokens,
                d.observed_tokens,
                d.excess_tokens,
                d.shortfall_tokens
            ),
            (u128::MAX, u128::MAX, u128::MAX, u128::MAX)
        );
    }
}

#[cfg(all(test, feature = "bench-harness"))]
mod backfill_tests {
    use super::*;
    fn at(seconds: u64) -> Duration {
        Duration::from_secs(seconds)
    }
    fn cost(tokens: u64) -> RequestCost {
        RequestCost::exact_fixture(tokens)
    }
    fn known_limit(n: u64) -> Limit {
        Limit::Known(n.try_into().unwrap())
    }
    fn fresh() -> Ledger {
        Ledger::new(
            Quota {
                rpm: known_limit(u64::MAX),
                tpm: known_limit(100),
            },
            Accounting::Reserved,
            4,
            at(0),
            std::time::Duration::from_secs(60),
        )
    }
    fn reserve(l: &mut Ledger, now: u64, tokens: u64) -> ReservationId {
        let Decision::Admitted(id) = l.admit(at(now), cost(tokens)) else {
            panic!("fixture admission")
        };
        id
    }
    fn seeded() -> Ledger {
        let mut l = fresh();
        let id = reserve(&mut l, 60, 70);
        assert!(l.start(at(60), id));
        assert!(l.finish(at(60), id, None));
        l
    }
    #[test]
    fn projection_is_strict_bounded_and_never_advances_to_the_future() {
        let mut l = fresh();
        assert!(
            !l.can_backfill(at(59), cost(80), at(180), cost(20)),
            "startup hold"
        );
        let mut l = seeded();
        assert!(
            !l.can_backfill(at(60), cost(80), at(180), cost(20)),
            "equality at one window is unsafe"
        );
        let mut equality = seeded();
        let candidate = reserve(&mut equality, 60, 20);
        equality.start(at(60), candidate);
        equality.finish(at(120), candidate, Some(30));
        assert!(
            equality.check(at(120), cost(80)).is_err(),
            "at exactly one window the candidate can recharge before the head"
        );
        assert!(
            !l.can_backfill(at(65), cost(80), at(120), cost(20)),
            "deadline equality"
        );
        assert!(l.can_backfill(at(65), cost(80), at(180), cost(20)));
        assert!(
            l.can_backfill(at(0), cost(80), at(180), cost(20)),
            "use normalized ledger time"
        );
        assert_eq!(l.now, at(65));
        let s = l.snapshot(at(65));
        assert_eq!(
            (s.retained, s.active, s.tpm_debited, s.tpm_held),
            (1, 0, 70, 0)
        );
        let mut empty = fresh();
        assert!(
            !empty.can_backfill(at(65), cost(80), at(180), cost(20)),
            "no expiry to protect"
        );
    }
    #[test]
    fn absent_expired_and_future_recharge_ambiguity_decline() {
        let mut l = seeded();
        assert!(!l.can_backfill(at(65), cost(80), at(180), cost(0)));
        let zero = reserve(&mut l, 65, 0);
        assert!(
            !l.can_backfill(at(65), cost(80), at(180), cost(20)),
            "unstarted zero may recharge after starting"
        );
        l.start(at(65), zero);
        assert!(
            !l.can_backfill(at(65), cost(80), at(180), cost(20)),
            "started zero has no expiry"
        );
        l.finish(at(66), zero, Some(31));
        assert!(
            l.check(at(120), cost(80)).is_err(),
            "zero cost alone was not a safe bound"
        );

        let mut l = fresh();
        let long = reserve(&mut l, 60, 70);
        l.start(at(60), long);
        assert!(
            !l.can_backfill(at(65), cost(80), at(180), cost(20)),
            "active debit expires exactly at projected opportunity"
        );
        let seed = reserve(&mut l, 121, 70);
        l.start(at(121), seed);
        l.finish(at(121), seed, None);
        assert!(
            !l.can_backfill(at(126), cost(80), at(240), cost(20)),
            "expired active debit may recharge"
        );
        l.finish(at(127), long, Some(31));
        assert!(l.check(at(181), cost(80)).is_err());
    }
    #[test]
    fn metadata_backfill_is_safe_before_start_and_while_active() {
        let mut l = seeded();
        for _ in 0..3 {
            assert!(l.can_backfill(at(65), cost(80), at(180), RequestCost::Metadata));
            let Decision::Admitted(id) = l.admit(at(65), RequestCost::Metadata) else {
                panic!("metadata admission")
            };
            if l.snapshot(at(65)).active < 3 {
                assert!(l.can_backfill(at(65), cost(80), at(180), cost(20)));
            }
            assert!(l.start(at(65), id));
        }
        assert!(
            !l.can_backfill(at(65), cost(80), at(180), RequestCost::Metadata),
            "metadata still reserves a slot for the protected head"
        );
        assert!(l.check(at(120), cost(80)).is_ok());
        assert_eq!(l.snapshot(at(120)).tpm_debited, 0);

        let mut l = seeded();
        l.quota.rpm = known_limit(3);
        for _ in 0..2 {
            assert!(l.can_backfill(at(65), cost(80), at(180), RequestCost::Metadata));
            let Decision::Admitted(id) = l.admit(at(65), RequestCost::Metadata) else {
                panic!("metadata admission")
            };
            l.start(at(65), id);
            l.finish(at(66), id, Some(500));
        }
        assert!(
            !l.can_backfill(at(66), cost(80), at(180), RequestCost::Metadata),
            "metadata still consumes RPM cumulatively"
        );
        assert!(l.check(at(120), cost(80)).is_ok());
        assert_eq!(l.snapshot(at(120)).tpm_debited, 0);
    }
    #[test]
    fn projection_counts_unstarted_tokens_and_surviving_rpm_jointly() {
        let mut l = seeded();
        let pending = reserve(&mut l, 65, 21);
        assert!(!l.can_backfill(at(65), cost(80), at(180), cost(1)));
        l.cancel(at(65), pending);
        let pending = reserve(&mut l, 65, 10);
        assert!(l.can_backfill(at(65), cost(80), at(180), cost(10)));
        assert_eq!(
            (l.snapshot(at(65)).retained, l.snapshot(at(65)).tpm_held),
            (2, 10)
        );
        l.start(at(90), pending);
        assert!(
            l.can_backfill(at(90), cost(80), at(180), cost(10)),
            "delayed start stays charged across T"
        );

        let mut l = fresh();
        l.quota.rpm = known_limit(3);
        let delayed = reserve(&mut l, 60, 1);
        l.start(at(60), delayed);
        l.finish(at(121), delayed, Some(70));
        for now in [122, 123] {
            let metadata = reserve(&mut l, now, 0);
            l.start(at(now), metadata);
            l.finish(at(now), metadata, None);
        }
        assert!(
            l.check(at(126), cost(20)).is_ok(),
            "candidate fits current RPM/TPM"
        );
        assert!(
            !l.can_backfill(at(126), cost(80), at(240), cost(20)),
            "both surviving RPM debits leave only one future admission"
        );
        assert_eq!(l.snapshot(at(126)).retained, 3);
    }
    #[test]
    fn reservation_identity_and_entry_capacity_leave_two_positions() {
        let mut l = seeded();
        l.next = u64::MAX - 1;
        assert!(l.check(at(65), cost(20)).is_ok());
        assert!(!l.can_backfill(at(65), cost(80), at(180), cost(20)));
        l.next = u64::MAX - 2;
        assert!(l.can_backfill(at(65), cost(80), at(180), cost(20)));
        // Construct the bounded retained-debit boundary without 8192 repeated admission scans.
        l.entries.extend((2..=MAX_ENTRIES - 1).map(|id| Entry {
            id: ReservationId(id as u64),
            active: false,
            started: true,
            metadata: false,
            tokens: 0,
            rpm_until: Some(at(125)),
            tpm_until: None,
        }));
        assert_eq!(l.snapshot(at(65)).retained, MAX_ENTRIES - 1);
        assert!(l.check(at(65), cost(20)).is_ok());
        assert!(!l.can_backfill(at(65), cost(80), at(180), cost(20)));
        l.entries.pop();
        assert_eq!(l.snapshot(at(65)).retained, MAX_ENTRIES - 2);
        assert!(l.can_backfill(at(65), cost(80), at(180), cost(20)));
    }

    #[test]
    #[ignore = "profiling only: run explicitly in release mode after other builds and workloads stop"]
    fn profile_full_ledger_admission_scans() {
        assert!(!cfg!(debug_assertions), "profiling requires --release");
        const BATCHES: usize = 25;
        const ITERATIONS_PER_BATCH: usize = 16;
        const WARMUP_CALLS: usize = 32;
        let mut rows = Vec::new();
        for retained in [0, 1, MAX_ENTRIES - 2, MAX_ENTRIES] {
            for root_heads in [1, 16] {
                for projection in [false, true] {
                    let mut l = if retained == 0 { fresh() } else { seeded() };
                    l.entries.extend((l.entries.len()..retained).map(|i| Entry {
                        id: ReservationId(i as u64 + 1),
                        active: false,
                        started: true,
                        metadata: false,
                        tokens: 0,
                        rpm_until: Some(at(125)),
                        tpm_until: None,
                    }));
                    l.next = retained as u64;
                    assert_eq!(l.snapshot(at(65)).retained, retained);
                    let expected = retained < MAX_ENTRIES && (!projection || retained > 0);
                    let mut call = || {
                        let ledger = std::hint::black_box(&mut l);
                        if projection {
                            ledger.can_backfill(at(65), cost(80), at(180), cost(20))
                        } else {
                            ledger.check(at(65), cost(20)).is_ok()
                        }
                    };
                    assert_eq!(call(), expected, "verify the intended admission path");
                    for _ in 0..WARMUP_CALLS {
                        std::hint::black_box(call());
                    }
                    let mut batch_ns = Vec::with_capacity(BATCHES);
                    let mut successes = 0;
                    for _ in 0..BATCHES {
                        let start = std::time::Instant::now();
                        for _ in 0..ITERATIONS_PER_BATCH {
                            // Sequential root-head eligibility checks, not a full Queue::drive.
                            for _ in 0..root_heads {
                                successes += usize::from(std::hint::black_box(call()));
                            }
                        }
                        batch_ns.push(start.elapsed().as_nanos() as u64);
                    }
                    let calls = BATCHES * ITERATIONS_PER_BATCH * root_heads;
                    assert_eq!(successes, if expected { calls } else { 0 });
                    batch_ns.sort_unstable();
                    rows.push(serde_json::json!({
                        "retained": retained, "root_heads_simulated": root_heads,
                        "operation": if projection { "can_backfill" } else { "check" },
                        "calls": calls, "successful_calls": successes,
                        "successful_projections": if projection { successes } else { 0 },
                        "mean_ns_per_call": batch_ns.iter().sum::<u64>() as f64 / calls as f64,
                        "p95_batch_ns_per_call": batch_ns[(BATCHES * 95).div_ceil(100) - 1] as f64 / (ITERATIONS_PER_BATCH * root_heads) as f64,
                        "p95_batch_ns_per_root_scan": batch_ns[(BATCHES * 95).div_ceil(100) - 1] as f64 / ITERATIONS_PER_BATCH as f64
                    }));
                }
            }
        }
        println!(
            "{}",
            serde_json::json!({
                "kind": "ledger_scan_profile", "entry_size_bytes": std::mem::size_of::<Entry>(),
                "batches": BATCHES, "iterations_per_batch": ITERATIONS_PER_BATCH,
                "warmup_calls_per_case": WARMUP_CALLS,
                "scope": "fixed-time ledger calls only; 1 or 16 sequential identical root heads; excludes queue selection, locks and HTTP",
                "cases": rows
            })
        );
    }
}
