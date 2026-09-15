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

pub(super) struct Queue {
    pub ledger: Ledger,
    #[cfg(feature = "bench-harness")]
    pub policy: BenchmarkPolicy,
    pub cooldown_until: Duration,
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
                    fits[root] = self.ledger.check(now, head.cost).is_ok();
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
