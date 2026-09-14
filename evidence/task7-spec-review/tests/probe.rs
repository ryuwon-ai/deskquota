use axum::http::{HeaderMap, HeaderValue};
use llmgw::{
    admission::{Admission, ManualClock, quota::RequestCost, retry},
    config::{Config, Endpoint, Limit, LoadedConfig},
    server::{self, RuntimeCredentials},
};
use std::{
    net::SocketAddr,
    sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    },
    time::{Duration, SystemTime},
};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::TcpListener,
    sync::Notify,
};
fn headers() -> HeaderMap {
    let mut h = HeaderMap::new();
    h.insert("content-type", HeaderValue::from_static("application/json"));
    h
}
fn config(address: SocketAddr) -> Config {
    let mut c = LoadedConfig::load("../../product/examples/fixture.toml")
        .unwrap()
        .config;
    c.listen = "127.0.0.1:0".parse().unwrap();
    c.upstream.api_base = format!("http://{address}/v1").parse().unwrap();
    c.quota.rpm = Limit::Known(10.try_into().unwrap());
    c.concurrency = 2;
    c.retry_transient_429 = true;
    let mut other = c.roots[0].clone();
    other.id = "other".into();
    c.roots.push(other);
    c
}
fn credentials() -> RuntimeCredentials {
    RuntimeCredentials::new(b"synthetic-data", b"synthetic-control", None).unwrap()
}
fn client() -> reqwest::Client {
    reqwest::Client::builder()
        .no_proxy()
        .retry(reqwest::retry::never())
        .redirect(reqwest::redirect::Policy::none())
        .build()
        .unwrap()
}
async fn get(c: &reqwest::Client, a: SocketAddr, root: &str) -> reqwest::Response {
    c.get(format!("http://{a}/r/{root}/v1/models"))
        .header("x-llmgw-token", "synthetic-data")
        .send()
        .await
        .unwrap()
}
#[test]
fn duplicate_null_discriminators_and_full_document_validation() {
    let h = headers();
    let reject = [
        r#"{"error":{"code":"slow_down","type":null,"type":"rate_limit_error"}}"#,
        r#"{"error":{"code":"slow_down","type":"rate_limit_error","type":null}}"#,
        r#"{"type":null,"type":"error","error":{"code":"slow_down"}}"#,
        r#"{"code":null,"code":null,"error":{"code":"slow_down"}}"#,
        r#"{"error":{"code":"slow_down","details":null,"details":null}}"#,
        r#"{"error":{"code":"slow_down","t\u0079pe":null,"type":null}}"#,
        r#"{"error":{"code":null,"code":"slow_down"}}"#,
        r#"{"error":{"code":"slow_down","code":null}}"#,
        r#"{"error":null,"error":{"code":"slow_down"}}"#,
        r#"{"error":{"code":"slow_down"},"ignored":"\udfff"}"#,
        r#"{"error":{"code":"slow_down"},"ignored":[{"x":"\ud800"}]}"#,
        r#"{"error":{"code":"slow_down"}} false"#,
    ];
    for (i, b) in reject.iter().enumerate() {
        assert!(!retry::transient(&h, b.as_bytes()), "negative {i}");
    }
    for b in [
        r#"{"error":{"code":"slow_down","type":null}}"#,
        r#"{"type":null,"code":null,"error":{"code":"rate_limit_exceeded","details":null}}"#,
    ] {
        assert!(retry::transient(&h, b.as_bytes()));
    }
    println!("classifier_negative=12 classifier_positive=2 http=0");
}
#[tokio::test]
async fn saturated_cooldown_survives_canceled_retry_and_smaller_update() {
    let mut c = config("127.0.0.1:9".parse().unwrap());
    c.concurrency = 1;
    let clock = ManualClock::default();
    let a = Admission::new(&c, Some(clock.clone()));
    clock.advance_to(Duration::from_secs(60));
    let mut h = a
        .acquire(
            0,
            RequestCost::Metadata,
            Endpoint::Models,
            Duration::from_secs(120),
        )
        .await
        .unwrap();
    h.start();
    let mut headers = headers();
    headers.append(
        "retry-after-ms",
        HeaderValue::from_static("999999999999999999999999999999999999"),
    );
    headers.append("retry-after", HeaderValue::from_static("0"));
    assert!(!retry::timing_allows_retry(&headers));
    a.cooldown(retry::header_delay(&headers, SystemTime::now()).unwrap());
    let task = tokio::spawn(h.retry());
    tokio::task::yield_now().await;
    assert_eq!(a.snapshot().active, 0);
    assert_eq!(a.snapshot().rpm_debited, 1);
    task.abort();
    assert!(task.await.err().unwrap().is_cancelled());
    a.cooldown(Duration::ZERO);
    clock.advance_to(Duration::from_secs(180));
    let aa = a.clone();
    let fresh = tokio::spawn(async move {
        aa.acquire(
            1,
            RequestCost::Metadata,
            Endpoint::Models,
            Duration::from_secs(120),
        )
        .await
    });
    tokio::task::yield_now().await;
    assert!(!fresh.is_finished());
    assert_eq!(a.snapshot().starts, 1);
    assert_eq!(a.snapshot().active, 0);
    fresh.abort();
    let _ = fresh.await;
    println!("manual_trace=1 starts=1 new_attempts_after_cancel=0 http=0");
}
struct Fixture {
    address: SocketAddr,
    attempts: Arc<AtomicUsize>,
    headers_sent: Arc<Notify>,
    release: Arc<Notify>,
    task: tokio::task::JoinHandle<()>,
}
impl Fixture {
    async fn new(body: Vec<u8>, delayed: bool) -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let attempts = Arc::new(AtomicUsize::new(0));
        let headers_sent = Arc::new(Notify::new());
        let release = Arc::new(Notify::new());
        let (n, h, r) = (attempts.clone(), headers_sent.clone(), release.clone());
        let task = tokio::spawn(async move {
            while let Ok((mut stream, _)) = listener.accept().await {
                let n = n.clone();
                let body = body.clone();
                let (h, r) = (h.clone(), r.clone());
                tokio::spawn(async move {
                    let mut incoming = Vec::new();
                    loop {
                        let mut b = [0; 2048];
                        let count = stream.read(&mut b).await.unwrap();
                        if count == 0 {
                            return;
                        }
                        incoming.extend_from_slice(&b[..count]);
                        if incoming.windows(4).any(|x| x == b"\r\n\r\n") {
                            break;
                        }
                    }
                    let attempt = n.fetch_add(1, Ordering::SeqCst);
                    if attempt == 0 {
                        let timing = if delayed {
                            "Retry-After-Ms: 5000\r\n"
                        } else {
                            "Retry-After: 0\r\n"
                        };
                        stream.write_all(format!("HTTP/1.1 429 Too Many Requests\r\nContent-Type: application/json\r\n{timing}Content-Length: {}\r\nConnection: close\r\n\r\n",body.len()).as_bytes()).await.unwrap();
                        h.notify_one();
                        if delayed {
                            r.notified().await;
                        }
                        stream.write_all(&body).await.unwrap();
                    } else {
                        stream.write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 2\r\nConnection: close\r\n\r\n{}").await.unwrap();
                    }
                    let _ = stream.shutdown().await;
                });
            }
        });
        Self {
            address,
            attempts,
            headers_sent,
            release,
            task,
        }
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        self.task.abort();
    }
}
async fn status(c: &reqwest::Client, a: SocketAddr) -> serde_json::Value {
    serde_json::from_slice(
        &c.get(format!("http://{a}/_llmgw/status"))
            .header("x-llmgw-control-token", "synthetic-control")
            .send()
            .await
            .unwrap()
            .bytes()
            .await
            .unwrap(),
    )
    .unwrap()
}
#[tokio::test]
async fn header_cooldown_before_eof_then_both_roots_reenter_same_ledger() {
    let fixture = Fixture::new(br#"{"error":{"code":"slow_down"}}"#.to_vec(), true).await;
    let clock = ManualClock::default();
    let gateway = server::testing::spawn_with_clock(
        config(fixture.address),
        credentials(),
        clock.clone(),
        None,
    )
    .await
    .unwrap();
    clock.advance_to(Duration::from_secs(60));
    let a = gateway.address();
    let c = client();
    let c1 = c.clone();
    let first = tokio::spawn(async move { get(&c1, a, "pi-work").await });
    fixture.headers_sent.notified().await;
    tokio::time::timeout(Duration::from_secs(2), async {
        loop {
            if status(&c, a).await["admission"]["blocked_reason"] == "upstream_cooldown" {
                break;
            }
            tokio::task::yield_now().await;
        }
    })
    .await
    .unwrap();
    let c2 = c.clone();
    let second = tokio::spawn(async move { get(&c2, a, "other").await });
    tokio::time::timeout(Duration::from_secs(2), async {
        loop {
            let s = status(&c, a).await;
            if s["admission"]["queue_length"] == 1 {
                assert_eq!(s["admission"]["active"], 1);
                break;
            }
            tokio::task::yield_now().await;
        }
    })
    .await
    .unwrap();
    assert_eq!(fixture.attempts.load(Ordering::SeqCst), 1);
    fixture.release.notify_one();
    tokio::time::timeout(Duration::from_secs(2), async {
        loop {
            let s = status(&c, a).await;
            if s["admission"]["queue_length"] == 2 {
                assert_eq!(s["admission"]["active"], 0);
                assert_eq!(s["admission"]["rpm_debited"], "1");
                break;
            }
            tokio::task::yield_now().await;
        }
    })
    .await
    .unwrap();
    clock.advance_to(Duration::from_millis(64999));
    tokio::task::yield_now().await;
    assert_eq!(fixture.attempts.load(Ordering::SeqCst), 1);
    assert!(!first.is_finished());
    assert!(!second.is_finished());
    clock.advance_to(Duration::from_secs(65));
    for t in [first, second] {
        let response = tokio::time::timeout(Duration::from_secs(2), t)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(response.status(), 200);
        assert_eq!(response.bytes().await.unwrap().as_ref(), b"{}");
    }
    assert_eq!(fixture.attempts.load(Ordering::SeqCst), 3);
    assert_eq!(server::testing::quota_snapshot(&gateway).rpm_debited, 3);
    gateway.shutdown().await.unwrap();
    println!("data_ingress=2 wire_attempts=3 outcomes_200=2 cooldown_gates=3");
}
#[tokio::test]
async fn exact_cap_replays_and_one_byte_over_raw_forwards() {
    for (size, attempts, code) in [(16384, 2, 200), (16385, 1, 429)] {
        let mut body = br#"{"error":{"code":"slow_down"}}"#.to_vec();
        body.resize(size, b' ');
        let fixture = Fixture::new(body.clone(), false).await;
        let clock = ManualClock::default();
        let gateway = server::testing::spawn_with_clock(
            config(fixture.address),
            credentials(),
            clock.clone(),
            None,
        )
        .await
        .unwrap();
        clock.advance_to(Duration::from_secs(60));
        let response = get(&client(), gateway.address(), "pi-work").await;
        assert_eq!(response.status(), code);
        let raw = response.bytes().await.unwrap();
        if code == 429 {
            assert_eq!(raw.as_ref(), body);
        }
        assert_eq!(fixture.attempts.load(Ordering::SeqCst), attempts);
        assert_eq!(
            server::testing::quota_snapshot(&gateway).rpm_debited,
            attempts as u128
        );
        gateway.shutdown().await.unwrap();
    }
    println!("data_ingress=2 wire_attempts=3 outcomes_200=1 outcomes_429=1");
}
