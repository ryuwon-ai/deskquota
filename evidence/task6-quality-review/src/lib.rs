#[cfg(test)]
mod tests {
    use futures_util::task::noop_waker;
    use llmgw::admission::quota::RequestCost;
    use llmgw::admission::{AcquireError, Admission, Hold, ManualClock};
    use llmgw::config::{
        Accounting, Auth, CancelPolicy, Config, Endpoint, Limit, Quota, Root, Upstream,
    };
    use std::future::Future;
    use std::pin::Pin;
    use std::sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    };
    use std::task::{Context, Poll, Wake, Waker};
    use std::time::Duration;

    type Ticket = Pin<Box<dyn Future<Output = Result<Hold, AcquireError>> + Send>>;
    fn config(rpm: Limit, tpm: u64) -> Config {
        Config {
            listen: "127.0.0.1:0".parse().unwrap(),
            concurrency: 16,
            cancel_policy: CancelPolicy::Drain,
            accounting: Accounting::Reserved,
            upstream: Upstream {
                api_base: "http://127.0.0.1:9".parse().unwrap(),
                auth: Auth::None,
            },
            quota: Quota {
                rpm,
                tpm: Limit::Known(tpm.try_into().unwrap()),
            },
            models: vec![],
            roots: (0..16)
                .map(|n| Root {
                    id: format!("review{n}"),
                    endpoints: vec![Endpoint::Models],
                    models: vec![],
                })
                .collect(),
        }
    }
    fn ticket(a: &Admission, root: usize, cost: u64, wait: u64) -> Ticket {
        let a = a.clone();
        Box::pin(async move {
            a.acquire(
                root,
                RequestCost::exact_fixture(cost),
                Endpoint::Models,
                Duration::from_secs(wait),
            )
            .await
        })
    }
    fn poll(ticket: &mut Ticket, waker: &Waker) -> Poll<Result<Hold, AcquireError>> {
        ticket.as_mut().poll(&mut Context::from_waker(waker))
    }
    fn pending(ticket: &mut Ticket, waker: &Waker) {
        assert!(poll(ticket, waker).is_pending());
    }
    #[derive(Default)]
    struct Counter(AtomicUsize);
    impl Wake for Counter {
        fn wake(self: Arc<Self>) {
            self.0.fetch_add(1, Ordering::SeqCst);
        }
        fn wake_by_ref(self: &Arc<Self>) {
            self.0.fetch_add(1, Ordering::SeqCst);
        }
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    async fn parallel_cancel_release_start_and_snapshot_preserve_joint_ownership() {
        tokio::time::timeout(Duration::from_secs(30), async {
            let clock = ManualClock::default();
            let a = Admission::new(&config(Limit::Known(10000.try_into().unwrap()), 10000), Some(clock.clone()));
            let mut cleanup_counts = vec![];
            for round in 0..8 {
                // New instance per round makes starts/debits independently countable.
                let a = if round == 0 { a.clone() } else {
                    Admission::new(&config(Limit::Known(10000.try_into().unwrap()), 10000), Some(clock.clone()))
                };
                let base = 60 + round * 61;
                clock.advance_to(Duration::from_secs(base));
                let mut running = vec![];
                for root in 0..16 {
                    let mut hold = ticket(&a, root, 3, 120).await.unwrap();
                    if root % 2 == 0 { hold.start(); }
                    running.push(hold);
                }
                let mut waiting = vec![];
                for i in 0..64 {
                    let mut t = ticket(&a, i % 16, 3, 120);
                    pending(&mut t, &noop_waker());
                    waiting.push(t);
                }
                assert!(matches!(ticket(&a, 0, 3, 120).await, Err(AcquireError::QueueFull)));
                let rendezvous = Arc::new(tokio::sync::Barrier::new(83));
                let mut jobs = vec![];
                for (i, t) in waiting.into_iter().enumerate() {
                    let b = rendezvous.clone();
                    jobs.push(tokio::spawn(async move {
                        b.wait().await;
                        if i % 2 == 0 { drop(t); false } else {
                            let mut hold = t.await.unwrap();
                            hold.start();
                            tokio::task::yield_now().await;
                            drop(hold);
                            true
                        }
                    }));
                }
                for hold in running {
                    let b = rendezvous.clone();
                    jobs.push(tokio::spawn(async move { b.wait().await; drop(hold); false }));
                }
                let read_a = a.clone();
                let b = rendezvous.clone();
                jobs.push(tokio::spawn(async move {
                    b.wait().await;
                    for _ in 0..512 {
                        let s = read_a.snapshot();
                        assert!(s.active <= 16);
                        assert!(s.tpm_debited + s.tpm_held <= 10000);
                        tokio::task::yield_now().await;
                    }
                    false
                }));
                let tick_clock = clock.clone();
                let b = rendezvous.clone();
                jobs.push(tokio::spawn(async move {
                    b.wait().await;
                    for millis in 1..=128 {
                        tick_clock.advance_to(Duration::from_millis(base * 1000 + millis));
                        tokio::task::yield_now().await;
                    }
                    false
                }));
                rendezvous.wait().await;
                let mut completed = 0;
                for job in jobs { completed += usize::from(job.await.unwrap()); }
                assert_eq!(completed, 32);
                let s = a.snapshot();
                assert_eq!((s.active, s.starts, s.retained, s.rpm_debited, s.tpm_debited, s.tpm_held), (0, 40, 40, 40, 120, 0));
                // Canceled tickets may have won provisional admission, but never start.
                assert!((48..=80).contains(&s.cleanups));
                cleanup_counts.push(s.cleanups);
                let mut refill = ticket(&a, 0, 3, 120).await.unwrap();
                refill.start();
                drop(refill);
                let refilled = a.snapshot();
                assert_eq!(refilled.cleanups, s.cleanups + 1);
                assert_eq!((refilled.active, refilled.starts, refilled.rpm_debited, refilled.tpm_debited, refilled.tpm_held), (0, 41, 41, 123, 0));
            }
            println!("race: 8 rounds; ingress=656 = initial holds128 + waiting512 + rejected8 + refill8; waiting completed256/canceled256; starts328; cleanup counts={cleanup_counts:?}");
        }).await.expect("bounded race must finish without a lost wake or deadlock");
    }

    #[tokio::test(start_paused = true)]
    async fn full_ledger_and_full_queue_have_no_repeated_idle_wakes_and_reclaim_all_holds() {
        let a = Admission::new(&config(Limit::Unlimited, 8192), None);
        tokio::time::advance(Duration::from_secs(60)).await;
        for i in 0..8192 {
            let mut h = ticket(&a, i % 16, 1, 120).await.unwrap();
            h.start();
            drop(h);
        }
        assert_eq!(a.snapshot().retained, 8192);
        let counter = Arc::new(Counter::default());
        let waker = Waker::from(counter.clone());
        let mut waiting = vec![];
        for i in 0..64 {
            let mut t = ticket(&a, i % 16, 1, 120);
            pending(&mut t, &waker);
            waiting.push(t);
        }
        // Drain enqueue notifications before measuring timer-only wake activity.
        for t in &mut waiting {
            pending(t, &waker);
        }
        counter.0.store(0, Ordering::SeqCst);
        tokio::time::advance(Duration::from_secs(5)).await;
        let age_wakes = counter.0.swap(0, Ordering::SeqCst);
        assert!(
            (1..=64).contains(&age_wakes),
            "one registered age timer per ticket at most: {age_wakes}"
        );
        for t in &mut waiting {
            pending(t, &waker);
        }
        counter.0.store(0, Ordering::SeqCst);
        tokio::time::advance(Duration::from_secs(10)).await;
        assert_eq!(
            counter.0.load(Ordering::SeqCst),
            0,
            "protected queue must be idle until a future quota/deadline event"
        );
        tokio::time::advance(Duration::from_secs(45)).await;
        let expiry_wakes = counter.0.load(Ordering::SeqCst);
        assert!((1..=64).contains(&expiry_wakes));
        let first = match poll(&mut waiting[0], &waker) {
            Poll::Ready(Ok(h)) => h,
            _ => panic!("expiry must make room for root0's head"),
        };
        let s = a.snapshot();
        assert_eq!(
            (
                s.active,
                s.retained,
                s.tpm_debited,
                s.tpm_held,
                s.starts,
                s.cleanups
            ),
            (16, 16, 0, 16, 8192, 8192)
        );
        drop(first);
        drop(waiting);
        let s = a.snapshot();
        assert_eq!(
            (s.active, s.retained, s.tpm_held, s.starts, s.cleanups),
            (0, 0, 0, 8192, 8208)
        );
        let refill = ticket(&a, 15, 8192, 120).await.unwrap();
        drop(refill);
        assert_eq!(a.snapshot().cleanups, 8209);
        println!(
            "saturated: ledger8192, waiting64; age wakes={age_wakes}, idle wakes=0, expiry wakes={expiry_wakes}; 16 provisional canceled exactly once, 48 pending canceled, refill succeeds"
        );
    }

    #[tokio::test(start_paused = true)]
    async fn simultaneous_quota_expiry_and_queue_deadlines_cannot_start_expired_work() {
        let a = Admission::new(&config(Limit::Known(1.try_into().unwrap()), 100), None);
        tokio::time::advance(Duration::from_secs(60)).await;
        let mut seed = ticket(&a, 0, 10, 120).await.unwrap();
        seed.start();
        drop(seed);
        let mut waiting = vec![];
        for i in 0..64 {
            let mut t = ticket(&a, i % 16, 10, 60);
            pending(&mut t, &noop_waker());
            waiting.push(t);
        }
        tokio::time::advance(Duration::from_secs(5)).await;
        for t in &mut waiting {
            pending(t, &noop_waker());
        }
        tokio::time::advance(Duration::from_secs(55)).await;
        for t in &mut waiting {
            assert!(matches!(
                poll(t, &noop_waker()),
                Poll::Ready(Err(AcquireError::Deadline))
            ));
        }
        drop(waiting);
        let s = a.snapshot();
        assert_eq!(
            (
                s.active,
                s.retained,
                s.starts,
                s.cleanups,
                s.rpm_debited,
                s.tpm_debited,
                s.tpm_held
            ),
            (0, 0, 1, 1, 0, 0, 0)
        );
        let fresh = ticket(&a, 15, 100, 120).await.unwrap();
        drop(fresh);
        assert_eq!(a.snapshot().cleanups, 2);
        println!(
            "deadline tie: 64/64 queued requests expire before reservation at quota expiry; no extra starts/cleanup, fresh full-capacity ticket succeeds"
        );
    }
}
