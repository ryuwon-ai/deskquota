//! ManualClock/exact_fixture traces; these times are not HTTP latency.
use llmgw::admission::quota::RequestCost;
use llmgw::admission::{Admission, Hold, ManualClock};
use llmgw::config::{
    Accounting, Auth, CancelPolicy, Config, Endpoint, Limit, Quota, Root, Upstream,
};
use std::time::Duration;
use tokio::task::JoinHandle;
fn config(cap: u8, tpm: u64, roots: usize) -> Config {
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
            tpm: Limit::Known(tpm.try_into().unwrap()),
        },
        models: vec![],
        roots: (0..roots)
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
async fn setup(cap: u8, tpm: u64, roots: usize) -> (Admission, ManualClock) {
    let clock = ManualClock::default();
    let a = Admission::new(&config(cap, tpm, roots), Some(clock.clone()));
    clock.advance_to(Duration::from_secs(60));
    (a, clock)
}
async fn admitted(t: JoinHandle<Result<Hold, llmgw::admission::AcquireError>>) -> Hold {
    assert!(
        t.is_finished(),
        "request must enter admission at this trace point"
    );
    t.await.unwrap().unwrap()
}

#[cfg(feature = "bench-harness")]
#[tokio::test]
async fn fifo_waits_for_global_oldest_nonfit_head_while_rr_bypasses() {
    for fifo in [false, true] {
        let clock = ManualClock::default();
        let cfg = config(2, 100, 2);
        let a = if fifo {
            Admission::benchmark(
                &cfg,
                Some(clock.clone()),
                llmgw::admission::BenchmarkPolicy::Fifo,
            )
        } else {
            Admission::new(&cfg, Some(clock.clone()))
        };
        clock.advance_to(Duration::from_secs(60));
        let seed = request(&a, 0, 60);
        turn().await;
        let mut seed = admitted(seed).await;
        seed.start();
        drop(seed);
        let heavy = request(&a, 0, 80);
        turn().await;
        let light = request(&a, 1, 10);
        turn().await;
        assert_eq!(
            light.is_finished(),
            !fifo,
            "FIFO must not bypass global oldest nonfit head"
        );
        if !fifo {
            drop(admitted(light).await);
            clock.advance_to(Duration::from_secs(120));
            turn().await;
            drop(admitted(heavy).await);
        } else {
            clock.advance_to(Duration::from_secs(120));
            turn().await;
            drop(admitted(heavy).await);
            turn().await;
            drop(admitted(light).await);
        }
        assert_eq!(a.snapshot().active, 0);
    }
}
#[tokio::test]
async fn cap_one_running_long_is_not_preempted_and_cancelled_waiter_never_starts() {
    let (a, _) = setup(1, 100, 2).await;
    let long = request(&a, 0, 80);
    turn().await;
    let mut long = admitted(long).await;
    long.start();
    let short = request(&a, 1, 1);
    turn().await;
    assert!(!short.is_finished());
    short.abort();
    assert!(matches!(short.await, Err(e) if e.is_cancelled()));
    assert_eq!(a.snapshot().starts, 1);
    drop(long);
    turn().await;
    assert_eq!(a.snapshot().active, 0);
}

fn policies() -> Vec<bool> {
    #[cfg(feature = "bench-harness")]
    return vec![false, true];
    #[cfg(not(feature = "bench-harness"))]
    vec![false]
}
fn policy_admission(fifo: bool, cfg: &Config, clock: ManualClock) -> Admission {
    #[cfg(feature = "bench-harness")]
    if fifo {
        return Admission::benchmark(cfg, Some(clock), llmgw::admission::BenchmarkPolicy::Fifo);
    }
    assert!(!fifo);
    Admission::new(cfg, Some(clock))
}
#[tokio::test]
async fn both_heavy_light_arrival_orders_account_for_long_timeout() {
    for fifo in policies() {
        for roots in [[0, 1, 0], [0, 0, 1]] {
            let c = ManualClock::default();
            let a = policy_admission(fifo, &config(2, 100, 2), c.clone());
            c.advance_to(Duration::from_secs(60));
            let seed = request(&a, 1, 50);
            turn().await;
            let mut seed = admitted(seed).await;
            seed.start();
            drop(seed);
            let mut tasks = Vec::new();
            for root in roots {
                tasks.push(request(&a, root, if root == 0 { 80 } else { 10 }));
                turn().await;
            }
            let light_index = roots.iter().position(|r| *r == 1).unwrap();
            assert_eq!(tasks[light_index].is_finished(), !fifo);
            // A completed join handle retains its reservation until consumed.
            // Start/drop light before advancing, as the HTTP worker would.
            let mut light = Some(tasks.remove(light_index));
            if !fifo {
                let mut h = admitted(light.take().unwrap()).await;
                h.start();
                drop(h);
            }
            c.advance_to(Duration::from_secs(120));
            turn().await;
            let mut heavy = admitted(tasks.remove(0)).await;
            heavy.start();
            drop(heavy);
            turn().await;
            let light_times_out = fifo && light_index == 2;
            if fifo && !light_times_out {
                let mut h = admitted(light.take().unwrap()).await;
                h.start();
                drop(h);
            }
            c.advance_to(Duration::from_secs(180));
            turn().await;
            assert!(matches!(
                tasks.remove(0).await.unwrap(),
                Err(llmgw::admission::AcquireError::Deadline)
            ));
            if light_times_out {
                assert!(matches!(
                    light.take().unwrap().await.unwrap(),
                    Err(llmgw::admission::AcquireError::Deadline)
                ));
            }
            // All four submissions end: FIFO HHL loses the light as well.
            assert_eq!(a.snapshot().starts, if light_times_out { 2 } else { 3 });
            assert_eq!(a.snapshot().active, 0);
        }
    }
}
#[tokio::test]
async fn continual_small_arrivals_and_shared_root_cancellation_preserve_heavy() {
    for fifo in policies() {
        let c = ManualClock::default();
        let a = policy_admission(fifo, &config(2, 100, 2), c.clone());
        c.advance_to(Duration::from_secs(60));
        let seed = request(&a, 1, 50);
        turn().await;
        let mut seed = admitted(seed).await;
        seed.start();
        drop(seed);
        let heavy = request(&a, 0, 80);
        turn().await;
        let child = request(&a, 0, 1);
        turn().await;
        assert!(!child.is_finished(), "child shares heavy root FIFO");
        child.abort();
        assert!(matches!(child.await, Err(e) if e.is_cancelled()));
        let mut small = Vec::new();
        for n in 0..9 {
            let t = request(&a, 1, 1);
            turn().await;
            if !fifo && n < 8 {
                let mut h = admitted(t).await;
                h.start();
                drop(h);
            } else {
                assert!(!t.is_finished());
                small.push(t);
            }
        }
        c.advance_to(Duration::from_secs(120));
        turn().await;
        let mut h = admitted(heavy).await;
        h.start();
        drop(h);
        turn().await;
        for t in small {
            let mut h = admitted(t).await;
            h.start();
            drop(h);
            turn().await;
        }
        assert_eq!(a.snapshot().starts, 11);
        assert_eq!(a.snapshot().active, 0);
    }
}
