mod support;

use llmgw::config::{self, Auth, Config, Limit};
use llmgw::server::{self, GatewayHandle, RuntimeCredentials};
use serde_json::{Value, json};
use std::net::SocketAddr;
use std::sync::{
    Arc, Mutex,
    atomic::{AtomicUsize, Ordering},
};
use std::time::Duration;
use support::fixture::UpstreamFixture;
use tokio::io::AsyncWriteExt;
use tokio::sync::watch;

fn config(address: SocketAddr) -> Config {
    let mut cfg = config::parse(include_bytes!("../examples/fixture.toml")).unwrap();
    cfg.listen = "127.0.0.1:0".parse().unwrap();
    cfg.startup_hold_secs = 0;
    cfg.upstream.api_base = format!("http://{address}/v1").parse().unwrap();
    cfg.upstream.auth = Auth::Forward;
    cfg.quota.rpm = Limit::Known(1000.try_into().unwrap());
    cfg.quota.tpm = Limit::Unknown;
    cfg
}

fn client() -> reqwest::Client {
    reqwest::Client::builder()
        .no_proxy()
        .timeout(Duration::from_secs(3))
        .build()
        .unwrap()
}

async fn spawn(cfg: Config) -> GatewayHandle {
    server::spawn(
        cfg,
        RuntimeCredentials::new(b"synthetic-control", None).unwrap(),
    )
    .await
    .unwrap()
}

async fn status(gateway: &GatewayHandle) -> Value {
    serde_json::from_slice(
        &client()
            .get(format!("http://{}/_llmgw/status", gateway.address()))
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

async fn request(gateway: &GatewayHandle) -> reqwest::Response {
    send(
        gateway.address(),
        "pi-work",
        "example-model",
        "synthetic-one",
        false,
    )
    .await
}

async fn send(
    address: SocketAddr,
    root: &str,
    model: &str,
    auth: &str,
    no_cache: bool,
) -> reqwest::Response {
    let mut request = client()
        .post(format!("http://{address}/r/{root}/v1/chat/completions"))
        .bearer_auth(auth)
        .header("content-type", "application/json")
        .body(
            json!({"model":model, "messages":[{"role":"user","content":"synthetic request"}]})
                .to_string(),
        );
    if no_cache {
        request = request.header("cache-control", "no-store");
    }
    request.send().await.unwrap()
}

const GOOD: &str = r#"{"choices":[{"index":0,"message":{"role":"assistant","content":"synthetic answer"},"finish_reason":"stop"}]}"#;
fn response(code: u16, headers: &str, body: &str) -> Vec<u8> {
    format!("HTTP/1.1 {code} Synthetic\r\nContent-Type: application/json\r\nContent-Length: {}\r\n{headers}Connection: close\r\n\r\n{body}", body.len()).into_bytes()
}

struct ControlledUpstream {
    address: SocketAddr,
    attempts: Arc<AtomicUsize>,
    responses: Arc<Mutex<std::collections::VecDeque<Vec<u8>>>>,
    gate: watch::Sender<bool>,
    task: tokio::task::JoinHandle<()>,
}
impl ControlledUpstream {
    async fn start(responses: Vec<Vec<u8>>, ready: bool) -> Self {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let attempts = Arc::new(AtomicUsize::new(0));
        let responses = Arc::new(Mutex::new(std::collections::VecDeque::from(responses)));
        let (gate, rx) = watch::channel(ready);
        let count = attempts.clone();
        let replies = responses.clone();
        let task = tokio::spawn(async move {
            let mut connections = tokio::task::JoinSet::new();
            loop {
                tokio::select! {
                    accepted = listener.accept() => {
                        let (mut socket, _) = accepted.unwrap();
                        let count = count.clone();
                        let replies = replies.clone();
                        let mut ready = rx.clone();
                        connections.spawn(async move {
                            support::fixture::read_request(&mut socket).await.unwrap();
                            let reply = {
                                let mut replies = replies.lock().unwrap();
                                if replies.len() > 1 { replies.pop_front().unwrap() } else { replies.front().unwrap().clone() }
                            };
                            count.fetch_add(1, Ordering::SeqCst);
                            while !*ready.borrow() { if ready.changed().await.is_err() { return; } }
                            let _ = socket.write_all(&reply).await;
                            let _ = socket.shutdown().await;
                        });
                    }
                    completed = connections.join_next(), if !connections.is_empty() => { completed.unwrap().unwrap(); }
                }
            }
        });
        Self {
            address,
            attempts,
            responses,
            gate,
            task,
        }
    }
    fn set(&self, responses: Vec<Vec<u8>>) {
        *self.responses.lock().unwrap() = responses.into();
    }
    fn attempts(&self) -> usize {
        self.attempts.load(Ordering::SeqCst)
    }
}
impl Drop for ControlledUpstream {
    fn drop(&mut self) {
        self.task.abort();
    }
}

async fn wait_status(gateway: &GatewayHandle, condition: impl Fn(&Value) -> bool) -> Value {
    tokio::time::timeout(Duration::from_secs(2), async {
        loop {
            let value = status(gateway).await;
            if condition(&value) {
                return value;
            }
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
    })
    .await
    .unwrap()
}

#[tokio::test]
async fn short_server_timing_recovers_http_json_and_sse_without_rewriting_bytes() {
    let error = r#"{"error":{"code":500,"message":"synthetic outage"}}"#;
    for (code, media, body) in [
        (503, "application/json", error.to_owned()),
        (200, "application/json", error.to_owned()),
        (
            200,
            "text/event-stream",
            format!("data: {error}\n\ndata: [DONE]\n\n"),
        ),
    ] {
        let wire = String::from_utf8(response(code, "Retry-After-Ms: 799\r\n", &body))
            .unwrap()
            .replace(
                "Content-Type: application/json",
                &format!("Content-Type: {media}"),
            )
            .into_bytes();
        let upstream = ControlledUpstream::start(vec![wire], true).await;
        let gateway = spawn(config(upstream.address)).await;
        for _ in 0..3 {
            let reply = request(&gateway).await;
            assert_eq!(reply.status(), code);
            assert_eq!(reply.bytes().await.unwrap().as_ref(), body.as_bytes());
        }
        wait_status(&gateway, |v| {
            v["circuit_breaker"]["open"] == 1 && v["active"] == 0
        })
        .await;
        let blocked = request(&gateway).await;
        assert_eq!(blocked.status(), 503);
        assert_eq!(blocked.headers()["retry-after"], "1", "{code} {media}");
        assert_eq!(upstream.attempts(), 3);
        upstream.set(vec![response(200, "", GOOD)]);
        tokio::time::sleep(Duration::from_millis(1050)).await;
        assert_eq!(
            request(&gateway).await.bytes().await.unwrap().as_ref(),
            GOOD.as_bytes()
        );
        let value = wait_status(&gateway, |v| v["circuit_breaker"]["recoveries"] == 1).await;
        assert_eq!(value["circuit_breaker"]["probes"], 1);
        assert_eq!(upstream.attempts(), 4);
        gateway.shutdown().await.unwrap();
    }
}

#[tokio::test]
async fn http_200_stream_error_opens_circuit_without_rewriting_body() {
    let body =
        "data: {\"error\":{\"code\":500,\"message\":\"synthetic outage\"}}\n\ndata: [DONE]\n\n";
    let wire = String::from_utf8(response(200, "Retry-After: 7\r\n", body))
        .unwrap()
        .replace(
            "Content-Type: application/json",
            "Content-Type: text/event-stream",
        )
        .into_bytes();
    let upstream = UpstreamFixture::start(wire).await;
    let gateway = spawn(config(upstream.address())).await;
    for _ in 0..3 {
        let reply = request(&gateway).await;
        assert_eq!(reply.status(), 200);
        assert_eq!(reply.bytes().await.unwrap().as_ref(), body.as_bytes());
    }
    let blocked = request(&gateway).await;
    assert_eq!(blocked.status(), 503);
    assert!(
        blocked.headers()["retry-after"]
            .to_str()
            .unwrap()
            .parse::<u64>()
            .unwrap()
            >= 6
    );
    assert!(
        blocked
            .text()
            .await
            .unwrap()
            .contains("upstream_circuit_open")
    );
    assert_eq!(upstream.attempts(), 3);
    let value = status(&gateway).await;
    assert_eq!(value["response_errors"]["server"], 3);
    assert_eq!(value["circuit_breaker"]["failures"], 3);
    assert_eq!(value["terminal_body_eof"], 3);
    assert_eq!(value["usage_unknown"], 3);
    assert_eq!(value["admission"]["rpm_debited"], "3");
    gateway.shutdown().await.unwrap();
}

#[tokio::test]
async fn in_band_errors_are_classified_across_protocols_and_representations() {
    for (endpoint, media, body, category) in [
        (
            "chat/completions",
            "application/json",
            r#"{"error":{"code":500}}"#,
            "server",
        ),
        (
            "responses",
            "application/json",
            r#"{"status":"failed","error":{"code":"server_error"}}"#,
            "server",
        ),
        (
            "responses",
            "text/event-stream",
            "data: {\"type\":\"response.failed\",\"response\":{\"error\":{\"code\":\"server_error\"}}}\n\n",
            "server",
        ),
        (
            "messages",
            "text/event-stream",
            "event: error\ndata: {\"type\":\"error\",\"error\":{\"type\":\"overloaded_error\"}}\n\n",
            "server",
        ),
        (
            "messages",
            "application/json",
            r#"{"type":"error","error":{"type":"api_error"}}"#,
            "server",
        ),
        (
            "chat/completions",
            "text/event-stream",
            "data: {\"error\":{\"code\":429}}\n\n",
            "rate_limit",
        ),
        (
            "chat/completions",
            "application/json",
            r#"{"error":{"type":"invalid_request_error"}}"#,
            "client",
        ),
        (
            "chat/completions",
            "text/event-stream",
            "event: error\ndata: unstructured\n\n",
            "unknown",
        ),
    ] {
        let wire = String::from_utf8(response(200, "", body))
            .unwrap()
            .replace(
                "Content-Type: application/json",
                &format!("Content-Type: {media}"),
            )
            .into_bytes();
        let upstream = UpstreamFixture::start(wire).await;
        let mut cfg = config(upstream.address());
        cfg.retry_transient_429 = true;
        cfg.cache = Some(config::CacheConfig::default());
        let gateway = spawn(cfg).await;
        let payload = match endpoint {
            "responses" => json!({"model":"example-model","input":"synthetic request"}),
            _ => {
                json!({"model":"example-model","messages":[{"role":"user","content":"synthetic request"}],"max_tokens":10})
            }
        };
        let attempts = if category == "server" { 3 } else { 4 };
        for _ in 0..attempts {
            let reply = client()
                .post(format!(
                    "http://{}/r/pi-work/v1/{endpoint}",
                    gateway.address()
                ))
                .header("content-type", "application/json")
                .body(payload.to_string())
                .send()
                .await
                .unwrap();
            assert_eq!(reply.status(), 200, "{endpoint} {category}");
            assert_eq!(reply.bytes().await.unwrap().as_ref(), body.as_bytes());
        }
        let value = wait_status(&gateway, |v| v["active"] == 0).await;
        assert_eq!(value["response_errors"][category], attempts);
        assert_eq!(
            value["circuit_breaker"]["failures"],
            if category == "server" { 3 } else { 0 }
        );
        assert_eq!(
            value["circuit_breaker"]["open"],
            u64::from(category == "server")
        );
        assert_eq!(value["exact_cache"]["stores"], 0);
        assert_eq!(value["usage_known"], 0);
        assert_eq!(value["admission"]["shared_cooldown_ms"], 0);
        assert_eq!(
            upstream.attempts(),
            attempts as usize,
            "no post-header retries"
        );
        gateway.shutdown().await.unwrap();
    }
}

#[tokio::test]
async fn in_band_client_error_is_neutral_and_cannot_recover_a_probe() {
    let failure = response(200, "", r#"{"error":{"code":503}}"#);
    let neutral = response(200, "", r#"{"error":{"code":429}}"#);
    let upstream = ControlledUpstream::start(
        vec![
            failure.clone(),
            neutral.clone(),
            failure.clone(),
            neutral.clone(),
            failure,
        ],
        true,
    )
    .await;
    let gateway = spawn(config(upstream.address)).await;
    for _ in 0..5 {
        let reply = request(&gateway).await;
        assert_eq!(reply.status(), 200);
        reply.bytes().await.unwrap();
    }
    wait_status(&gateway, |v| v["circuit_breaker"]["open"] == 1).await;
    assert_eq!(
        upstream.attempts(),
        5,
        "neutral errors did not reset the server-failure streak"
    );
    upstream.set(vec![neutral, response(200, "", GOOD)]);
    tokio::time::sleep(Duration::from_millis(5100)).await;
    request(&gateway).await.bytes().await.unwrap();
    let value = wait_status(&gateway, |v| v["active"] == 0).await;
    assert_eq!(value["circuit_breaker"]["recoveries"], 0);
    assert_eq!(value["circuit_breaker"]["probes"], 1);
    assert_eq!(value["circuit_breaker"]["half_open"], 0);
    assert_eq!(request(&gateway).await.text().await.unwrap(), GOOD);
    let value = wait_status(&gateway, |v| v["active"] == 0).await;
    assert_eq!(value["circuit_breaker"]["recoveries"], 1);
    assert_eq!(value["circuit_breaker"]["probes"], 2);
    gateway.shutdown().await.unwrap();
}

#[tokio::test]
async fn explicit_sse_server_error_is_recorded_before_http_eof() {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let gateway = spawn(config(listener.local_addr().unwrap())).await;
    let (finish, finished) = tokio::sync::oneshot::channel::<()>();
    let task = tokio::spawn(async move {
        let (mut socket, _) = listener.accept().await.unwrap();
        support::fixture::read_request(&mut socket).await.unwrap();
        let body = "data: {\"error\":{\"code\":500}}\n\n";
        let head = format!(
            "HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\nTransfer-Encoding: chunked\r\nConnection: close\r\n\r\n{:x}\r\n{body}\r\n",
            body.len()
        );
        socket.write_all(head.as_bytes()).await.unwrap();
        finished.await.unwrap();
        socket.write_all(b"0\r\n\r\n").await.unwrap();
    });
    let reply = request(&gateway).await;
    let value = wait_status(&gateway, |v| v["response_errors"]["server"] == 1).await;
    assert_eq!(value["terminal_body_eof"], 0);
    assert_eq!(value["circuit_breaker"]["failures"], 1);
    finish.send(()).unwrap();
    reply.bytes().await.unwrap();
    let value = wait_status(&gateway, |v| v["active"] == 0).await;
    assert_eq!(value["response_errors"]["server"], 1);
    assert_eq!(value["circuit_breaker"]["failures"], 1);
    task.await.unwrap();
    gateway.shutdown().await.unwrap();
}

#[tokio::test]
async fn consecutive_503_opens_without_an_extra_attempt_or_quota_charge() {
    let upstream = UpstreamFixture::start(
        b"HTTP/1.1 503 Unavailable\r\nContent-Length: 0\r\nConnection: close\r\n\r\n".to_vec(),
    )
    .await;
    let gateway = spawn(config(upstream.address())).await;
    for _ in 0..3 {
        let response = request(&gateway).await;
        assert_eq!(response.status(), 503);
        assert!(response.bytes().await.unwrap().is_empty());
    }
    let blocked = request(&gateway).await;
    assert_eq!(blocked.status(), 503);
    assert!(blocked.headers().contains_key("retry-after"));
    assert!(
        String::from_utf8(blocked.bytes().await.unwrap().to_vec())
            .unwrap()
            .contains("upstream_circuit_open")
    );
    assert_eq!(upstream.attempts(), 3);
    let value = status(&gateway).await;
    assert_eq!(value["circuit_breaker"]["opens"], 1);
    assert_eq!(value["admission"]["rpm_debited"], "3");
    gateway.shutdown().await.unwrap();
}

#[tokio::test]
async fn queued_attempts_recheck_open_circuit_before_quota_start() {
    let upstream = ControlledUpstream::start(vec![response(503, "", "")], false).await;
    let gateway = spawn(config(upstream.address)).await;
    let mut requests = tokio::task::JoinSet::new();
    for _ in 0..8 {
        let address = gateway.address();
        requests.spawn(async move {
            let response = send(address, "pi-work", "example-model", "synthetic-one", false).await;
            assert_eq!(response.status(), 503);
            response.bytes().await.unwrap()
        });
    }
    wait_status(&gateway, |v| v["admission"]["queue_length"] == 7).await;
    upstream.gate.send(true).unwrap();
    let mut blocked = 0;
    while let Some(result) = requests.join_next().await {
        if !result.unwrap().is_empty() {
            blocked += 1;
        }
    }
    assert_eq!(blocked, 5);
    assert_eq!(upstream.attempts(), 3);
    let value = wait_status(&gateway, |v| v["active"] == 0).await;
    assert_eq!(value["admission"]["rpm_debited"], "3");
    assert_eq!(value["admission"]["tpm_held"], "0");
    assert_eq!(value["circuit_breaker"]["tracked_attempts"], 0);
    gateway.shutdown().await.unwrap();
}

#[tokio::test]
async fn scopes_isolate_models_roots_credentials_and_client_errors_do_not_trip() {
    let upstream = ControlledUpstream::start(vec![response(503, "", "")], true).await;
    let mut cfg = config(upstream.address);
    let mut second_root = cfg.roots[0].clone();
    second_root.id = "other".into();
    cfg.roots.push(second_root);
    let gateway = spawn(cfg).await;
    for _ in 0..3 {
        request(&gateway).await.bytes().await.unwrap();
    }
    for (root, model, credential) in [
        ("other", "example-model", "synthetic-one"),
        ("pi-work", "request-bounded-model", "synthetic-one"),
        ("pi-work", "example-model", "synthetic-two"),
    ] {
        let result = send(gateway.address(), root, model, credential, false).await;
        assert_eq!(result.status(), 503);
        assert!(
            result.bytes().await.unwrap().is_empty(),
            "isolated scope still reaches upstream"
        );
    }
    assert_eq!(upstream.attempts(), 6);
    for code in [400, 401, 403, 404, 429] {
        upstream.set(vec![response(code, "Retry-After: 0\r\n", "{}")]);
        for _ in 0..4 {
            let result = send(
                gateway.address(),
                "other",
                "example-model",
                "synthetic-two",
                false,
            )
            .await;
            assert_eq!(result.status().as_u16(), code);
            result.bytes().await.unwrap();
        }
    }
    assert_eq!(upstream.attempts(), 26);
    assert_eq!(status(&gateway).await["circuit_breaker"]["opens"], 1);
    gateway.shutdown().await.unwrap();
}

#[tokio::test]
async fn cache_hits_survive_open_circuit_and_recovery_admits_one_probe_including_429_retry() {
    let upstream = ControlledUpstream::start(vec![response(200, "", GOOD)], true).await;
    let mut cfg = config(upstream.address);
    cfg.cache = Some(config::CacheConfig::default());
    cfg.retry_transient_429 = true;
    cfg.concurrency = 3;
    let gateway = spawn(cfg).await;
    assert_eq!(request(&gateway).await.bytes().await.unwrap(), GOOD);
    wait_status(&gateway, |v| v["exact_cache"]["stores"] == 1).await;
    upstream.set(vec![response(503, "", "")]);
    for _ in 0..3 {
        send(
            gateway.address(),
            "pi-work",
            "example-model",
            "synthetic-one",
            true,
        )
        .await
        .bytes()
        .await
        .unwrap();
    }
    assert_eq!(request(&gateway).await.bytes().await.unwrap(), GOOD);
    assert_eq!(upstream.attempts(), 4, "cache hit creates no outage probe");
    upstream.set(vec![
        response(
            429,
            "Retry-After: 0\r\n",
            r#"{"error":{"code":"slow_down","type":"rate_limit_error"}}"#,
        ),
        response(200, "", GOOD),
    ]);
    upstream.gate.send(false).unwrap();
    tokio::time::sleep(Duration::from_millis(5050)).await;
    let address = gateway.address();
    let probe = tokio::spawn(async move {
        send(address, "pi-work", "example-model", "synthetic-one", true)
            .await
            .bytes()
            .await
            .unwrap()
    });
    wait_status(&gateway, |v| v["circuit_breaker"]["half_open"] == 1).await;
    let follower = tokio::spawn(async move {
        send(address, "pi-work", "example-model", "synthetic-one", true)
            .await
            .bytes()
            .await
            .unwrap()
    });
    tokio::time::sleep(Duration::from_millis(30)).await;
    assert!(
        !follower.is_finished(),
        "a half-open follower must await the real probe"
    );
    assert_eq!(upstream.attempts(), 5);
    upstream.gate.send(true).unwrap();
    assert_eq!(probe.await.unwrap(), GOOD);
    assert_eq!(follower.await.unwrap(), GOOD);
    assert_eq!(
        upstream.attempts(),
        7,
        "429 retry acquires its own guard and cannot reject itself"
    );
    let value = wait_status(&gateway, |v| v["active"] == 0).await;
    assert_eq!(value["circuit_breaker"]["recoveries"], 1);
    assert_eq!(value["circuit_breaker"]["tracked_attempts"], 0);
    assert_eq!(value["admission"]["rpm_debited"], "7");
    gateway.shutdown().await.unwrap();
}

#[tokio::test]
async fn late_half_open_race_releases_quota_and_keeps_original_admission_deadline() {
    let upstream =
        ControlledUpstream::start(vec![response(503, "Retry-After-Ms: 1\r\n", "")], true).await;
    let mut cfg = config(upstream.address);
    cfg.concurrency = 2;
    let clock = llmgw::admission::ManualClock::default();
    let (gate, rx) = watch::channel(true);
    let gateway = server::testing::spawn_with_clock(
        cfg,
        RuntimeCredentials::new(b"synthetic-control", None).unwrap(),
        clock.clone(),
        Some(rx),
    )
    .await
    .unwrap();
    for _ in 0..3 {
        request(&gateway).await.bytes().await.unwrap();
    }
    wait_status(&gateway, |v| {
        v["active"] == 0 && v["circuit_breaker"]["open"] == 1
    })
    .await;
    upstream.set(vec![response(200, "", GOOD)]);
    upstream.gate.send(false).unwrap();
    gate.send(false).unwrap();
    tokio::time::sleep(Duration::from_millis(1050)).await;
    let mut requests = tokio::task::JoinSet::new();
    for _ in 0..2 {
        let address = gateway.address();
        requests.spawn(async move {
            let response = send(address, "pi-work", "example-model", "synthetic-one", true).await;
            (response.status().as_u16(), response.bytes().await.unwrap())
        });
    }
    wait_status(&gateway, |v| v["active"] == 2).await;
    assert_eq!(upstream.attempts(), 3);
    clock.advance_to(Duration::from_secs(119));
    gate.send(true).unwrap();
    let value = wait_status(&gateway, |v| {
        v["circuit_breaker"]["waiters"] == 1 && v["admission"]["active"] == 1
    })
    .await;
    assert_eq!(upstream.attempts(), 4, "only the probe reaches the wire");
    assert_eq!(
        value["admission"]["rpm_debited"], "1",
        "the other reservation is canceled without debit"
    );
    assert_eq!(server::testing::quota_snapshot(&gateway).starts, 4);
    clock.advance_to(Duration::from_secs(120));
    upstream.gate.send(true).unwrap();
    let mut codes = Vec::new();
    while let Some(result) = requests.join_next().await {
        let (code, body) = result.unwrap();
        if code == 200 {
            assert_eq!(body, GOOD);
        } else {
            assert!(
                String::from_utf8(body.to_vec())
                    .unwrap()
                    .contains("gateway_queue_deadline")
            );
        }
        codes.push(code);
    }
    codes.sort_unstable();
    assert_eq!(codes, [200, 504]);
    let value = wait_status(&gateway, |v| {
        v["active"] == 0 && v["circuit_breaker"]["waiters"] == 0
    })
    .await;
    assert_eq!(upstream.attempts(), 4);
    assert_eq!(value["circuit_breaker"]["recoveries"], 1);
    gateway.shutdown().await.unwrap();
}

#[tokio::test]
async fn canceled_half_open_probe_wakes_follower_without_claiming_recovery() {
    use support::fixture::{abort_socket, open_raw};
    let upstream =
        ControlledUpstream::start(vec![response(503, "Retry-After-Ms: 1\r\n", "")], true).await;
    let mut cfg = config(upstream.address);
    cfg.cancel_policy = config::CancelPolicy::Close;
    cfg.concurrency = 2;
    let gateway = spawn(cfg).await;
    for _ in 0..3 {
        request(&gateway).await.bytes().await.unwrap();
    }
    wait_status(&gateway, |v| v["active"] == 0).await;
    upstream.set(vec![response(200, "", GOOD)]);
    upstream.gate.send(false).unwrap();
    tokio::time::sleep(Duration::from_millis(1050)).await;
    let body =
        r#"{"model":"example-model","messages":[{"role":"user","content":"synthetic request"}]}"#;
    let raw = format!(
        "POST /r/pi-work/v1/chat/completions HTTP/1.1\r\nHost: localhost\r\nAuthorization: Bearer synthetic-one\r\nContent-Type: application/json\r\nContent-Length: {}\r\n\r\n{body}",
        body.len()
    );
    let probe = open_raw(gateway.address(), raw.as_bytes()).await;
    wait_status(&gateway, |v| {
        v["circuit_breaker"]["half_open"] == 1 && upstream.attempts() == 4
    })
    .await;
    let address = gateway.address();
    let follower = tokio::spawn(async move {
        send(address, "pi-work", "example-model", "synthetic-one", true)
            .await
            .bytes()
            .await
            .unwrap()
    });
    wait_status(&gateway, |v| v["circuit_breaker"]["waiters"] == 1).await;
    abort_socket(probe);
    let value = wait_status(&gateway, |v| {
        v["circuit_breaker"]["probes"] == 2 && upstream.attempts() == 5
    })
    .await;
    assert_eq!(value["circuit_breaker"]["recoveries"], 0);
    assert_eq!(value["circuit_breaker"]["waiters"], 0);
    assert!(!follower.is_finished());
    upstream.gate.send(true).unwrap();
    assert_eq!(follower.await.unwrap(), GOOD);
    let value = wait_status(&gateway, |v| v["active"] == 0).await;
    assert_eq!(value["circuit_breaker"]["recoveries"], 1);
    assert_eq!(value["circuit_breaker"]["tracked_attempts"], 0);
    assert_eq!(upstream.attempts(), 5);
    gateway.shutdown().await.unwrap();
}

#[tokio::test]
async fn pre_header_and_body_transport_failures_trip_without_replaying_partial_responses() {
    for reply in [
        vec![],
        b"HTTP/1.1 200 OK\r\nContent-Length: 50\r\nConnection: close\r\n\r\ntruncated".to_vec(),
        b"HTTP/1.1 503 Unavailable\r\nContent-Length: 50\r\nConnection: close\r\n\r\ntruncated"
            .to_vec(),
    ] {
        let upstream = UpstreamFixture::start(reply).await;
        let gateway = spawn(config(upstream.address())).await;
        for _ in 0..3 {
            let _ = request(&gateway).await.bytes().await;
        }
        let blocked = request(&gateway).await;
        assert_eq!(blocked.status(), 503);
        assert!(
            String::from_utf8(blocked.bytes().await.unwrap().to_vec())
                .unwrap()
                .contains("upstream_circuit_open")
        );
        assert_eq!(upstream.attempts(), 3);
        assert_eq!(status(&gateway).await["circuit_breaker"]["failures"], 3);
        gateway.shutdown().await.unwrap();
    }
}

#[tokio::test]
async fn body_deadlines_with_slow_downstream_do_not_mark_upstream_unhealthy() {
    use support::fixture::{abort_socket, open_raw, read_until};
    let upstream = UpstreamFixture::start(response(200, "", &"x".repeat(8 * 1024 * 1024))).await;
    let gateway = server::testing::spawn_with_timeouts(
        config(upstream.address()),
        RuntimeCredentials::new(b"synthetic-control", None).unwrap(),
        Duration::from_millis(150),
        Duration::from_millis(100),
    )
    .await
    .unwrap();
    for _ in 0..3 {
        let body = r#"{"model":"example-model","messages":[]}"#;
        let raw = format!(
            "POST /r/pi-work/v1/chat/completions HTTP/1.1\r\nHost: localhost\r\nContent-Type: application/json\r\nContent-Length: {}\r\n\r\n{body}",
            body.len()
        );
        let mut socket = open_raw(gateway.address(), raw.as_bytes()).await;
        read_until(&mut socket, b"\r\n\r\n").await;
        wait_status(&gateway, |v| v["active"] == 0).await;
        abort_socket(socket);
    }
    let value = status(&gateway).await;
    assert_eq!(upstream.attempts(), 3);
    assert_eq!(value["circuit_breaker"]["failures"], 0);
    assert_eq!(value["circuit_breaker"]["opens"], 0);
    gateway.shutdown().await.unwrap();
}

#[tokio::test]
async fn coalesced_followers_reuse_success_after_the_owners_authorized_429_retry() {
    let upstream = ControlledUpstream::start(
        vec![
            response(
                429,
                "Retry-After: 0\r\n",
                r#"{"error":{"code":"slow_down","type":"rate_limit_error"}}"#,
            ),
            response(200, "", GOOD),
        ],
        false,
    )
    .await;
    let mut cfg = config(upstream.address);
    cfg.cache = Some(config::CacheConfig::default());
    cfg.retry_transient_429 = true;
    cfg.concurrency = 3;
    let gateway = spawn(cfg).await;
    let mut requests = tokio::task::JoinSet::new();
    for _ in 0..3 {
        let address = gateway.address();
        requests.spawn(async move {
            send(address, "pi-work", "example-model", "synthetic-one", false)
                .await
                .bytes()
                .await
                .unwrap()
        });
    }
    wait_status(&gateway, |v| {
        v["exact_cache"]["waiters"] == 2 && upstream.attempts() == 1
    })
    .await;
    assert_eq!(upstream.attempts(), 1);
    upstream.gate.send(true).unwrap();
    while let Some(result) = requests.join_next().await {
        assert_eq!(result.unwrap(), GOOD);
    }
    let value = wait_status(&gateway, |v| {
        v["active"] == 0 && v["exact_cache"]["waiters"] == 0
    })
    .await;
    assert_eq!(upstream.attempts(), 2);
    assert_eq!(value["exact_cache"]["hits"], 2);
    assert_eq!(value["exact_cache"]["stores"], 1);
    assert_eq!(value["exact_cache"]["inflight_keys"], 0);
    assert_eq!(value["admission"]["rpm_debited"], "2");
    assert_eq!(value["circuit_breaker"]["tracked_attempts"], 0);
    gateway.shutdown().await.unwrap();
}
