//! Root FIFO + request round robin, with a bounded bypass/age barrier.
//! This owner contains the only ledger; considering a head never reserves resources.
use super::AcquireError;
#[cfg(feature = "bench-harness")]
use super::BenchmarkPolicy;
use super::quota::{Decision, Ledger, RequestCost, ReservationId};
use std::collections::VecDeque;
use std::sync::{Arc, Mutex};
use std::time::Duration;

pub const MAX_WAITING: usize = 64;
const MAX_BYPASSES: u8 = 8;
const MAX_AGE: Duration = Duration::from_secs(5);
pub(super) type ResultCell = Arc<Mutex<Option<Result<ReservationId, AcquireError>>>>;
struct Waiting {
    result: ResultCell,
    cost: RequestCost,
    enqueued: Duration,
    deadline: Duration,
    sequence: u64,
    bypasses: u8,
}

struct Recovery {
    generation_only: bool,
    probe: Option<ReservationId>,
}

pub(super) struct Queue {
    pub ledger: Ledger,
    #[cfg(feature = "bench-harness")]
    pub policy: BenchmarkPolicy,
    pub cooldown_until: Duration,
    pub generation: u64,
    recovery: Option<Recovery>,
    no_hint_stage: usize,
    roots: Vec<VecDeque<Waiting>>,
    cursor: usize,
    sequence: u64,
    barrier: Option<(usize, u64)>,
}
impl Queue {
    pub fn new(ledger: Ledger, roots: usize) -> Self {
        Self {
            ledger,
            #[cfg(feature = "bench-harness")]
            policy: BenchmarkPolicy::Rr,
            cooldown_until: Duration::ZERO,
            generation: 0,
            recovery: None,
            no_hint_stage: 0,
            roots: (0..roots).map(|_| VecDeque::new()).collect(),
            cursor: 0,
            sequence: 0,
            barrier: None,
        }
    }
    pub fn len(&self) -> usize {
        self.roots.iter().map(VecDeque::len).sum()
    }
    pub fn lengths(&self) -> Vec<usize> {
        self.roots.iter().map(VecDeque::len).collect()
    }
    pub fn barrier(&self) -> Option<usize> {
        self.barrier.map(|(root, _)| root)
    }
    fn eligible(&self, cost: RequestCost) -> bool {
        self.recovery.as_ref().is_none_or(|recovery| {
            (recovery.generation_only && !cost.is_generation()) || recovery.probe.is_none()
        })
    }
    pub fn can_start(&self, now: Duration, id: ReservationId, cost: RequestCost) -> bool {
        now >= self.cooldown_until
            && self.recovery.as_ref().is_none_or(|recovery| {
                (recovery.generation_only && !cost.is_generation()) || recovery.probe == Some(id)
            })
    }
    pub fn arm_recovery(&mut self, now: Duration, delay: Duration, generation_only: bool) {
        self.cooldown_until = self.cooldown_until.max(now.saturating_add(delay));
        let generation_only =
            generation_only || self.recovery.as_ref().is_some_and(|r| r.generation_only);
        self.generation = self.generation.wrapping_add(1);
        self.recovery = Some(Recovery {
            generation_only,
            probe: None,
        });
    }
    pub fn throttle(
        &mut self,
        now: Duration,
        generation: u64,
        cost: RequestCost,
        has_timing: bool,
    ) {
        // Old error bodies cannot multiply backoff or replace a newer recovery probe.
        if generation != self.generation {
            if cost.is_generation() && self.recovery.as_ref().is_some_and(|r| !r.generation_only) {
                // Deduplicate timing, not eligibility: an older generation429 still
                // invalidates a metadata probe's claim to prove generation recovery.
                let keep_probe = self
                    .recovery
                    .as_ref()
                    .and_then(|r| r.probe)
                    .is_some_and(|id| self.ledger.is_active_generation(id));
                let recovery = self.recovery.as_mut().expect("checked active recovery");
                recovery.generation_only = true;
                if !keep_probe {
                    self.generation = self.generation.wrapping_add(1);
                    recovery.probe = None;
                }
            }
            return;
        }
        let delay = if has_timing {
            // Hold::headers already applied the server's wait to the shared deadline.
            Duration::ZERO
        } else {
            const SECONDS: [u64; 6] = [1, 3, 5, 10, 20, 30];
            let delay = Duration::from_secs(SECONDS[self.no_hint_stage])
                + Duration::from_millis(rand::random_range(0..=250));
            self.no_hint_stage = (self.no_hint_stage + 1).min(SECONDS.len() - 1);
            delay
        };
        self.arm_recovery(now, delay, cost.is_generation());
    }
    pub fn accepted(&mut self, now: Duration, id: ReservationId, generation: u64) {
        if generation == self.generation
            && now >= self.cooldown_until
            && self.recovery.as_ref().is_some_and(|r| r.probe == Some(id))
        {
            self.recovery = None;
            self.no_hint_stage = 0;
        }
    }
    pub fn release_probe(&mut self, id: ReservationId) {
        if let Some(recovery) = self.recovery.as_mut() {
            if recovery.probe == Some(id) {
                recovery.probe = None;
            }
        }
    }
    pub fn enqueue(
        &mut self,
        root: usize,
        cost: RequestCost,
        enqueued: Duration,
        deadline: Duration,
    ) -> Result<ResultCell, AcquireError> {
        if root >= self.roots.len() {
            return Err(AcquireError::InvalidRoot);
        }
        if !self.ledger.total_fits(cost) {
            return Err(AcquireError::EstimateExceedsBudget);
        }
        if self.len() >= MAX_WAITING {
            return Err(AcquireError::QueueFull);
        }
        self.sequence = self
            .sequence
            .checked_add(1)
            .ok_or(AcquireError::QueueFull)?;
        let result = Arc::new(Mutex::new(None));
        self.roots[root].push_back(Waiting {
            result: result.clone(),
            cost,
            enqueued,
            deadline,
            sequence: self.sequence,
            bypasses: 0,
        });
        Ok(result)
    }
    pub fn cancel(&mut self, now: Duration, root: usize, result: &ResultCell) {
        if let Some(index) = self.roots[root]
            .iter()
            .position(|w| Arc::ptr_eq(&w.result, result))
        {
            self.roots[root].remove(index);
        }
        if let Some(Ok(id)) = result.lock().expect("ticket lock").take() {
            self.ledger.cancel(now, id);
            self.release_probe(id);
        }
        self.protect(now);
    }
    fn protect(&mut self, now: Duration) {
        #[cfg(feature = "bench-harness")]
        if self.policy == BenchmarkPolicy::Fifo {
            self.barrier = None;
            return;
        }
        if self.barrier.is_some_and(|(root, sequence)| {
            self.roots[root]
                .front()
                .is_some_and(|head| head.sequence == sequence)
        }) {
            return;
        }
        // All impossible costs are rejected at enqueue; quota capacities are immutable.
        let triggered = self
            .roots
            .iter()
            .filter_map(|q| q.front())
            .any(|h| h.bypasses >= MAX_BYPASSES || now.saturating_sub(h.enqueued) >= MAX_AGE);
        self.barrier = if triggered {
            self.roots
                .iter()
                .enumerate()
                .filter_map(|(root, q)| q.front().map(|head| (root, head)))
                .min_by_key(|(_, h)| (h.enqueued, h.sequence))
                .map(|(root, head)| (root, head.sequence))
        } else {
            None
        };
    }
    pub fn drive(&mut self, now: Duration) -> Option<Duration> {
        for q in &mut self.roots {
            q.retain(|w| {
                if now >= w.deadline {
                    *w.result.lock().expect("ticket lock") = Some(Err(AcquireError::Deadline));
                    false
                } else {
                    true
                }
            });
        }
        loop {
            self.protect(now);
            if now < self.cooldown_until {
                break;
            }
            // Inspect each root head once for this selection, before any reservation.
            let mut fits = [false; 16];
            for (root, q) in self.roots.iter().enumerate() {
                if let Some(head) = q.front() {
                    fits[root] =
                        self.ledger.check(now, head.cost).is_ok() && self.eligible(head.cost);
                }
            }
            let selected = if let Some((root, _)) = self.barrier {
                fits[root].then_some(root)
            } else {
                (0..self.roots.len())
                    .map(|offset| (self.cursor + offset) % self.roots.len())
                    .find(|root| fits[*root])
            };
            #[cfg(feature = "bench-harness")]
            let selected = if self.policy == BenchmarkPolicy::Fifo {
                self.roots
                    .iter()
                    .enumerate()
                    .filter_map(|(root, q)| q.front().map(|head| (root, head)))
                    .min_by_key(|(_, head)| (head.enqueued, head.sequence))
                    .and_then(|(root, _)| fits[root].then_some(root))
            } else if self.policy == BenchmarkPolicy::Backfill && selected.is_none() {
                self.barrier.and_then(|(protected, _)| {
                    let head = self.roots[protected].front().expect("protected head");
                    (0..self.roots.len())
                        .map(|offset| (self.cursor + offset) % self.roots.len())
                        .find(|root| {
                            *root != protected
                                && fits[*root]
                                && self.ledger.can_backfill(
                                    now,
                                    head.cost,
                                    head.deadline,
                                    self.roots[*root].front().expect("fitting head").cost,
                                )
                        })
                })
            } else {
                selected
            };
            let Some(root) = selected else {
                break;
            };
            // Normal RR turns between fit heads are not resource bypasses.
            for (other_root, q) in self.roots.iter_mut().enumerate() {
                if other_root != root && !fits[other_root] {
                    if let Some(other) = q.front_mut() {
                        other.bypasses = other.bypasses.saturating_add(1);
                    }
                }
            }
            let head = self.roots[root].pop_front().expect("selected head");
            let Decision::Admitted(id) = self.ledger.admit(now, head.cost) else {
                unreachable!("same-owner checked admission");
            };
            if let Some(recovery) = self.recovery.as_mut() {
                if !recovery.generation_only || head.cost.is_generation() {
                    recovery.probe = Some(id);
                }
            }
            *head.result.lock().expect("ticket lock") = Some(Ok(id));
            self.cursor = (root + 1) % self.roots.len();
        }
        if self.len() == 0 {
            return None;
        }
        let deadlines = self.roots.iter().flat_map(|q| q.iter().map(|w| w.deadline));
        let ages = self
            .roots
            .iter()
            .filter_map(|q| q.front())
            .map(|w| w.enqueued.saturating_add(MAX_AGE))
            .filter(|at| *at > now);
        let resource_wake = if now < self.cooldown_until {
            Some(self.cooldown_until)
        } else {
            self.ledger.next_wake()
        };
        deadlines.chain(ages).chain(resource_wake).min()
    }
    pub fn blocked_reason(&mut self, now: Duration) -> Option<&'static str> {
        if self.len() == 0 {
            return None;
        }
        if now < self.cooldown_until {
            return Some("upstream_cooldown");
        }
        if self
            .roots
            .iter()
            .filter_map(|q| q.front())
            .any(|head| !self.eligible(head.cost))
        {
            return Some("upstream_recovery");
        }
        if let Some((root, _)) = self.barrier {
            return self.roots[root]
                .front()
                .and_then(|head| self.ledger.check(now, head.cost).err());
        }
        self.roots
            .iter()
            .filter_map(|q| q.front())
            .find_map(|w| self.ledger.check(now, w.cost).err())
    }
}

#[cfg(test)]
mod diagnostics_tests {
    use super::*;
    use crate::config::{Accounting, Limit, Quota};

    fn recovery_queue() -> Queue {
        Queue::new(
            Ledger::new(
                Quota {
                    rpm: Limit::Unlimited,
                    tpm: Limit::Unknown,
                },
                Accounting::Reserved,
                4,
                Duration::ZERO,
                Duration::ZERO,
            ),
            2,
        )
    }
    fn enqueue(queue: &mut Queue, root: usize, cost: RequestCost, now: Duration) -> ResultCell {
        queue
            .enqueue(root, cost, now, now + Duration::from_secs(120))
            .unwrap()
    }

    #[test]
    fn no_hint_generations_back_off_once_and_only_one_generation_probe_recovers() {
        let mut queue = recovery_queue();
        let cost = RequestCost::exact_fixture(1);
        let mut now = Duration::ZERO;
        for seconds in [1, 3, 5, 10, 20, 30, 30] {
            let generation = queue.generation;
            queue.throttle(now, generation, cost, false);
            let until = queue.cooldown_until;
            assert!(until - now >= Duration::from_secs(seconds));
            assert!(until - now <= Duration::from_millis(seconds * 1000 + 250));
            queue.throttle(now, generation, cost, false);
            assert_eq!(
                queue.cooldown_until, until,
                "concurrent old429 cannot extend a no-hint generation"
            );
            now = until;
        }
        let generation = queue.generation;
        let first = enqueue(&mut queue, 0, cost, now);
        let second = enqueue(&mut queue, 0, cost, now);
        let metadata = enqueue(&mut queue, 1, RequestCost::Metadata, now);
        queue.drive(now);
        let id = first.lock().unwrap().take().unwrap().unwrap();
        let metadata_id = metadata.lock().unwrap().take().unwrap().unwrap();
        assert!(queue.can_start(now, id, cost));
        assert!(queue.can_start(now, metadata_id, RequestCost::Metadata));
        assert!(second.lock().unwrap().is_none());
        queue.accepted(now, metadata_id, generation);
        queue.accepted(now, id, generation - 1);
        queue.drive(now);
        assert!(
            second.lock().unwrap().is_none(),
            "metadata and stale success cannot clear a generation gate"
        );
        queue.accepted(now, id, generation);
        queue.drive(now);
        assert!(second.lock().unwrap().is_some());
        assert_eq!(queue.no_hint_stage, 0);
    }

    #[test]
    fn canceled_or_failed_probe_releases_ownership_and_new_feedback_survives_success() {
        let mut queue = recovery_queue();
        let now = Duration::ZERO;
        let cost = RequestCost::Metadata;
        queue.arm_recovery(now, Duration::ZERO, false);
        let first = enqueue(&mut queue, 0, cost, now);
        let second = enqueue(&mut queue, 1, cost, now);
        queue.drive(now);
        assert!(first.lock().unwrap().is_some());
        assert!(second.lock().unwrap().is_none());
        queue.cancel(now, 0, &first);
        queue.drive(now);
        let id = second.lock().unwrap().take().unwrap().unwrap();
        let generation = queue.generation;
        let stage = queue.no_hint_stage;
        queue.release_probe(id);
        assert_eq!(queue.generation, generation);
        assert_eq!(queue.no_hint_stage, stage);
        let next = enqueue(&mut queue, 0, cost, now);
        queue.drive(now);
        let next_id = next.lock().unwrap().take().unwrap().unwrap();
        queue.arm_recovery(now, Duration::from_secs(5), true);
        queue.accepted(now, next_id, generation);
        assert_eq!(queue.cooldown_until, Duration::from_secs(5));
        assert!(!queue.can_start(now, next_id, cost));
        assert!(queue.recovery.as_ref().unwrap().generation_only);
    }

    #[test]
    fn metadata_at_same_root_head_does_not_claim_or_block_generation_probe() {
        let mut queue = recovery_queue();
        let now = Duration::ZERO;
        queue.arm_recovery(now, Duration::ZERO, true);
        let metadata = enqueue(&mut queue, 0, RequestCost::Metadata, now);
        let generation = enqueue(&mut queue, 0, RequestCost::exact_fixture(1), now);
        let follower = enqueue(&mut queue, 0, RequestCost::exact_fixture(1), now);
        queue.drive(now);
        let metadata_id = metadata.lock().unwrap().take().unwrap().unwrap();
        let generation_id = generation.lock().unwrap().take().unwrap().unwrap();
        assert_eq!(queue.recovery.as_ref().unwrap().probe, Some(generation_id));
        queue.accepted(now, metadata_id, queue.generation);
        queue.drive(now);
        assert!(follower.lock().unwrap().is_none());
        queue.accepted(now, generation_id, queue.generation);
        queue.drive(now);
        assert!(follower.lock().unwrap().is_some());
    }

    fn stale_generation_rejection_strengthens_metadata_gate(
        during_probe: bool,
        started: bool,
        has_timing: bool,
    ) {
        let mut queue = recovery_queue();
        let old_generation = queue.generation;
        queue.throttle(Duration::ZERO, old_generation, RequestCost::Metadata, false);
        let until = queue.cooldown_until;
        let stage = queue.no_hint_stage;
        let cost = RequestCost::exact_fixture(1);
        if !during_probe {
            queue.throttle(Duration::ZERO, old_generation, cost, has_timing);
        }
        let metadata = enqueue(&mut queue, 0, RequestCost::Metadata, until);
        queue.drive(until);
        let metadata_id = metadata.lock().unwrap().take().unwrap().unwrap();
        let metadata_generation = queue.generation;
        assert!(queue.can_start(until, metadata_id, RequestCost::Metadata));
        if started {
            assert!(queue.ledger.start(until, metadata_id));
        }
        if during_probe {
            assert_eq!(queue.recovery.as_ref().unwrap().probe, Some(metadata_id));
            queue.throttle(until, old_generation, cost, has_timing);
        }
        assert!(
            queue.recovery.as_ref().unwrap().generation_only,
            "stale generation429 must retain generation-only recovery eligibility"
        );
        assert_eq!(
            queue.recovery.as_ref().unwrap().probe,
            None,
            "metadata probe ownership must be invalidated when eligibility strengthens"
        );
        assert_eq!(queue.no_hint_stage, stage);
        assert_eq!(queue.cooldown_until, until);
        let generation = enqueue(&mut queue, 0, cost, until);
        let follower = enqueue(&mut queue, 0, cost, until);
        queue.drive(until);
        let generation_id = generation.lock().unwrap().take().unwrap().unwrap();
        queue.accepted(until, metadata_id, metadata_generation);
        queue.drive(until);
        assert!(
            follower.lock().unwrap().is_none(),
            "metadata200 cannot heal the stronger gate"
        );
        queue.accepted(until, generation_id, queue.generation);
        queue.drive(until);
        assert!(follower.lock().unwrap().is_some());
    }

    #[test]
    fn stale_generation_429_before_metadata_probe_strengthens_recovery_without_backoff() {
        for (started, has_timing) in [(false, false), (true, false), (false, true), (true, true)] {
            stale_generation_rejection_strengthens_metadata_gate(false, started, has_timing);
        }
    }

    #[test]
    fn stale_generation_429_during_metadata_probe_invalidates_its_recovery_ownership() {
        for (started, has_timing) in [(false, false), (true, false), (false, true), (true, true)] {
            stale_generation_rejection_strengthens_metadata_gate(true, started, has_timing);
        }
    }

    #[test]
    fn stale_generation_429_preserves_an_already_eligible_generation_probe() {
        for (started, has_timing) in [(false, false), (true, false), (false, true), (true, true)] {
            let mut queue = recovery_queue();
            let old_generation = queue.generation;
            queue.throttle(Duration::ZERO, old_generation, RequestCost::Metadata, false);
            let until = queue.cooldown_until;
            let stage = queue.no_hint_stage;
            let cost = RequestCost::exact_fixture(1);
            let probe = enqueue(&mut queue, 0, cost, until);
            let follower = enqueue(&mut queue, 0, cost, until);
            queue.drive(until);
            let probe_id = probe.lock().unwrap().take().unwrap().unwrap();
            let probe_generation = queue.generation;
            assert!(queue.can_start(until, probe_id, cost));
            if started {
                assert!(queue.ledger.start(until, probe_id));
            }
            queue.throttle(until, old_generation, cost, has_timing);
            queue.throttle(until, old_generation, cost, has_timing);
            assert!(queue.recovery.as_ref().unwrap().generation_only);
            assert_eq!(
                queue.recovery.as_ref().unwrap().probe,
                Some(probe_id),
                "an already-eligible generation probe must remain the sole recovery owner"
            );
            assert_eq!(queue.generation, probe_generation);
            assert_eq!(queue.no_hint_stage, stage);
            assert_eq!(queue.cooldown_until, until);
            queue.drive(until);
            assert!(
                follower.lock().unwrap().is_none(),
                "promotion cannot start a second generation probe"
            );
            if !started {
                assert!(queue.can_start(until, probe_id, cost));
                assert!(queue.ledger.start(until, probe_id));
            }
            queue.accepted(until, probe_id, probe_generation);
            queue.drive(until);
            assert!(
                follower.lock().unwrap().is_some(),
                "the retained generation probe can prove recovery"
            );
        }
    }

    #[test]
    fn protected_head_reports_resource_and_empty_cooldown_has_no_blocker() {
        let ledger = Ledger::new(
            Quota {
                rpm: Limit::Unlimited,
                tpm: Limit::Known(100.try_into().unwrap()),
            },
            Accounting::Reserved,
            2,
            Duration::ZERO,
            Duration::ZERO,
        );
        let mut queue = Queue::new(ledger, 2);
        let Decision::Admitted(id) = queue
            .ledger
            .admit(Duration::ZERO, RequestCost::exact_fixture(70))
        else {
            panic!("fixture admission")
        };
        queue.ledger.start(Duration::ZERO, id);
        queue.ledger.finish(Duration::ZERO, id, None);
        let now = Duration::from_secs(5);
        let ticket = queue
            .enqueue(
                0,
                RequestCost::exact_fixture(80),
                Duration::ZERO,
                Duration::from_secs(120),
            )
            .unwrap();
        let wake = queue.drive(now);
        assert_eq!(queue.barrier(), Some(0));
        assert_eq!(queue.blocked_reason(now), Some("tpm"));
        queue.cooldown_until = Duration::from_secs(9);
        assert_eq!(queue.blocked_reason(now), Some("upstream_cooldown"));
        queue.cooldown_until = Duration::ZERO;
        assert_eq!(queue.drive(now), wake);
        assert!(ticket.lock().unwrap().is_none());
        queue.cancel(now, 0, &ticket);
        queue.cooldown_until = Duration::from_secs(90);
        assert_eq!(queue.blocked_reason(now), None);
    }
}
