use llmgw::admission::quota::{Decision, Ledger, RequestCost};
use llmgw::config::{Accounting, Limit, Quota};
use std::time::Duration;
fn time(s: u64) -> Duration {
    Duration::from_secs(s)
}
fn known(n: u64) -> Limit {
    Limit::Known(n.try_into().unwrap())
}
fn ledger(rpm: Limit, tpm: Limit, mode: Accounting) -> Ledger {
    Ledger::new(
        Quota { rpm, tpm },
        mode,
        2,
        time(0),
        std::time::Duration::from_secs(60),
    )
}
fn admitted(l: &mut Ledger, at: u64, cost: u64) -> llmgw::admission::quota::ReservationId {
    match l.admit(time(at), RequestCost::exact_fixture(cost)) {
        Decision::Admitted(id) => id,
        other => panic!("expected admission: {other:?}"),
    }
}
#[test]
fn restart_hold_expires_exactly_at_sixty_seconds() {
    let mut l = ledger(known(2), Limit::Unknown, Accounting::Reserved);
    assert_eq!(
        l.admit(time(0), RequestCost::Metadata),
        Decision::Wait(Some(time(60)))
    );
    assert_eq!(
        l.admit(time(59), RequestCost::Metadata),
        Decision::Wait(Some(time(60)))
    );
    admitted(&mut l, 60, 0);
}
#[test]
fn rpm_debits_only_on_start_and_never_refunds_on_finish() {
    let mut l = ledger(known(1), Limit::Unknown, Accounting::Actual);
    let id = admitted(&mut l, 60, 0);
    assert_eq!(l.snapshot(time(60)).rpm_debited, 0);
    assert!(l.cancel(time(80), id));
    assert!(!l.cancel(time(80), id));
    let id = admitted(&mut l, 90, 0);
    assert!(l.start(time(100), id));
    assert!(!l.start(time(100), id));
    assert!(l.finish(time(101), id, Some(0)));
    assert!(!l.finish(time(101), id, Some(0)));
    assert_eq!(
        l.admit(time(159), RequestCost::Metadata),
        Decision::Wait(Some(time(160)))
    );
    admitted(&mut l, 160, 0);
}
#[test]
fn provisional_holds_never_expire_and_queued_candidates_hold_nothing() {
    let mut l = ledger(known(1), known(100), Accounting::Actual);
    let id = admitted(&mut l, 60, 70);
    assert_eq!(
        l.admit(time(500), RequestCost::exact_fixture(40)),
        Decision::Wait(None)
    );
    let snap = l.snapshot(time(500));
    assert_eq!(snap.active, 1);
    assert_eq!(snap.tpm_held, 70);
    assert!(l.cancel(time(500), id));
    assert_eq!(l.snapshot(time(500)).cleanups, 1);
    admitted(&mut l, 500, 100);
}
#[test]
fn reserved_retains_full_estimate_and_unknown_actual_does_too() {
    for mode in [Accounting::Reserved, Accounting::Actual] {
        let mut l = ledger(Limit::Unlimited, known(100), mode);
        let id = admitted(&mut l, 60, 90);
        l.start(time(60), id);
        l.finish(
            time(61),
            id,
            if mode == Accounting::Reserved {
                Some(2)
            } else {
                None
            },
        );
        assert_eq!(l.snapshot(time(61)).tpm_debited, 90);
        assert_eq!(
            l.admit(time(61), RequestCost::exact_fixture(20)),
            Decision::Wait(Some(time(120)))
        );
        admitted(&mut l, 120, 100);
    }
}
#[test]
fn actual_settles_once_and_excess_creates_debt() {
    let mut l = ledger(Limit::Unlimited, known(100), Accounting::Actual);
    let id = admitted(&mut l, 60, 90);
    l.start(time(60), id);
    l.finish(time(61), id, Some(20));
    assert_eq!(l.snapshot(time(61)).tpm_debited, 20);
    assert!(!l.finish(time(61), id, Some(0)));
    let next = admitted(&mut l, 61, 80);
    l.start(time(61), next);
    l.finish(time(62), next, Some(150));
    assert_eq!(l.snapshot(time(62)).tpm_debited, 170);
    assert_eq!(
        l.admit(time(62), RequestCost::exact_fixture(1)),
        Decision::Wait(Some(time(120)))
    );
    assert_eq!(
        l.admit(time(120), RequestCost::exact_fixture(1)),
        Decision::Wait(Some(time(121)))
    );
    admitted(&mut l, 121, 100);
}
#[test]
fn late_usage_is_new_debit_and_old_identity_cannot_refund_new_work() {
    for mode in [Accounting::Actual, Accounting::Reserved] {
        let mut l = ledger(Limit::Unlimited, known(100), mode);
        let old = admitted(&mut l, 60, 90);
        l.start(time(60), old);
        let new = admitted(&mut l, 120, 90);
        l.start(time(120), new);
        l.finish(time(121), old, Some(20));
        assert_eq!(l.snapshot(time(121)).tpm_debited, 110);
        assert!(!l.finish(time(122), old, Some(0)));
        l.finish(time(122), new, None);
        assert_eq!(l.snapshot(time(180)).tpm_debited, 20);
        assert_eq!(l.snapshot(time(181)).tpm_debited, 0);
    }
}
#[test]
fn unsupported_and_unknown_limits_are_not_zero_or_provider_unlimited_claims() {
    assert_ne!(Limit::Unknown, Limit::Unlimited);
    let mut l = ledger(Limit::Unknown, Limit::Unlimited, Accounting::Reserved);
    let a = admitted(&mut l, 0, u64::MAX);
    l.start(time(0), a);
    l.finish(time(0), a, None);
    assert_eq!(l.snapshot(time(0)).retained, 0);
    let mut l = ledger(Limit::Unknown, known(100), Accounting::Reserved);
    assert_eq!(
        l.admit(time(60), RequestCost::exact_fixture(101)),
        Decision::EstimateExceedsBudget
    );
    assert_eq!(l.snapshot(time(60)).active, 0);
}
#[test]
fn resume_recomputes_window_without_accumulated_burst() {
    let mut l = ledger(known(1), known(100), Accounting::Reserved);
    let id = admitted(&mut l, 60, 100);
    l.start(time(60), id);
    l.finish(time(61), id, None);
    let id = admitted(&mut l, 10000, 100);
    l.start(time(10000), id);
    l.finish(time(10000), id, None);
    assert_eq!(
        l.admit(time(10000), RequestCost::Metadata),
        Decision::Wait(Some(time(10060)))
    );
}

#[test]
fn maximum_retained_entries_wait_instead_of_losing_unexpired_debits() {
    use llmgw::admission::quota::MAX_ENTRIES;
    let mut l = ledger(known(100000), Limit::Unknown, Accounting::Reserved);
    for _ in 0..MAX_ENTRIES {
        let id = admitted(&mut l, 60, 0);
        assert!(l.start(time(60), id));
        assert!(l.finish(time(60), id, None));
    }
    assert_eq!(l.snapshot(time(60)).retained, MAX_ENTRIES);
    assert_eq!(
        l.admit(time(60), RequestCost::Metadata),
        Decision::Wait(Some(time(120)))
    );
    assert_eq!(l.snapshot(time(120)).retained, 0);
    let id = admitted(&mut l, 120, 0);
    l.start(time(120), id);
    assert_eq!(l.snapshot(time(120)).rpm_debited, 1);
}

#[test]
fn joint_admission_does_not_hold_quota_while_concurrency_is_full() {
    let mut l = ledger(known(10), known(1000), Accounting::Reserved);
    let a = admitted(&mut l, 60, 100);
    let b = admitted(&mut l, 60, 100);
    l.start(time(60), a);
    l.start(time(60), b);
    assert_eq!(
        l.admit(time(60), RequestCost::exact_fixture(100)),
        Decision::Wait(None)
    );
    let s = l.snapshot(time(60));
    assert_eq!(s.active, 2);
    assert_eq!(s.tpm_debited, 200);
    assert_eq!(s.tpm_held, 0);
    assert_eq!(s.rpm_debited, 2);
    l.finish(time(61), a, None);
    admitted(&mut l, 61, 100);
}

#[test]
fn expired_unknown_usage_has_no_fresh_credit_and_time_cannot_move_backwards() {
    let mut l = ledger(known(1), known(100), Accounting::Actual);
    let old = admitted(&mut l, 60, 100);
    l.start(time(60), old);
    l.finish(time(121), old, None);
    assert_eq!(l.snapshot(time(1)).tpm_debited, 0);
    let new = admitted(&mut l, 121, 100);
    l.start(time(121), new);
    assert!(!l.cancel(time(121), new));
    assert!(!l.finish(time(121), old, Some(0)));
    assert_eq!(l.snapshot(time(121)).tpm_debited, 100);
}

#[test]
fn estimated_cost_addition_cannot_wrap_and_metadata_needs_no_tpm() {
    let mut l = ledger(Limit::Unlimited, known(u64::MAX), Accounting::Reserved);
    assert_eq!(
        l.admit(time(60), RequestCost::estimated(1, u64::MAX)),
        Decision::EstimateExceedsBudget
    );
    admitted(&mut l, 60, 0);
}

#[test]
fn metadata_never_recharges_but_other_zero_costs_keep_late_usage_debt() {
    for mode in [Accounting::Reserved, Accounting::Actual] {
        for (cost, expected) in [
            (RequestCost::Metadata, 0),
            (RequestCost::exact_fixture(0), 31),
            (RequestCost::estimated(0, 0), 31),
        ] {
            let mut l = ledger(known(10), known(100), mode);
            let Decision::Admitted(id) = l.admit(time(60), cost) else {
                panic!("admission")
            };
            assert!(l.start(time(60), id));
            assert!(l.finish(time(121), id, Some(31)));
            assert_eq!(l.snapshot(time(121)).tpm_debited, expected);
            assert!(!l.finish(time(121), id, Some(31)));
        }
    }
}

#[test]
fn configurable_startup_hold_does_not_change_the_sixty_second_quota_window() {
    for seconds in [0, 5, 3600] {
        let mut l = Ledger::new(
            Quota {
                rpm: known(1),
                tpm: Limit::Unknown,
            },
            Accounting::Actual,
            1,
            time(10),
            time(seconds),
        );
        if seconds > 0 {
            assert_eq!(
                l.admit(time(10), RequestCost::Metadata),
                Decision::Wait(Some(time(10 + seconds)))
            );
        }
        let now = 10 + seconds;
        let id = admitted(&mut l, now, 0);
        assert!(l.start(time(now), id));
        assert!(l.finish(time(now), id, None));
        assert_eq!(
            l.admit(time(now + 59), RequestCost::Metadata),
            Decision::Wait(Some(time(now + 60)))
        );
        admitted(&mut l, now + 60, 0);
    }
    let mut unlimited = Ledger::new(
        Quota {
            rpm: Limit::Unlimited,
            tpm: Limit::Unknown,
        },
        Accounting::Actual,
        1,
        time(0),
        time(3600),
    );
    admitted(&mut unlimited, 0, 0);
}
