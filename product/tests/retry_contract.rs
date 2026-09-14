mod support;
use llmgw::config::{
    Accounting, Auth, CancelPolicy, Config, Endpoint, Limit, Model, Quota, Root, Upstream,
};
use llmgw::server::{self, RuntimeCredentials};
use std::time::Duration;
use support::fixture::{UpstreamFixture, response_body, send_raw, status};
fn config(address: std::net::SocketAddr) -> Config {
    Config {
        listen: "127.0.0.1:0".parse().unwrap(),
        concurrency: 1,
        cancel_policy: CancelPolicy::Drain,
        accounting: Accounting::Reserved,
        retry_transient_429: false,
        upstream: Upstream {
            api_base: format!("http://{address}/v1").parse().unwrap(),
            auth: Auth::None,
            proxy: None,
            ca_bundle: None,
        },
        quota: Quota {
            rpm: Limit::Unlimited,
            tpm: Limit::Unknown,
        },
        models: vec![Model {
            id: "fixture".into(),
            max_output_tokens: None,
        }],
        roots: ["a", "b"]
            .into_iter()
            .map(|id| Root {
                id: id.into(),
                models: vec!["fixture".into()],
                endpoints: vec![
                    Endpoint::Models,
                    Endpoint::ChatCompletions,
                    Endpoint::Messages,
                    Endpoint::CountTokens,
                ],
            })
            .collect(),
    }
}
fn request(path: &str) -> Vec<u8> {
    format!("GET {path} HTTP/1.1\r\nHost: localhost\r\nX-LLMGW-Token: synthetic-data\r\nConnection: close\r\n\r\n").into_bytes()
}
fn rejection(header: &str, body: &[u8]) -> Vec<u8> {
    [format!("HTTP/1.1 429 Too Many Requests\r\nContent-Type: application/json\r\nContent-Length: {}\r\n{header}Connection: close\r\n\r\n", body.len()).into_bytes(), body.to_vec()].concat()
}
#[tokio::test]
async fn retry_after_survives_request_deadline_and_new_root_metadata_client() {
    let raw = br#"{"error":{"type":"rate_limit_error"}}"#;
    let upstream = UpstreamFixture::start(rejection("Retry-After: 2\r\n", raw)).await;
    let gateway = server::testing::spawn_with_timeouts(
        config(upstream.address()),
        RuntimeCredentials::new(b"synthetic-data", b"synthetic-control", None).unwrap(),
        Duration::from_millis(150),
        Duration::from_millis(100),
    )
    .await
    .unwrap();
    let first = send_raw(gateway.address(), &request("/r/a/v1/models")).await;
    assert_eq!(status(&first), 429);
    assert_eq!(response_body(&first), raw);
    let second = send_raw(gateway.address(), &request("/r/b/v1/models")).await;
    assert_eq!(
        status(&second),
        504,
        "new client's metadata must wait for shared cooldown"
    );
    let third = send_raw(gateway.address(), &request("/r/a/v1/models")).await;
    assert_eq!(
        status(&third),
        504,
        "the first deadline must not truncate group cooldown"
    );
    assert_eq!(
        upstream.attempts(),
        1,
        "three ingress requests, one actual Reqwest wire attempt"
    );
    gateway.shutdown().await.unwrap();
}

fn opted_config(address: std::net::SocketAddr) -> Config {
    let path = std::env::temp_dir().join(format!(
        "llmgw-task7-retry-{}-{:?}.toml",
        std::process::id(),
        std::thread::current().id()
    ));
    let original = std::fs::read_to_string("examples/fixture.toml").unwrap();
    std::fs::write(&path, format!("retry_transient_429 = true\n{original}")).unwrap();
    let loaded = llmgw::config::LoadedConfig::load(&path);
    std::fs::remove_file(path).unwrap();
    let mut loaded = loaded
        .expect("explicit opt-in configuration must be accepted")
        .config;
    let c = config(address);
    loaded.listen = c.listen;
    loaded.upstream = c.upstream;
    loaded.quota = c.quota;
    loaded.models = c.models;
    loaded.roots = c.roots;
    loaded
}
#[tokio::test]
async fn opt_in_replays_recognized_rejection_once_with_exact_body_and_two_rpm_charges() {
    let raw = br#"{"error":{"type":"rate_limit_error","code":"slow_down"}}"#;
    let upstream = UpstreamFixture::start(rejection("Retry-After: 0\r\n", raw)).await;
    let mut config = opted_config(upstream.address());
    config.quota.rpm = Limit::Known(10.try_into().unwrap());
    let clock = llmgw::admission::ManualClock::default();
    let gateway = server::testing::spawn_with_clock(
        config,
        RuntimeCredentials::new(b"synthetic-data", b"synthetic-control", None).unwrap(),
        clock.clone(),
        None,
    )
    .await
    .unwrap();
    clock.advance_to(Duration::from_secs(60));
    let response = send_raw(gateway.address(), &request("/r/a/v1/models")).await;
    assert_eq!(status(&response), 429);
    assert_eq!(response_body(&response), raw);
    assert_eq!(
        upstream.attempts(),
        2,
        "one ingress, exactly two actual wire attempts, never three"
    );
    let ledger = server::testing::quota_snapshot(&gateway);
    assert_eq!(ledger.starts, 2);
    assert_eq!(ledger.rpm_debited, 2);
    gateway.shutdown().await.unwrap();
}

async fn one_response_case(body: &[u8], headers: &str, enabled: bool, expected: usize) {
    let upstream = UpstreamFixture::start(rejection(headers, body)).await;
    let c = if enabled {
        opted_config(upstream.address())
    } else {
        config(upstream.address())
    };
    let gateway = server::spawn(
        c,
        RuntimeCredentials::new(b"synthetic-data", b"synthetic-control", None).unwrap(),
    )
    .await
    .unwrap();
    let response = send_raw(gateway.address(), &request("/r/a/v1/models")).await;
    assert_eq!(status(&response), 429);
    assert_eq!(response_body(&response), body);
    assert_eq!(
        upstream.attempts(),
        expected,
        "actual wire attempts for case {headers:?}"
    );
    gateway.shutdown().await.unwrap();
}
#[tokio::test]
async fn permanent_unknown_conflicting_malformed_encoded_and_oversized_errors_are_not_replayed() {
    let cases: &[&[u8]] = &[
        br#"{"error":{"code":"insufficient_quota"}}"#,
        br#"{"error":{"code":"credit_balance_exhausted","type":"rate_limit_error"}}"#,
        br#"{"error":{"code":"organization_spend_limit_exceeded"}}"#,
        br#"{"error":{"code":"project_spend_limit_exceeded"}}"#,
        br#"{"error":{"code":"organization_usage_limit_exceeded"}}"#,
        br#"{"error":{"type":"rate_limit_error","details":{"error_code":"enforced_spend_limit_reached"}}}"#,
        br#"{"error":{"type":"rate_limit_error"}}"#,
        br#"{"error":{"code":"future_unknown"}}"#,
        br#"{"error":{"code":"slow_down","type":"insufficient_quota"}}"#,
        br#"{"error":{"code":"slow_down","details":{"error_code":"enforced_spend_limit_reached"}}}"#,
        br#"{"error":{"code":"slow_down","code":"slow_down"}}"#,
        br#"{"error":{"code":"slow_down","type":"insufficient_quota","type":"rate_limit_error"}}"#,
        br#"{"error":{"code":"slow_down"},"error":{"code":"slow_down"}}"#,
        br#"{"error":{"code":"slow_down"},"code":"insufficient_quota"}"#,
        br#"{"error":{"code":"slow_down"}} trailing"#,
    ];
    for body in cases {
        one_response_case(body, "Retry-After: 0\r\n", true, 1).await;
    }
    let transient = br#"{"error":{"code":"slow_down","type":"rate_limit_error"}}"#;
    one_response_case(transient, "Retry-After: 0\r\n", false, 1).await;
    one_response_case(
        transient,
        "Retry-After: 0\r\nContent-Encoding: gzip\r\n",
        true,
        1,
    )
    .await;
    let mut large = transient.to_vec();
    large.resize(32 * 1024, b' ');
    one_response_case(&large, "Retry-After: 0\r\n", true, 1).await;
}
#[tokio::test]
async fn invalid_delay_does_not_turn_recognized_body_into_an_early_retry() {
    for value in [
        "-1",
        "NaN",
        "inf",
        "1.5",
        "2seconds",
        "+2",
        "18446744073709551616",
    ] {
        one_response_case(
            br#"{"error":{"code":"slow_down"}}"#,
            &format!("Retry-After: {value}\r\n"),
            true,
            1,
        )
        .await;
        one_response_case(
            br#"{"error":{"code":"slow_down"}}"#,
            &format!("Retry-After-Ms: {value}\r\n"),
            true,
            1,
        )
        .await;
    }
}
#[tokio::test]
async fn recognized_without_header_waits_one_second_with_jitter_and_replays_only_once() {
    let start = tokio::time::Instant::now();
    one_response_case(br#"{"error":{"code":"slow_down"}}"#, "", true, 2).await;
    assert!(start.elapsed() >= Duration::from_secs(1));
    assert!(start.elapsed() < Duration::from_secs(3));
}
#[tokio::test]
async fn seconds_http_date_and_ms_share_cooldown_even_with_retry_off() {
    for header in [
        "Retry-After: 1\r\n".to_owned(),
        format!(
            "Retry-After: {}\r\n",
            httpdate::fmt_http_date(std::time::SystemTime::now() + Duration::from_secs(20))
        ),
        "Retry-After-Ms: 1000\r\n".into(),
        "Retry-After: 0\r\nRetry-After-Ms: 1000\r\n".into(),
        "Retry-After: 1\r\nRetry-After: 0\r\n".into(),
        "Retry-After: 18446744073709551615\r\n".into(),
    ] {
        let upstream = UpstreamFixture::start(rejection(&header, br#"{"unknown":true}"#)).await;
        let gateway = server::testing::spawn_with_timeouts(
            config(upstream.address()),
            RuntimeCredentials::new(b"synthetic-data", b"synthetic-control", None).unwrap(),
            Duration::from_millis(80),
            Duration::from_millis(100),
        )
        .await
        .unwrap();
        assert_eq!(
            status(&send_raw(gateway.address(), &request("/r/a/v1/models")).await),
            429
        );
        assert_eq!(
            status(&send_raw(gateway.address(), &request("/r/b/v1/models")).await),
            504,
            "{header:?}"
        );
        assert_eq!(upstream.attempts(), 1);
        gateway.shutdown().await.unwrap();
    }
}
#[tokio::test]
async fn ordinary_rate_limit_code_accepts_only_compatible_type() {
    for kind in ["rate_limit_error", "requests", "tokens"] {
        let body = format!(r#"{{"error":{{"code":"rate_limit_exceeded","type":"{kind}"}}}}"#);
        one_response_case(body.as_bytes(), "Retry-After: 0\r\n", true, 2).await;
    }
}

#[tokio::test]
async fn library_retry_wait_has_no_slot_preserves_age_deadline_and_unknown_tpm_debt() {
    use llmgw::admission::quota::RequestCost;
    use llmgw::admission::{Admission, ManualClock};
    let mut cfg = config("127.0.0.1:9".parse().unwrap());
    cfg.quota.rpm = Limit::Known(10.try_into().unwrap());
    cfg.quota.tpm = Limit::Known(10.try_into().unwrap());
    let clock = ManualClock::default();
    let admission = Admission::new(&cfg, Some(clock.clone()));
    clock.advance_to(Duration::from_secs(60));
    let mut first = admission
        .acquire(
            0,
            RequestCost::exact_fixture(5),
            Endpoint::ChatCompletions,
            Duration::from_secs(120),
        )
        .await
        .unwrap();
    first.start();
    clock.advance_to(Duration::from_secs(61));
    admission.cooldown(Duration::from_secs(10));
    let retry = tokio::spawn(first.retry());
    tokio::task::yield_now().await;
    let waiting = admission.snapshot();
    assert_eq!(waiting.active, 0);
    assert_eq!(waiting.rpm_debited, 1);
    assert_eq!(
        waiting.tpm_debited, 5,
        "429 is no basis for a guessed TPM refund"
    );
    clock.advance_to(Duration::from_secs(70));
    tokio::task::yield_now().await;
    assert!(!retry.is_finished());
    clock.advance_to(Duration::from_secs(71));
    let mut second = retry.await.unwrap().unwrap();
    assert_eq!(second.wait_started(), Duration::from_secs(60));
    assert_eq!(second.queue_deadline(), Duration::from_secs(180));
    second.start();
    assert_eq!(admission.snapshot().rpm_debited, 2);
    drop(second);
    assert_eq!(admission.snapshot().tpm_debited, 10);
}
#[tokio::test]
async fn real_retry_reenters_known_rpm_and_has_no_slot_while_waiting() {
    let upstream = UpstreamFixture::start(rejection(
        "Retry-After: 0\r\n",
        br#"{"error":{"code":"slow_down"}}"#,
    ))
    .await;
    let mut cfg = opted_config(upstream.address());
    cfg.quota.rpm = Limit::Known(1.try_into().unwrap());
    let clock = llmgw::admission::ManualClock::default();
    let gateway = server::testing::spawn_with_clock(
        cfg,
        RuntimeCredentials::new(b"synthetic-data", b"synthetic-control", None).unwrap(),
        clock.clone(),
        None,
    )
    .await
    .unwrap();
    clock.advance_to(Duration::from_secs(60));
    let address = gateway.address();
    let response = tokio::spawn(async move { send_raw(address, &request("/r/a/v1/models")).await });
    upstream.captures(1).await;
    tokio::time::timeout(Duration::from_secs(1), async {
        while server::testing::quota_snapshot(&gateway).active != 0 {
            tokio::task::yield_now().await;
        }
    })
    .await
    .unwrap();
    clock.advance_to(Duration::from_secs(119));
    tokio::time::sleep(Duration::from_millis(30)).await;
    assert_eq!(upstream.attempts(), 1);
    assert_eq!(server::testing::quota_snapshot(&gateway).rpm_debited, 1);
    assert!(!response.is_finished());
    clock.advance_to(Duration::from_secs(120));
    assert_eq!(status(&response.await.unwrap()), 429);
    assert_eq!(upstream.attempts(), 2);
    assert_eq!(server::testing::quota_snapshot(&gateway).starts, 2);
    gateway.shutdown().await.unwrap();
}
#[tokio::test]
async fn retry_original_queue_deadline_expires_without_acquiring_or_refreshing_age() {
    use llmgw::admission::{AcquireError, Admission, ManualClock};
    let clock = ManualClock::default();
    let admission = Admission::new(&config("127.0.0.1:9".parse().unwrap()), Some(clock.clone()));
    let mut first = admission
        .acquire(
            0,
            llmgw::admission::quota::RequestCost::Metadata,
            Endpoint::Models,
            Duration::from_secs(2),
        )
        .await
        .unwrap();
    first.start();
    clock.advance_to(Duration::from_secs(1));
    admission.cooldown(Duration::from_secs(10));
    let retry = tokio::spawn(first.retry());
    tokio::task::yield_now().await;
    clock.advance_to(Duration::from_secs(2));
    assert!(matches!(retry.await.unwrap(), Err(AcquireError::Deadline)));
    assert_eq!(admission.snapshot().active, 0);
    assert_eq!(admission.snapshot().starts, 1);
}
#[tokio::test]
async fn socket_ambiguity_503_redirect_and_partial_stream_have_exactly_one_wire_attempt_even_opted_in()
 {
    for (raw, expected_status) in [
        (Vec::new(), 502),
        (b"HTTP/1.1 503 Unavailable\r\nRetry-After: 0\r\nContent-Length: 0\r\nConnection: close\r\n\r\n".to_vec(), 503),
        (b"HTTP/1.1 307 Redirect\r\nLocation: /v1/models\r\nContent-Length: 0\r\nConnection: close\r\n\r\n".to_vec(), 307),
        (b"HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\nContent-Length: 999\r\nConnection: close\r\n\r\ndata: {\"error\":{\"code\":\"slow_down\"}}\n\n".to_vec(), 200),
    ] {
        let upstream = UpstreamFixture::start(raw).await;
        let gateway = server::spawn(opted_config(upstream.address()), RuntimeCredentials::new(b"synthetic-data", b"synthetic-control", None).unwrap()).await.unwrap();
        let body = br#"{"model":"fixture","messages":[]}"#;
        let req = [format!("POST /r/a/v1/chat/completions HTTP/1.1\r\nHost: localhost\r\nX-LLMGW-Token: synthetic-data\r\nContent-Length: {}\r\nConnection: close\r\n\r\n", body.len()).into_bytes(), body.to_vec()].concat();
        let response = send_raw(gateway.address(), &req).await;
        assert_eq!(status(&response), expected_status);
        assert_eq!(upstream.attempts(), 1);
        assert_eq!(upstream.capture().await.body, body);
        gateway.shutdown().await.unwrap();
    }
}

async fn gateway_status(address: std::net::SocketAddr) -> serde_json::Value {
    let response = send_raw(address, b"GET /_llmgw/status HTTP/1.1\r\nHost: localhost\r\nX-LLMGW-Control-Token: synthetic-control\r\nConnection: close\r\n\r\n").await;
    serde_json::from_slice(response_body(&response)).unwrap()
}
#[tokio::test]
async fn canceled_retry_queue_releases_request_body_and_never_starts_a_second_attempt() {
    let upstream = UpstreamFixture::start(rejection(
        "Retry-After: 10\r\n",
        br#"{"error":{"code":"rate_limit_exceeded","type":"tokens"}}"#,
    ))
    .await;
    let gateway = server::spawn(
        opted_config(upstream.address()),
        RuntimeCredentials::new(b"synthetic-data", b"synthetic-control", None).unwrap(),
    )
    .await
    .unwrap();
    let body = br#"{"model":"fixture","messages":[],"synthetic_prompt":"task7-prompt-sentinel"}"#;
    let req = [format!("POST /r/a/v1/chat/completions HTTP/1.1\r\nHost: localhost\r\nX-LLMGW-Token: synthetic-data\r\nAuthorization: Bearer task7-credential-sentinel\r\nContent-Length: {}\r\n\r\n",body.len()).into_bytes(), body.to_vec()].concat();
    let client = support::fixture::open_raw(gateway.address(), &req).await;
    tokio::time::timeout(Duration::from_secs(2), async {
        while gateway_status(gateway.address()).await["admission"]["queue_length"] != 1 {
            tokio::task::yield_now().await;
        }
    })
    .await
    .unwrap();
    let waiting = gateway_status(gateway.address()).await;
    assert_eq!(waiting["admission"]["active"], 0);
    assert_eq!(waiting["stored_request_bytes"], body.len());
    support::fixture::abort_socket(client);
    tokio::time::timeout(Duration::from_secs(2), async {
        loop {
            let s = gateway_status(gateway.address()).await;
            if s["active"] == 0
                && s["stored_request_bytes"] == 0
                && s["admission"]["queue_length"] == 0
            {
                break;
            }
            tokio::task::yield_now().await;
        }
    })
    .await
    .expect("retry cancellation releases body, worker, and admission ticket");
    assert_eq!(upstream.attempts(), 1);
    let status = gateway_status(gateway.address()).await.to_string();
    for sentinel in [
        "task7-prompt-sentinel",
        "task7-credential-sentinel",
        "synthetic-data",
        "synthetic-control",
    ] {
        assert!(!status.contains(sentinel));
    }
    gateway.shutdown().await.unwrap();
}
#[tokio::test]
async fn opted_retry_deadline_keeps_cooldown_for_the_next_client() {
    let upstream = UpstreamFixture::start(rejection(
        "Retry-After-Ms: 1000\r\n",
        br#"{"error":{"code":"slow_down"}}"#,
    ))
    .await;
    let gateway = server::testing::spawn_with_timeouts(
        opted_config(upstream.address()),
        RuntimeCredentials::new(b"synthetic-data", b"synthetic-control", None).unwrap(),
        Duration::from_millis(100),
        Duration::from_millis(100),
    )
    .await
    .unwrap();
    for path in ["/r/a/v1/models", "/r/b/v1/models"] {
        assert_eq!(
            status(&send_raw(gateway.address(), &request(path)).await),
            504
        );
    }
    assert_eq!(upstream.attempts(), 1);
    assert_eq!(
        gateway_status(gateway.address()).await["stored_request_bytes"],
        0
    );
    gateway.shutdown().await.unwrap();
}
#[tokio::test]
async fn chunked_error_observation_overflow_is_transparent_and_never_replayed() {
    let mut body = br#"{"error":{"code":"slow_down"}}"#.to_vec();
    body.resize(64 * 1024, b' ');
    let mut raw = b"HTTP/1.1 429 Too Many Requests\r\nContent-Type: application/json\r\nTransfer-Encoding: chunked\r\nRetry-After: 0\r\nConnection: close\r\n\r\n".to_vec();
    for chunk in body.chunks(8192) {
        raw.extend_from_slice(format!("{:x}\r\n", chunk.len()).as_bytes());
        raw.extend_from_slice(chunk);
        raw.extend_from_slice(b"\r\n");
    }
    raw.extend_from_slice(b"0\r\n\r\n");
    let upstream = UpstreamFixture::start(raw).await;
    let gateway = server::spawn(
        opted_config(upstream.address()),
        RuntimeCredentials::new(b"synthetic-data", b"synthetic-control", None).unwrap(),
    )
    .await
    .unwrap();
    let response = reqwest::Client::builder()
        .retry(reqwest::retry::never())
        .redirect(reqwest::redirect::Policy::none())
        .build()
        .unwrap()
        .get(format!("http://{}/r/a/v1/models", gateway.address()))
        .header("x-llmgw-token", "synthetic-data")
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), 429);
    assert_eq!(response.headers()["retry-after"], "0");
    assert_eq!(response.bytes().await.unwrap().as_ref(), body);
    assert_eq!(upstream.attempts(), 1);
    gateway.shutdown().await.unwrap();
}

#[tokio::test]
async fn invalid_utf8_in_ignored_error_payload_cannot_qualify_replay() {
    for bytes in [
        &b"{\"error\":{\"code\":\"slow_down\",\"message\":\"\xff\"}}"[..],
        &br#"{"error":{"code":"slow_down"},"extra":"\ud800"}"#[..],
        &b"{\"error\":{\"code\":\"slow_down\"},\"extra\":{\"key\":\"\xff\"}}"[..],
        &b"{\"error\":{\"code\":\"slow_down\"},\"extra\":{\"\xff\":true}}"[..],
    ] {
        one_response_case(bytes, "Retry-After: 0\r\n", true, 1).await;
    }
}

#[tokio::test]
async fn full_queue_on_retry_returns_protocol_429_and_is_not_a_deadline() {
    use tokio::io::AsyncWriteExt;
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let (started, first_started) = tokio::sync::oneshot::channel();
    let (release, released) = tokio::sync::oneshot::channel();
    let upstream = tokio::spawn(async move {
        let (mut socket, _) = listener.accept().await.unwrap();
        support::fixture::read_until(&mut socket, b"\r\n\r\n").await;
        started.send(()).unwrap();
        released.await.unwrap();
        socket
            .write_all(&rejection(
                "Retry-After: 10\r\n",
                br#"{"error":{"code":"slow_down"}}"#,
            ))
            .await
            .unwrap();
        socket.shutdown().await.unwrap();
        // Keep the fixture listener owned until the test aborts it; no second accept is needed.
        std::future::pending::<()>().await;
    });
    let gateway = server::spawn(
        opted_config(address),
        RuntimeCredentials::new(b"synthetic-data", b"synthetic-control", None).unwrap(),
    )
    .await
    .unwrap();
    let address = gateway.address();
    let first = tokio::spawn(async move { send_raw(address, &request("/r/a/v1/models")).await });
    first_started.await.unwrap();
    let mut clients = Vec::new();
    for _ in 0..64 {
        clients.push(support::fixture::open_raw(address, &request("/r/b/v1/models")).await);
    }
    tokio::time::timeout(Duration::from_secs(2), async {
        while gateway_status(address).await["admission"]["queue_length"] != 64 {
            tokio::task::yield_now().await;
        }
    })
    .await
    .unwrap();
    release.send(()).unwrap();
    let first = first.await.unwrap();
    assert_eq!(status(&first), 429);
    let error: serde_json::Value = serde_json::from_slice(response_body(&first)).unwrap();
    assert_eq!(error["error"]["code"], "gateway_queue_full");
    let snapshot = gateway_status(address).await;
    assert_eq!(snapshot["upstream_attempts"], 1);
    assert_eq!(
        snapshot["terminal_deadline"], 0,
        "queue rejection must not claim a deadline"
    );
    assert_eq!(snapshot["terminal_admission_rejected"], 1);
    for client in clients {
        support::fixture::abort_socket(client);
    }
    gateway.shutdown().await.unwrap();
    upstream.abort();
    let _ = upstream.await;
}

#[tokio::test]
async fn positive_decimal_overflow_keeps_group_blocked_for_new_clients_even_with_zero_header() {
    for header in [
        "Retry-After: 18446744073709551616\r\nRetry-After: 0\r\n",
        "Retry-After-Ms: 18446744073709551616\r\nRetry-After: 0\r\n",
    ] {
        let upstream = UpstreamFixture::start(rejection(header, br#"{"unknown":true}"#)).await;
        let gateway = server::testing::spawn_with_timeouts(
            config(upstream.address()),
            RuntimeCredentials::new(b"synthetic-data", b"synthetic-control", None).unwrap(),
            Duration::from_millis(80),
            Duration::from_millis(100),
        )
        .await
        .unwrap();
        assert_eq!(
            status(&send_raw(gateway.address(), &request("/r/a/v1/models")).await),
            429
        );
        assert_eq!(
            status(&send_raw(gateway.address(), &request("/r/b/v1/models")).await),
            504,
            "positive decimal overflow cannot become an immediate group retry"
        );
        assert_eq!(upstream.attempts(), 1);
        gateway.shutdown().await.unwrap();
    }
}
#[tokio::test]
async fn opted_post_retry_preserves_the_identical_wire_body_and_headers() {
    for accounting in [Accounting::Reserved, Accounting::Actual] {
        let upstream = UpstreamFixture::start(rejection(
            "Retry-After: 0\r\n",
            br#"{"error":{"code":"slow_down"}}"#,
        ))
        .await;
        let mut cfg = opted_config(upstream.address());
        cfg.upstream.auth = Auth::Forward;
        cfg.accounting = accounting;
        cfg.quota.tpm = Limit::Known(1000.try_into().unwrap());
        cfg.models[0].max_output_tokens = Some(1.try_into().unwrap());
        let clock = llmgw::admission::ManualClock::default();
        let gateway = server::testing::spawn_with_clock(
            cfg,
            RuntimeCredentials::new(b"synthetic-data", b"synthetic-control", None).unwrap(),
            clock.clone(),
            None,
        )
        .await
        .unwrap();
        clock.advance_to(Duration::from_secs(60));
        let body = br#"{ "model":"fixture", "messages":[], "user":"task7-prompt-sentinel" }"#;
        let req = [format!("POST /r/a/v1/chat/completions HTTP/1.1\r\nHost: localhost\r\nX-LLMGW-Token: synthetic-data\r\nAuthorization: Bearer task7-credential-sentinel\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",body.len()).into_bytes(), body.to_vec()].concat();
        assert_eq!(status(&send_raw(gateway.address(), &req).await), 429);
        let captures = upstream.captures(2).await;
        assert_eq!(captures.len(), 2);
        for c in captures {
            assert_eq!(c.body, body);
            assert_eq!(
                c.header("authorization"),
                Some(&b"Bearer task7-credential-sentinel"[..])
            );
        }
        let ledger = server::testing::quota_snapshot(&gateway);
        assert_eq!(ledger.starts, 2);
        assert_eq!(
            ledger.tpm_debited,
            2 * (body.len() as u128 + 1),
            "neither Reserved nor Actual invents rejected-attempt token refunds"
        );
        gateway.shutdown().await.unwrap();
    }
}

#[tokio::test]
async fn pre_head_rejection_body_failure_returns_502_keeps_cooldown_and_unknown_debt() {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    for accounting in [Accounting::Reserved, Accounting::Actual] {
        for chunked in [false, true] {
            let case = async {
                let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
                let address = listener.local_addr().unwrap();
                let (sent, received) = tokio::sync::oneshot::channel();
                let (release, released) = tokio::sync::oneshot::channel();
                let body = br#"{"model":"fixture","messages":[]}"#;
                let upstream = tokio::spawn(async move {
                    let (mut socket, _) = listener.accept().await.unwrap();
                    // Observe the entire actual POST before replying, including its final body byte.
                    support::fixture::read_until(&mut socket, body).await;
                    let framing = if chunked {
                        "Transfer-Encoding: chunked"
                    } else {
                        "Content-Length: 1000"
                    };
                    let rejection_body = r#"{"error":{"code":"slow_down"}}"#;
                    let prefix = if chunked {
                        format!("{:x}\r\n{rejection_body}\r\n", rejection_body.len())
                    } else {
                        rejection_body.to_owned()
                    };
                    socket.write_all(format!("HTTP/1.1 429 Too Many Requests\r\nContent-Type: application/json\r\nRetry-After: 5\r\n{framing}\r\nConnection: close\r\n\r\n{prefix}").as_bytes()).await.unwrap();
                    sent.send(()).unwrap();
                    released.await.unwrap();
                    socket.shutdown().await.unwrap();
                    let (mut fresh, _) = listener.accept().await.unwrap();
                    let observed = support::fixture::read_until(&mut fresh, b"\r\n\r\n").await;
                    assert!(
                        observed.starts_with(b"GET /v1/models "),
                        "only the fresh metadata request reaches upstream next"
                    );
                    fresh
                        .write_all(
                            b"HTTP/1.1 200 OK\r\nContent-Length: 2\r\nConnection: close\r\n\r\n{}",
                        )
                        .await
                        .unwrap();
                    fresh.shutdown().await.unwrap();
                    2usize
                });
                let mut cfg = opted_config(address);
                cfg.accounting = accounting;
                cfg.quota.rpm = Limit::Known(10.try_into().unwrap());
                cfg.quota.tpm = Limit::Known(1000.try_into().unwrap());
                cfg.models[0].max_output_tokens = Some(1.try_into().unwrap());
                let clock = llmgw::admission::ManualClock::default();
                let gateway = server::testing::spawn_with_clock(
                    cfg,
                    RuntimeCredentials::new(b"synthetic-data", b"synthetic-control", None).unwrap(),
                    clock.clone(),
                    None,
                )
                .await
                .unwrap();
                clock.advance_to(Duration::from_secs(60));
                let req = [format!("POST /r/a/v1/chat/completions HTTP/1.1\r\nHost: localhost\r\nX-LLMGW-Token: synthetic-data\r\nContent-Length: {}\r\nConnection: close\r\n\r\n", body.len()).into_bytes(), body.to_vec()].concat();
                let mut client = support::fixture::open_raw(gateway.address(), &req).await;
                received.await.unwrap();
                let mut response = Vec::new();
                assert!(
                    tokio::time::timeout(Duration::from_millis(50), client.read_buf(&mut response))
                        .await
                        .is_err(),
                    "no downstream head while probing the incomplete rejection"
                );
                release.send(()).unwrap();
                client.read_to_end(&mut response).await.unwrap();
                assert!(
                    response.starts_with(b"HTTP/1.1 502"),
                    "pre-head upstream failure must yield a protocol 502; received {} bytes",
                    response.len()
                );
                let error: serde_json::Value =
                    serde_json::from_slice(response_body(&response)).unwrap();
                assert_eq!(error["error"]["code"], "upstream_transport_error");
                let ledger = server::testing::quota_snapshot(&gateway);
                assert_eq!((ledger.starts, ledger.cleanups, ledger.active), (1, 1, 0));
                assert_eq!(ledger.rpm_debited, 1);
                assert_eq!(ledger.tpm_debited, body.len() as u128 + 1);
                assert_eq!(ledger.tpm_held, 0);
                let state = gateway_status(gateway.address()).await;
                assert_eq!(state["stored_request_bytes"], 0);
                assert_eq!(state["terminal_upstream_error"], 1);
                assert_eq!(state["response_body_eof"], 0);
                let address = gateway.address();
                let fresh =
                    tokio::spawn(
                        async move { send_raw(address, &request("/r/b/v1/models")).await },
                    );
                while gateway_status(address).await["admission"]["queue_length"] != 1 {
                    tokio::task::yield_now().await;
                }
                clock.advance_to(Duration::from_secs(64));
                tokio::time::sleep(Duration::from_millis(20)).await;
                assert!(
                    !fresh.is_finished(),
                    "other root cannot bypass the failed response's cooldown"
                );
                assert_eq!(server::testing::quota_snapshot(&gateway).starts, 1);
                clock.advance_to(Duration::from_secs(65));
                assert_eq!(status(&fresh.await.unwrap()), 200);
                assert_eq!(upstream.await.unwrap(), 2);
                let ledger = server::testing::quota_snapshot(&gateway);
                assert_eq!((ledger.starts, ledger.cleanups, ledger.active), (2, 2, 0));
                assert_eq!(ledger.rpm_debited, 2);
                assert_eq!(ledger.tpm_debited, body.len() as u128 + 1);
                assert_eq!(ledger.tpm_held, 0);
                gateway.shutdown().await.unwrap();
                eprintln!(
                    "Q7-1 framing={} accounting={accounting:?} data_ingress=2 upstream_attempts=2 http_502=1 http_200=1 replay=0 cleanup=2 active=0 held=0",
                    if chunked { "chunked" } else { "content-length" }
                );
            };
            tokio::time::timeout(Duration::from_secs(10), case)
                .await
                .expect("bounded pre-head transport regression");
        }
    }
}
