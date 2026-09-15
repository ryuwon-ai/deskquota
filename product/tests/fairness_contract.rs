//! Exact synthetic traces use the same asynchronous admission owner as HTTP.
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
#[tokio::test]
async fn root_fifo_heavy_light_heavy_cannot_skip_its_head() {
    let (a, c) = setup(4, 100, 2).await;
    let seed = request(&a, 1, 60);
    turn().await;
    let mut seed = admitted(seed).await;
    seed.start();
    drop(seed);
    let heavy = request(&a, 0, 80);
    turn().await;
    let light = request(&a, 0, 10);
    turn().await;
    let heavy2 = request(&a, 0, 80);
    turn().await;
    assert!(
        !light.is_finished(),
        "same-root light must wait behind heavy head"
    );
    c.advance_to(Duration::from_secs(120));
    turn().await;
    drop(admitted(heavy).await);
    turn().await;
    drop(admitted(light).await);
    turn().await;
    drop(admitted(heavy2).await);
}
#[tokio::test]
async fn total_queue_has_64_waiting_metadata_including_all_roots() {
    let (a, _) = setup(1, 100, 16).await;
    let running = request(&a, 0, 1);
    turn().await;
    let running = admitted(running).await;
    let mut waiting = vec![];
    for n in 0..64 {
        waiting.push(request(&a, n % 16, 0));
        turn().await;
    }
    let overflow = request(&a, 15, 0);
    turn().await;
    assert!(
        overflow.is_finished(),
        "65th waiting request must reject immediately"
    );
    assert!(overflow.await.unwrap().is_err());
    for t in waiting {
        t.abort();
    }
    drop(running);
    turn().await;
    assert_eq!(a.snapshot().active, 0);
}
#[tokio::test]
async fn root_round_robin_not_global_fifo() {
    let (a, _) = setup(1, 100, 2).await;
    let first = request(&a, 0, 0);
    turn().await;
    let first = admitted(first).await;
    let a1 = request(&a, 0, 0);
    turn().await;
    let a2 = request(&a, 0, 0);
    turn().await;
    let b1 = request(&a, 1, 0);
    turn().await;
    drop(first);
    turn().await;
    assert!(
        b1.is_finished(),
        "root1 takes the next turn after root0 actual admission"
    );
    assert!(!a1.is_finished());
    drop(admitted(b1).await);
    turn().await;
    drop(admitted(a1).await);
    turn().await;
    drop(admitted(a2).await);
}
async fn seed(a: &Admission, n: u64) {
    let t = request(a, 1, n);
    turn().await;
    let mut h = admitted(t).await;
    h.start();
    drop(h);
}
#[tokio::test]
async fn eight_actual_bypasses_protect_heavy_from_continuous_small_arrivals() {
    let (a, c) = setup(4, 100, 2).await;
    seed(&a, 50).await;
    let h = request(&a, 0, 80);
    turn().await;
    for _ in 0..8 {
        let s = request(&a, 1, 1);
        turn().await;
        let mut s = admitted(s).await;
        s.start();
        drop(s);
    }
    let ninth = request(&a, 1, 1);
    turn().await;
    assert!(
        !ninth.is_finished(),
        "eight actual bypasses must erect barrier for current-budget-unfit heavy"
    );
    c.advance_to(Duration::from_secs(120));
    turn().await;
    let mut h = admitted(h).await;
    h.start();
    drop(h);
    turn().await;
    drop(admitted(ninth).await);
    assert_eq!(a.snapshot().active, 0);
}
#[tokio::test]
async fn five_seconds_independently_protect_heavy() {
    let (a, c) = setup(4, 100, 2).await;
    seed(&a, 50).await;
    let h = request(&a, 0, 80);
    turn().await;
    c.advance_to(Duration::from_secs(65));
    turn().await;
    let light = request(&a, 1, 1);
    turn().await;
    assert!(!light.is_finished(), "age alone must block small admission");
    h.abort();
    let _ = h.await;
    turn().await;
    drop(admitted(light).await);
}
#[tokio::test]
async fn scans_and_time_events_do_not_count_as_bypasses() {
    let (a, c) = setup(4, 100, 2).await;
    seed(&a, 50).await;
    let h = request(&a, 0, 80);
    turn().await;
    for millis in 1..100 {
        c.advance_to(Duration::from_millis(60_000 + millis));
        turn().await;
    }
    let light = request(&a, 1, 1);
    turn().await;
    drop(admitted(light).await);
    h.abort();
    let _ = h.await;
}
#[tokio::test]
async fn queued_cancel_never_starts_and_invalid_head_does_not_block() {
    let (a, _) = setup(1, 100, 2).await;
    let run = request(&a, 0, 0);
    turn().await;
    let run = admitted(run).await;
    let invalid = request(&a, 1, 101);
    turn().await;
    assert!(invalid.is_finished());
    assert!(invalid.await.unwrap().is_err());
    let cancelled = request(&a, 1, 80);
    turn().await;
    cancelled.abort();
    let _ = cancelled.await;
    drop(run);
    turn().await;
    assert_eq!(a.snapshot().starts, 0);
    assert_eq!(a.snapshot().cleanups, 1);
    assert_eq!(a.snapshot().active, 0);
}
#[tokio::test]
async fn expired_barrier_removes_head_and_releases_other_root() {
    let (a, c) = setup(4, 100, 2).await;
    seed(&a, 50).await;
    let aa = a.clone();
    let head = tokio::spawn(async move {
        aa.acquire(
            0,
            RequestCost::exact_fixture(80),
            Endpoint::Models,
            Duration::from_secs(6),
        )
        .await
    });
    turn().await;
    c.advance_to(Duration::from_secs(65));
    turn().await;
    let light = request(&a, 1, 1);
    turn().await;
    assert!(!light.is_finished());
    c.advance_to(Duration::from_secs(66));
    turn().await;
    assert!(
        head.is_finished(),
        "manual clock must drive queue deadline as well as quota expiry"
    );
    assert!(head.await.unwrap().is_err());
    drop(admitted(light).await);
}
#[tokio::test]
async fn later_fifo_head_keeps_original_age() {
    let (a, c) = setup(4, 100, 3).await;
    seed(&a, 50).await;
    let h1 = request(&a, 0, 80);
    turn().await;
    let h2 = request(&a, 0, 70);
    turn().await;
    c.advance_to(Duration::from_secs(65));
    turn().await;
    h1.abort();
    let _ = h1.await;
    turn().await;
    let light = request(&a, 2, 1);
    turn().await;
    assert!(
        !light.is_finished(),
        "new FIFO head retains original enqueue age"
    );
    h2.abort();
    let _ = h2.await;
    turn().await;
    drop(admitted(light).await);
}
#[tokio::test]
async fn cap_one_running_long_request_is_nonpreemptible() {
    let (a, c) = setup(1, 100, 2).await;
    let run = request(&a, 0, 0);
    turn().await;
    let mut run = admitted(run).await;
    run.start();
    let h = request(&a, 1, 80);
    turn().await;
    c.advance_to(Duration::from_secs(121));
    turn().await;
    assert!(!h.is_finished());
    assert_eq!(a.snapshot().active, 1);
    drop(run);
    turn().await;
    drop(admitted(h).await);
}
mod support;
use llmgw::server::{self, RuntimeCredentials};
use support::fixture::{UpstreamFixture, response_body, send_raw, status};
fn creds() -> RuntimeCredentials {
    RuntimeCredentials::new(b"synthetic-control", None).unwrap()
}
async fn control_status(g: &server::GatewayHandle) -> serde_json::Value {
    let r=send_raw(g.address(),b"GET /_llmgw/status HTTP/1.1\r\nHost: localhost\r\nX-LLMGW-Control-Token: synthetic-control\r\nConnection: close\r\n\r\n").await;
    assert_eq!(status(&r), 200);
    serde_json::from_slice(response_body(&r)).unwrap()
}
#[tokio::test]
async fn control_reports_registered_roots_and_estimated_mode() {
    let u =
        UpstreamFixture::start(b"HTTP/1.1 200 OK\r\nContent-Length: 2\r\n\r\n{}".to_vec()).await;
    let mut cfg = config(1, 100, 2);
    cfg.upstream.api_base = format!("http://{}/v1", u.address()).parse().unwrap();
    let g = server::spawn(cfg, creds()).await.unwrap();
    let s = control_status(&g).await;
    assert_eq!(s["admission"]["queue_length"], 0);
    assert_eq!(s["admission"]["roots"].as_array().unwrap().len(), 2);
    assert_eq!(
        s["admission"]["estimate_mode"],
        "json_utf8_bytes_plus_output_reservation"
    );
    assert_eq!(s["admission"]["tpm_debited"], "0");
    g.shutdown().await.unwrap();
    assert_eq!(u.attempts(), 0);
}
#[test]
fn directly_constructed_config_cannot_create_unbounded_or_ambiguous_roots() {
    assert!(
        server::validate_start_config(&config(1, 100, 17)).is_err(),
        "public server boundary must reject root count above16"
    );
    let mut cfg = config(1, 100, 2);
    cfg.roots[1].id = cfg.roots[0].id.clone();
    assert!(server::validate_start_config(&cfg).is_err());
    cfg.roots[1].id = "a/b".into();
    assert!(server::validate_start_config(&cfg).is_err());
    cfg.roots[1].id = "x".repeat(65);
    assert!(server::validate_start_config(&cfg).is_err());
}
use support::fixture::{abort_socket, open_raw};
use tokio::io::AsyncReadExt;
fn wire_config(address: std::net::SocketAddr, roots: usize) -> Config {
    let mut c = config(1, 1000, roots);
    c.upstream.api_base = format!("http://{address}/v1").parse().unwrap();
    c.quota.tpm = Limit::Unknown;
    c.models = vec![llmgw::config::Model {
        id: "synthetic".into(),
        max_output_tokens: None,
    }];
    for r in &mut c.roots {
        r.endpoints = vec![Endpoint::Models, Endpoint::CountTokens];
        r.models = vec!["synthetic".into()];
    }
    c
}
fn wire_request(root: usize, id: &str, count: bool) -> Vec<u8> {
    let (method, path, body) = if count {
        (
            "POST",
            "messages/count_tokens",
            r#"{"model":"synthetic","messages":[]}"#,
        )
    } else {
        ("GET", "models", "")
    };
    format!("{method} /r/root{root}/v1/{path}?synthetic_id={id} HTTP/1.1\r\nHost: localhost\r\nContent-Type: application/json\r\nX-Pi-Session-Id: child-{id}\r\nX-Session-Id: independent-{id}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",body.len()).into_bytes()
}
async fn wait_queue(g: &server::GatewayHandle, n: u64) -> serde_json::Value {
    tokio::time::timeout(Duration::from_secs(3), async {
        loop {
            let s = control_status(g).await;
            if s["admission"]["queue_length"] == n {
                return s;
            }
            tokio::task::yield_now().await;
        }
    })
    .await
    .expect("queue state event did not arrive")
}
async fn wait_active(g: &server::GatewayHandle, n: u64) {
    tokio::time::timeout(Duration::from_secs(3), async {
        loop {
            if control_status(g).await["admission"]["active"] == n {
                return;
            }
            tokio::task::yield_now().await;
        }
    })
    .await
    .unwrap();
}
async fn drain(mut socket: tokio::net::TcpStream) -> Vec<u8> {
    let mut bytes = vec![];
    tokio::time::timeout(Duration::from_secs(3), socket.read_to_end(&mut bytes))
        .await
        .unwrap()
        .unwrap();
    bytes
}
const OK: &[u8] = b"HTTP/1.1 200 OK\r\nContent-Length: 2\r\nConnection: close\r\n\r\n{}";
#[tokio::test]
async fn wire_root_plus_fifteen_children_share_fifo_and_one_rr_turn() {
    let u = UpstreamFixture::start(OK.to_vec()).await;
    let clock = ManualClock::default();
    let (gate, rx) = tokio::sync::watch::channel(false);
    let g =
        server::testing::spawn_with_clock(wire_config(u.address(), 2), creds(), clock, Some(rx))
            .await
            .unwrap();
    let first = open_raw(g.address(), &wire_request(0, "first", false)).await;
    wait_active(&g, 1).await;
    let mut sockets = vec![first];
    for n in 1..=15 {
        sockets.push(open_raw(g.address(), &wire_request(0, &format!("a{n}"), n % 2 == 0)).await);
        wait_queue(&g, n).await;
    }
    for n in 1..=2 {
        sockets.push(open_raw(g.address(), &wire_request(1, &format!("b{n}"), false)).await);
        wait_queue(&g, 15 + n).await;
    }
    let s = control_status(&g).await;
    assert_eq!(s["admission"]["roots"][0]["queue_length"], 15);
    assert_eq!(s["admission"]["roots"][1]["queue_length"], 2);
    assert_eq!(u.attempts(), 0);
    gate.send(true).unwrap();
    let mut ids = vec![];
    for c in u.captures(18).await {
        ids.push(c.target.split("synthetic_id=").nth(1).unwrap().to_owned());
    }
    assert_eq!(&ids[..5], &["first", "b1", "a1", "b2", "a2"]);
    let a: Vec<_> = ids
        .iter()
        .filter(|id| id.starts_with('a'))
        .cloned()
        .collect();
    assert_eq!(a, (1..=15).map(|n| format!("a{n}")).collect::<Vec<_>>());
    for s in sockets {
        assert_eq!(status(&drain(s).await), 200);
    }
    assert_eq!(u.attempts(), 18);
    g.shutdown().await.unwrap();
}
#[tokio::test]
async fn wire_metadata_across_roots_share_rpm_without_tpm() {
    let u = UpstreamFixture::start(OK.to_vec()).await;
    let mut cfg = wire_config(u.address(), 2);
    cfg.quota.rpm = Limit::Known(1.try_into().unwrap());
    let clock = ManualClock::default();
    let g = server::testing::spawn_with_clock(cfg, creds(), clock.clone(), None)
        .await
        .unwrap();
    clock.advance_to(Duration::from_secs(60));
    let first = send_raw(g.address(), &wire_request(0, "models", false)).await;
    assert_eq!(status(&first), 200);
    let second = open_raw(g.address(), &wire_request(1, "count", true)).await;
    let s = wait_queue(&g, 1).await;
    assert_eq!(s["admission"]["blocked_reason"], "rpm");
    assert_eq!(s["admission"]["tpm_debited"], "0");
    assert_eq!(u.attempts(), 1);
    clock.advance_to(Duration::from_secs(120));
    assert_eq!(status(&drain(second).await), 200);
    assert_eq!(u.attempts(), 2);
    assert_eq!(server::testing::quota_snapshot(&g).starts, 2);
    g.shutdown().await.unwrap();
}
#[tokio::test]
async fn wire_64_total_waiters_overflow_429_and_rst_never_start() {
    let u = UpstreamFixture::start(OK.to_vec()).await;
    let (gate, rx) = tokio::sync::watch::channel(false);
    let g = server::testing::spawn_with_clock(
        wire_config(u.address(), 2),
        creds(),
        ManualClock::default(),
        Some(rx),
    )
    .await
    .unwrap();
    let active = open_raw(g.address(), &wire_request(0, "active", false)).await;
    wait_active(&g, 1).await;
    let mut sockets = vec![];
    for n in 0..64 {
        sockets.push(
            open_raw(
                g.address(),
                &wire_request(n % 2, &n.to_string(), n % 3 == 0),
            )
            .await,
        );
        wait_queue(&g, n as u64 + 1).await;
    }
    let overflow = send_raw(g.address(), &wire_request(1, "overflow", true)).await;
    assert_eq!(status(&overflow), 429);
    assert!(String::from_utf8_lossy(&overflow).contains("gateway_queue_full"));
    assert_eq!(u.attempts(), 0);
    for s in sockets {
        abort_socket(s);
    }
    wait_queue(&g, 0).await;
    abort_socket(active);
    wait_active(&g, 0).await;
    gate.send(true).unwrap();
    assert_eq!(u.attempts(), 0);
    let snapshot = server::testing::quota_snapshot(&g);
    assert_eq!(snapshot.starts, 0);
    assert_eq!(snapshot.cleanups, 1);
    g.shutdown().await.unwrap();
}
#[tokio::test]
async fn admitted_but_unpolled_waiter_cancel_releases_hold_once() {
    use std::future::Future;
    use std::task::{Context, Poll};
    let (a, _) = setup(1, 100, 2).await;
    let first = request(&a, 0, 0);
    turn().await;
    let first = admitted(first).await;
    let mut waiting = Box::pin(a.acquire(
        1,
        RequestCost::Metadata,
        Endpoint::Models,
        Duration::from_secs(120),
    ));
    let waker = futures_util::task::noop_waker();
    let mut cx = Context::from_waker(&waker);
    assert!(matches!(waiting.as_mut().poll(&mut cx), Poll::Pending));
    drop(first);
    let third = request(&a, 0, 0);
    turn().await;
    assert_eq!(a.snapshot().active, 1);
    drop(waiting);
    turn().await;
    drop(admitted(third).await);
    assert_eq!(a.snapshot().active, 0);
    assert_eq!(a.snapshot().starts, 0);
    assert_eq!(a.snapshot().cleanups, 3);
}
#[tokio::test]
async fn oldest_valid_barrier_wins_across_roots() {
    let (a, c) = setup(4, 100, 3).await;
    seed(&a, 50).await;
    let oldest = request(&a, 0, 80);
    turn().await;
    c.advance_to(Duration::from_secs(61));
    let younger = request(&a, 2, 60);
    turn().await;
    c.advance_to(Duration::from_secs(66));
    turn().await;
    c.advance_to(Duration::from_secs(120));
    turn().await;
    assert!(oldest.is_finished());
    assert!(
        !younger.is_finished(),
        "only oldest protected heavy fits the fresh budget first"
    );
    drop(admitted(oldest).await);
    turn().await;
    drop(admitted(younger).await);
}
#[tokio::test]
async fn heavy_heavy_light_in_same_root_finishes_in_fifo_order() {
    let (a, c) = setup(1, 100, 2).await;
    seed(&a, 50).await;
    c.advance_to(Duration::from_secs(61));
    let h1 = request(&a, 0, 80);
    turn().await;
    let h2 = request(&a, 0, 80);
    turn().await;
    let light = request(&a, 0, 10);
    turn().await;
    assert!(!light.is_finished());
    c.advance_to(Duration::from_secs(120));
    turn().await;
    let mut h1 = admitted(h1).await;
    h1.start();
    drop(h1);
    assert!(!h2.is_finished());
    c.advance_to(Duration::from_secs(180));
    turn().await;
    let mut h2 = admitted(h2).await;
    h2.start();
    drop(h2);
    turn().await;
    drop(admitted(light).await);
    assert_eq!(a.snapshot().active, 0);
}
#[tokio::test]
async fn normal_sixteen_root_rr_is_not_a_resource_bypass() {
    let (a, _) = setup(1, 100, 16).await;
    let first = request(&a, 0, 0);
    turn().await;
    let first = admitted(first).await;
    let root0 = request(&a, 0, 0);
    turn().await;
    let mut others = vec![];
    for root in 1..16 {
        others.push(request(&a, root, 0));
        turn().await;
    }
    drop(first);
    for (root, task) in others.into_iter().enumerate() {
        turn().await;
        assert!(
            task.is_finished(),
            "root {} must keep its ordinary RR turn",
            root + 1
        );
        assert!(
            !root0.is_finished(),
            "ordinary RR turns are not cost-fit bypasses"
        );
        drop(admitted(task).await);
    }
    turn().await;
    drop(admitted(root0).await);
}
#[tokio::test]
async fn bypass_trigger_selects_oldest_valid_head_even_if_it_was_current_fit() {
    let (a, c) = setup(1, 100, 10).await;
    let run = request(&a, 1, 50);
    turn().await;
    let mut run = admitted(run).await;
    run.start();
    let oldest = request(&a, 0, 1);
    turn().await;
    let heavy = request(&a, 1, 80);
    turn().await;
    let mut small = vec![];
    for root in 2..10 {
        small.push(request(&a, root, 1));
        turn().await;
    }
    drop(run);
    for t in small {
        turn().await;
        let mut h = admitted(t).await;
        h.start();
        drop(h);
    }
    turn().await;
    assert!(
        oldest.is_finished(),
        "threshold trigger protects oldest eligible head across all roots"
    );
    let mut oldest = admitted(oldest).await;
    oldest.start();
    drop(oldest);
    assert!(!heavy.is_finished());
    c.advance_to(Duration::from_secs(120));
    turn().await;
    let mut heavy = admitted(heavy).await;
    heavy.start();
    drop(heavy);
}
#[tokio::test]
async fn chosen_barrier_persists_until_its_own_admission_or_removal() {
    let (a, _) = setup(1, 100, 11).await;
    let run = request(&a, 1, 50);
    turn().await;
    let mut run = admitted(run).await;
    run.start();
    let oldest = request(&a, 0, 1);
    turn().await;
    let trigger = request(&a, 1, 80);
    turn().await;
    let mut small = vec![];
    for root in 2..10 {
        small.push(request(&a, root, 1));
        turn().await;
    }
    drop(run);
    let last = small.pop().unwrap();
    for t in small {
        turn().await;
        drop(admitted(t).await);
    }
    turn().await;
    let last = admitted(last).await;
    trigger.abort();
    let _ = trigger.await;
    let late = request(&a, 10, 1);
    turn().await;
    drop(last);
    turn().await;
    assert!(
        oldest.is_finished(),
        "canceling trigger must not clear another chosen barrier"
    );
    assert!(!late.is_finished());
    drop(admitted(oldest).await);
    turn().await;
    drop(admitted(late).await);
}
#[tokio::test]
async fn admitted_owner_retains_original_wait_start_and_queue_deadline() {
    let (a, c) = setup(1, 100, 2).await;
    seed(&a, 50).await;
    let queued = request(&a, 0, 80);
    turn().await;
    c.advance_to(Duration::from_secs(120));
    turn().await;
    let hold = admitted(queued).await;
    assert_eq!(hold.wait_started(), Duration::from_secs(60));
    assert_eq!(hold.queue_deadline(), Duration::from_secs(180));
    drop(hold);
}
