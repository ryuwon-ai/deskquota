//! Deterministic admission traces, not wall-clock or provider performance evidence.
#![cfg(feature = "bench-harness")]

use llmgw::admission::quota::RequestCost;
use llmgw::admission::{Admission, BenchmarkPolicy, Hold, ManualClock};
use llmgw::config::{
    Accounting, Auth, CancelPolicy, Config, Endpoint, Limit, Quota, Root, Upstream,
};
use std::time::Duration;
use tokio::task::JoinHandle;

fn config(cap: u8) -> Config {
    Config {
        listen: "127.0.0.1:0".parse().unwrap(),
        concurrency: cap,
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
            tpm: Limit::Known(100.try_into().unwrap()),
        },
        models: vec![],
        roots: (0..4)
            .map(|n| Root {
                id: format!("root{n}"),
                endpoints: vec![Endpoint::Models],
                models: vec![],
            })
            .collect(),
    }
}
async fn turn() {
    for _ in 0..10 {
        tokio::task::yield_now().await;
    }
}
fn request(
    a: &Admission,
    root: usize,
    cost: u64,
) -> JoinHandle<Result<Hold, llmgw::admission::AcquireError>> {
    let a = a.clone();
    tokio::spawn(async move {
        a.acquire(
            root,
            RequestCost::exact_fixture(cost),
            Endpoint::Models,
            Duration::from_secs(120),
        )
        .await
    })
}
async fn admitted(t: JoinHandle<Result<Hold, llmgw::admission::AcquireError>>) -> Hold {
    assert!(
        t.is_finished(),
        "request must be admitted at this trace point"
    );
    t.await.unwrap().unwrap()
}
async fn seed(a: &Admission, n: u64) {
    let t = request(a, 3, n);
    turn().await;
    let mut hold = admitted(t).await;
    hold.start();
    drop(hold);
}

#[tokio::test]
async fn next_expiry_slack_admits_light_and_keeps_heavy_slot() {
    let clock = ManualClock::default();
    let a = Admission::benchmark(&config(2), Some(clock.clone()), BenchmarkPolicy::Backfill);
    clock.advance_to(Duration::from_secs(60));
    seed(&a, 70).await;
    let heavy = request(&a, 0, 80);
    turn().await;
    clock.advance_to(Duration::from_secs(65));
    turn().await;
    let light = request(&a, 1, 20);
    turn().await;
    let mut light = admitted(light).await;
    light.start();
    let extra = request(&a, 2, 1);
    turn().await;
    assert!(!extra.is_finished());
    clock.advance_to(Duration::from_secs(120));
    turn().await;
    let mut heavy = admitted(heavy).await;
    heavy.start();
    assert_eq!(
        a.snapshot().active,
        2,
        "light need not finish to preserve the heavy slot"
    );
    assert_eq!(a.snapshot().retained, 2);
    assert!(!extra.is_finished());
    extra.abort();
    assert!(matches!(extra.await, Err(error) if error.is_cancelled()));
    drop(heavy);
    drop(light);
    turn().await;
    assert_eq!(a.snapshot().active, 0);
    assert_eq!(a.snapshot().starts, 3);
}

#[tokio::test]
async fn cap_one_actual_rpm_and_no_slack_keep_the_barrier() {
    for case in ["cap_one", "actual", "rpm", "no_slack", "baseline"] {
        let mut cfg = config(if case == "cap_one" { 1 } else { 4 });
        if case == "actual" {
            cfg.accounting = Accounting::Actual;
        }
        if case == "rpm" {
            cfg.quota.rpm = Limit::Known(1.try_into().unwrap());
        }
        let clock = ManualClock::default();
        let policy = if case == "baseline" {
            BenchmarkPolicy::Rr
        } else {
            BenchmarkPolicy::Backfill
        };
        let a = Admission::benchmark(&cfg, Some(clock.clone()), policy);
        clock.advance_to(Duration::from_secs(60));
        seed(&a, 70).await;
        let heavy = request(&a, 0, 80);
        turn().await;
        clock.advance_to(Duration::from_secs(65));
        turn().await;
        let light = request(&a, 1, if case == "no_slack" { 21 } else { 20 });
        turn().await;
        assert!(!light.is_finished(), "{case} must not backfill");
        assert_eq!(a.snapshot().retained, 1);
        clock.advance_to(Duration::from_secs(120));
        turn().await;
        let mut heavy = admitted(heavy).await;
        heavy.start();
        light.abort();
        let _ = light.await;
        drop(heavy);
        turn().await;
        assert_eq!(a.snapshot().active, 0);
    }
}

#[tokio::test]
async fn cumulative_backfill_cannot_spend_the_protected_quota() {
    let clock = ManualClock::default();
    let a = Admission::benchmark(&config(4), Some(clock.clone()), BenchmarkPolicy::Backfill);
    clock.advance_to(Duration::from_secs(60));
    seed(&a, 70).await;
    let heavy = request(&a, 0, 80);
    turn().await;
    clock.advance_to(Duration::from_secs(65));
    turn().await;
    let mut lights = vec![];
    for _ in 0..2 {
        let light = request(&a, 1, 10);
        turn().await;
        let mut light = admitted(light).await;
        light.start();
        lights.push(light);
    }
    let mut blocked = vec![];
    for _ in 0..9 {
        let extra = request(&a, 2, 1);
        turn().await;
        assert!(
            !extra.is_finished(),
            "future slack must be recomputed cumulatively"
        );
        blocked.push(extra);
    }
    let s = a.snapshot();
    assert_eq!((s.retained, s.active, s.tpm_debited), (3, 2, 90));
    clock.advance_to(Duration::from_secs(120));
    turn().await;
    let mut heavy = admitted(heavy).await;
    heavy.start();
    assert_eq!(a.snapshot().tpm_debited, 100);
    for task in blocked {
        assert!(!task.is_finished());
        task.abort();
        let _ = task.await;
    }
    drop(heavy);
    drop(lights);
    turn().await;
    assert_eq!((a.snapshot().active, a.snapshot().starts), (0, 4));
}

#[tokio::test]
async fn unpolled_candidate_keeps_its_hold_and_cancels_exactly_once() {
    use std::future::Future;
    use std::task::{Context, Poll};
    let clock = ManualClock::default();
    let a = Admission::benchmark(&config(2), Some(clock.clone()), BenchmarkPolicy::Backfill);
    clock.advance_to(Duration::from_secs(60));
    seed(&a, 70).await;
    let heavy = request(&a, 0, 80);
    turn().await;
    a.cooldown(Duration::from_secs(5));
    let mut light = Box::pin(a.acquire(
        1,
        RequestCost::exact_fixture(20),
        Endpoint::Models,
        Duration::from_secs(120),
    ));
    let waker = futures_util::task::noop_waker();
    let mut cx = Context::from_waker(&waker);
    assert!(matches!(light.as_mut().poll(&mut cx), Poll::Pending));
    clock.advance_to(Duration::from_secs(64));
    turn().await;
    assert_eq!(a.snapshot().active, 0, "shared cooldown still applies");
    clock.advance_to(Duration::from_secs(65));
    turn().await;
    assert_eq!(
        (
            a.snapshot().active,
            a.snapshot().tpm_held,
            a.snapshot().retained
        ),
        (1, 20, 2)
    );
    clock.advance_to(Duration::from_secs(120));
    turn().await;
    let mut heavy = admitted(heavy).await;
    heavy.start();
    assert_eq!((a.snapshot().active, a.snapshot().tpm_held), (2, 20));
    drop(light);
    drop(heavy);
    turn().await;
    assert_eq!(
        (
            a.snapshot().active,
            a.snapshot().starts,
            a.snapshot().cleanups
        ),
        (0, 2, 3)
    );
}

#[tokio::test]
async fn protected_cancellation_and_deadline_release_other_root() {
    for cancel in [false, true] {
        let clock = ManualClock::default();
        let a = Admission::benchmark(&config(4), Some(clock.clone()), BenchmarkPolicy::Backfill);
        clock.advance_to(Duration::from_secs(60));
        seed(&a, 70).await;
        let aa = a.clone();
        let heavy = tokio::spawn(async move {
            aa.acquire(
                0,
                RequestCost::exact_fixture(80),
                Endpoint::Models,
                Duration::from_secs(10),
            )
            .await
        });
        turn().await;
        clock.advance_to(Duration::from_secs(65));
        turn().await;
        let light = request(&a, 1, 20);
        turn().await;
        assert!(
            !light.is_finished(),
            "projected opportunity is after protected deadline"
        );
        if cancel {
            heavy.abort();
            assert!(matches!(heavy.await, Err(error) if error.is_cancelled()));
        } else {
            clock.advance_to(Duration::from_secs(70));
            turn().await;
            assert!(matches!(
                heavy.await.unwrap(),
                Err(llmgw::admission::AcquireError::Deadline)
            ));
        }
        turn().await;
        drop(admitted(light).await);
        assert_eq!((a.snapshot().starts, a.snapshot().active), (1, 0));
    }
}

#[tokio::test]
async fn backfill_preserves_both_root_fifo_arrival_orders() {
    for costs in [[80, 10, 80], [80, 80, 10]] {
        let clock = ManualClock::default();
        let a = Admission::benchmark(&config(4), Some(clock.clone()), BenchmarkPolicy::Backfill);
        clock.advance_to(Duration::from_secs(60));
        seed(&a, 70).await;
        let first = request(&a, 0, costs[0]);
        turn().await;
        clock.advance_to(Duration::from_secs(65));
        turn().await;
        let second = request(&a, 0, costs[1]);
        turn().await;
        let third = request(&a, 0, costs[2]);
        turn().await;
        let light = request(&a, 1, 20);
        turn().await;
        let mut light = admitted(light).await;
        light.start();
        assert!(
            !second.is_finished() && !third.is_finished(),
            "no in-root skipping"
        );
        clock.advance_to(Duration::from_secs(120));
        turn().await;
        let mut first = admitted(first).await;
        first.start();
        drop(first);
        turn().await;
        assert!(!second.is_finished() && !third.is_finished());
        clock.advance_to(Duration::from_secs(125));
        drop(light);
        turn().await;
        assert_eq!(second.is_finished(), costs[1] == 10);
        assert!(!third.is_finished());
        if costs[1] == 10 {
            let mut second = admitted(second).await;
            second.start();
            drop(second);
            clock.advance_to(Duration::from_secs(180));
            turn().await;
        } else {
            clock.advance_to(Duration::from_secs(180));
            turn().await;
            let mut second = admitted(second).await;
            second.start();
            drop(second);
            turn().await;
        }
        let mut third = admitted(third).await;
        third.start();
        drop(third);
        assert_eq!((a.snapshot().starts, a.snapshot().active), (5, 0));
    }
}
