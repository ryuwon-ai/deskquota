mod support;

use std::net::SocketAddr;
use std::time::Duration;

use llmgw::config::{self, CacheConfig, Config};
use llmgw::server::{self, GatewayHandle, RuntimeCredentials};
use serde_json::{Value, json};
use support::fixture::{GatedUpstreamFixture, UpstreamFixture, abort_socket, open_raw, read_until};

const JSON_RESPONSE: &str = r#"{"id":"synthetic","choices":[{"index":0,"message":{"role":"assistant","content":"cached answer","refusal":null,"annotations":[]},"finish_reason":"stop"}],"usage":{"prompt_tokens":20,"completion_tokens":3}}"#;
const DELTA: &str = "data: {\"choices\":[{\"index\":0,\"delta\":{\"role\":\"assistant\",\"content\":\"cached answer\",\"refusal\":null,\"annotations\":[]},\"finish_reason\":null}]}\n\n";
const STOP: &str =
    "data: {\"choices\":[{\"index\":0,\"delta\":{},\"finish_reason\":\"stop\"}]}\n\n";
const USAGE: &str =
    "data: {\"choices\":[],\"usage\":{\"prompt_tokens\":20,\"completion_tokens\":3}}\n\n";
const DONE: &str = "data: [DONE]\n\n";

fn config(address: SocketAddr) -> Config {
    config::parse(
        format!(
            r#"
listen = "127.0.0.1:0"
startup_hold_secs = 0
[cache]
[upstream]
api_base = "http://{address}/v1?api-version=synthetic"
[upstream.auth]
mode = "forward"
[quota.rpm]
kind = "known"
value = 10000
[quota.tpm]
kind = "known"
value = 1000000
[[models]]
id = "synthetic"
max_output_tokens = 64
[[roots]]
id = "one"
endpoints = ["chat/completions", "responses", "messages"]
models = ["synthetic"]
[[roots]]
id = "two"
endpoints = ["chat/completions", "responses", "messages"]
models = ["synthetic"]
"#
        )
        .as_bytes(),
    )
    .unwrap()
}
async fn spawn_gateway(config: Config) -> GatewayHandle {
    server::spawn(
        config,
        RuntimeCredentials::new(b"synthetic-control", None).unwrap(),
    )
    .await
    .unwrap()
}
fn client() -> reqwest::Client {
    reqwest::Client::builder()
        .no_proxy()
        .timeout(Duration::from_secs(5))
        .build()
        .unwrap()
}
fn request(stream: bool) -> Value {
    json!({"model":"synthetic","messages":[{"role":"user","content":"synthetic classifier"}],"stream":stream,"max_tokens":64,"temperature":0})
}
fn upstream_response(status: u16, media: &str, headers: &str, body: &str) -> Vec<u8> {
    format!("HTTP/1.1 {status} Synthetic\r\nContent-Type: {media}\r\nContent-Length: {}\r\n{headers}Connection: close\r\n\r\n{body}", body.len()).into_bytes()
}
async fn send(
    gateway: &GatewayHandle,
    path: &str,
    body: &Value,
    headers: &[(&str, &str)],
) -> reqwest::Response {
    let mut request = client()
        .post(format!("http://{}{path}", gateway.address()))
        .header("content-type", "application/json")
        .header("authorization", "Bearer synthetic-one")
        .body(body.to_string());
    for (name, value) in headers {
        request = request.header(*name, *value);
    }
    request.send().await.unwrap()
}
async fn status(gateway: &GatewayHandle) -> Value {
    let response = client()
        .get(format!("http://{}/_llmgw/status", gateway.address()))
        .header("x-llmgw-control-token", "synthetic-control")
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), 200);
    serde_json::from_slice(&response.bytes().await.unwrap()).unwrap()
}
const PATH: &str = "/r/one/v1/chat/completions";

fn assert_not_stored(cache: &Value, reason: &str, expected: u64) {
    let counts = cache["not_stored"]
        .as_object()
        .expect("response cache diagnostics");
    assert_eq!(counts.len(), 7, "only bounded aggregate reason names");
    assert_eq!(counts[reason], expected, "{reason}");
    assert_eq!(
        counts
            .values()
            .map(|value| value.as_u64().unwrap())
            .sum::<u64>(),
        expected,
        "one terminal reason per response"
    );
}

#[tokio::test]
async fn response_cache_diagnostics_explain_no_cache_and_preserve_policy_precedence() {
    for stream in [false, true] {
        let media = if stream {
            "text/event-stream"
        } else {
            "application/json"
        };
        let output = if stream {
            [DELTA, STOP, USAGE, DONE].concat()
        } else {
            JSON_RESPONSE.to_owned()
        };
        let oversized = "x".repeat(270_000);
        for (code, media, headers, output, reason) in [
            (
                200,
                media,
                "Cache-Control: no-cache\r\n",
                output.as_str(),
                "response_cache_control",
            ),
            (
                200,
                media,
                "Cache-Control: no-cache\r\nSet-Cookie: synthetic=yes\r\n",
                oversized.as_str(),
                "response_cache_control",
            ),
            (
                500,
                media,
                "Cache-Control: no-cache\r\n",
                output.as_str(),
                "status",
            ),
            (200, media, "Vary: *\r\n", output.as_str(), "unsafe_headers"),
            (
                200,
                media,
                "Content-Encoding: gzip\r\n",
                output.as_str(),
                "encoding",
            ),
            (200, "text/plain", "", output.as_str(), "representation"),
            (200, media, "", oversized.as_str(), "size"),
        ] {
            let upstream =
                UpstreamFixture::start(upstream_response(code, media, headers, output)).await;
            let gateway = spawn_gateway(config(upstream.address())).await;
            for _ in 0..2 {
                let response = send(&gateway, PATH, &request(stream), &[]).await;
                assert_eq!(response.status(), code);
                assert_eq!(response.bytes().await.unwrap(), output);
            }
            let metrics = status(&gateway).await;
            let cache = &metrics["exact_cache"];
            assert_eq!(upstream.attempts(), 2);
            assert_eq!(cache["misses"], 2);
            assert_eq!(cache["stores"], 0);
            assert_eq!(cache["hits"], 0);
            assert_eq!(cache["retained_bytes"], 0);
            assert_not_stored(cache, reason, 2);
            gateway.shutdown().await.unwrap();
        }
    }
}

async fn wait_for_queue(gateway: &GatewayHandle, length: usize) {
    tokio::time::timeout(Duration::from_secs(2), async {
        while status(gateway).await["admission"]["queue_length"] != length {
            tokio::time::sleep(Duration::from_millis(2)).await;
        }
    })
    .await
    .expect("observed expected queue length");
}

async fn wait_for_cache_waiters(gateway: &GatewayHandle, length: usize) {
    tokio::time::timeout(Duration::from_secs(2), async {
        while status(gateway).await["exact_cache"]["waiters"] != length {
            tokio::time::sleep(Duration::from_millis(2)).await;
        }
    })
    .await
    .expect("observed expected exact-cache waiter count");
}

async fn queued_reuse(stream: bool, rpm_limited: bool) {
    let output = if stream {
        [DELTA, STOP, USAGE, DONE].concat()
    } else {
        JSON_RESPONSE.to_owned()
    };
    let upstream = if stream {
        GatedUpstreamFixture::start(
            vec![DELTA.as_bytes().to_vec()],
            vec![[STOP, USAGE, DONE].concat().into_bytes()],
        )
        .await
    } else {
        GatedUpstreamFixture::start_json(vec![], vec![output.clone().into_bytes()], false).await
    };
    let mut cfg = config(upstream.address());
    cfg.concurrency = 1;
    if rpm_limited {
        cfg.quota.rpm = config::Limit::Known(1.try_into().unwrap());
    }
    let gateway = spawn_gateway(cfg).await;
    let leader = send(&gateway, PATH, &request(stream), &[]).await;
    let mut followers = tokio::task::JoinSet::new();
    for _ in 0..2 {
        let address = gateway.address();
        followers.spawn(async move {
            let response = client()
                .post(format!("http://{address}{PATH}"))
                .header("content-type", "application/json")
                .bearer_auth("synthetic-one")
                .body(request(stream).to_string())
                .send()
                .await
                .unwrap();
            assert_eq!(response.status(), 200);
            response.bytes().await.unwrap()
        });
    }
    wait_for_cache_waiters(&gateway, 2).await;
    wait_for_queue(&gateway, 0).await;
    upstream.release_first();
    upstream.release_final();
    upstream.wait_for_final().await;
    assert_eq!(status(&gateway).await["exact_cache"]["stores"], 0);
    assert!(followers.try_join_next().is_none(), "no replay before EOF");
    upstream.release_eof();
    assert_eq!(leader.bytes().await.unwrap(), output);
    tokio::time::timeout(Duration::from_secs(1), async {
        while let Some(result) = followers.join_next().await {
            assert_eq!(result.unwrap(), output);
        }
    })
    .await
    .expect("queued duplicates must reuse the committed response without waiting for RPM expiry");
    wait_for_queue(&gateway, 0).await;
    let value = status(&gateway).await;
    assert_eq!(upstream.attempts(), 1);
    assert_eq!(value["exact_cache"]["considered"], 3);
    assert_eq!(value["exact_cache"]["hits"], 2);
    assert_eq!(value["exact_cache"]["misses"], 1);
    assert_eq!(value["exact_cache"]["coalesced"], 2);
    assert_eq!(value["exact_cache"]["waiters"], 0);
    assert_eq!(value["exact_cache"]["inflight_keys"], 0);
    for counter in ["upstream_attempts", "usage_known", "terminal_total"] {
        assert_eq!(value[counter], 1, "{counter}");
    }
    let quota = server::testing::quota_snapshot(&gateway);
    assert_eq!(quota.starts, 1);
    assert_eq!(quota.rpm_debited, 1);
    assert_eq!(quota.tpm_debited, 23);
    assert_eq!(quota.tpm_held, 0);
    assert_eq!(quota.active, 0);
    gateway.shutdown().await.unwrap();
}

async fn concurrent_reuse(stream: bool) {
    let output = if stream {
        [DELTA, STOP, USAGE, DONE].concat()
    } else {
        JSON_RESPONSE.to_owned()
    };
    let upstream = if stream {
        GatedUpstreamFixture::start(
            vec![DELTA.as_bytes().to_vec()],
            vec![[STOP, USAGE, DONE].concat().into_bytes()],
        )
        .await
    } else {
        GatedUpstreamFixture::start_json(vec![], vec![output.clone().into_bytes()], false).await
    };
    let mut cfg = config(upstream.address());
    cfg.concurrency = 3;
    let gateway = spawn_gateway(cfg).await;
    let leader = send(&gateway, PATH, &request(stream), &[]).await;
    let mut followers = tokio::task::JoinSet::new();
    for _ in 0..2 {
        let address = gateway.address();
        followers.spawn(async move {
            let response = client()
                .post(format!("http://{address}{PATH}"))
                .header("content-type", "application/json")
                .bearer_auth("synthetic-one")
                .body(request(stream).to_string())
                .send()
                .await
                .unwrap();
            assert_eq!(response.status(), 200);
            response.bytes().await.unwrap()
        });
    }
    tokio::time::timeout(Duration::from_secs(2), async {
        loop {
            let value = status(&gateway).await;
            if value["exact_cache"]["waiters"] == 2 || upstream.attempts() == 3 {
                break;
            }
            tokio::time::sleep(Duration::from_millis(2)).await;
        }
    })
    .await
    .expect("duplicates either coalesce or expose the old three-attempt behavior");
    upstream.release_first();
    upstream.release_final();
    upstream.release_eof();
    assert_eq!(leader.bytes().await.unwrap(), output);
    while let Some(result) = followers.join_next().await {
        assert_eq!(result.unwrap(), output);
    }
    assert_eq!(
        upstream.attempts(),
        1,
        "three concurrent exact requests must share one fill"
    );
    let value = status(&gateway).await;
    assert_eq!(value["exact_cache"]["hits"], 2);
    assert_eq!(value["exact_cache"]["coalesced"], 2);
    assert_eq!(value["exact_cache"]["waiters"], 0);
    assert_eq!(value["exact_cache"]["inflight_keys"], 0);
    assert_eq!(value["upstream_attempts"], 1);
    let quota = server::testing::quota_snapshot(&gateway);
    assert_eq!(quota.starts, 1);
    assert_eq!(quota.rpm_debited, 1);
    assert_eq!(quota.tpm_debited, 23);
    assert_eq!(quota.active, 0);
    assert_eq!(quota.tpm_held, 0);
    gateway.shutdown().await.unwrap();
}

#[tokio::test]
async fn concurrent_json_exact_requests_share_one_upstream_fill() {
    concurrent_reuse(false).await;
}

#[tokio::test]
async fn concurrent_sse_exact_requests_share_one_upstream_fill() {
    concurrent_reuse(true).await;
}

#[tokio::test]
async fn queued_json_reuses_completion_during_rpm_or_concurrency_wait() {
    for rpm_limited in [true, false] {
        queued_reuse(false, rpm_limited).await;
    }
}

#[tokio::test]
async fn queued_sse_reuses_completion_during_rpm_or_concurrency_wait() {
    for rpm_limited in [true, false] {
        queued_reuse(true, rpm_limited).await;
    }
}

#[tokio::test]
async fn queued_reuse_preserves_credentials_cache_control_and_cancellation() {
    let upstream =
        GatedUpstreamFixture::start_json(vec![], vec![JSON_RESPONSE.as_bytes().to_vec()], false)
            .await;
    let mut cfg = config(upstream.address());
    cfg.concurrency = 1;
    cfg.quota.rpm = config::Limit::Known(1.try_into().unwrap());
    let gateway = spawn_gateway(cfg).await;
    let leader = send(&gateway, PATH, &request(false), &[]).await;
    let body = request(false).to_string();
    let mut sockets = Vec::new();
    for (credential, extra) in [
        ("synthetic-one", ""),
        ("synthetic-two", ""),
        ("synthetic-one", "Cache-Control: no-store\r\n"),
        ("synthetic-one", ""),
    ] {
        let raw = format!(
            "POST {PATH} HTTP/1.1\r\nHost: localhost\r\nContent-Type: application/json\r\nAccept: */*\r\nAuthorization: Bearer {credential}\r\n{extra}Content-Length: {}\r\n\r\n{body}",
            body.len()
        );
        sockets.push(open_raw(gateway.address(), raw.as_bytes()).await);
    }
    wait_for_queue(&gateway, 2).await;
    wait_for_cache_waiters(&gateway, 2).await;
    abort_socket(sockets.remove(0));
    wait_for_cache_waiters(&gateway, 1).await;
    wait_for_queue(&gateway, 2).await;
    upstream.release_first();
    upstream.release_final();
    upstream.release_eof();
    assert_eq!(leader.bytes().await.unwrap(), JSON_RESPONSE);
    let mut matched = sockets.pop().unwrap();
    read_until(&mut matched, JSON_RESPONSE.as_bytes()).await;
    wait_for_queue(&gateway, 2).await;
    let value = status(&gateway).await;
    assert_eq!(value["exact_cache"]["considered"], 5);
    assert_eq!(value["exact_cache"]["hits"], 1);
    assert_eq!(value["exact_cache"]["misses"], 3);
    assert_eq!(value["exact_cache"]["bypasses"]["request_cache_control"], 1);
    assert_eq!(value["admission"]["active"], 0);
    assert_eq!(value["admission"]["tpm_held"], "0");
    assert_eq!(upstream.attempts(), 1);
    for socket in sockets {
        abort_socket(socket);
    }
    abort_socket(matched);
    wait_for_queue(&gateway, 0).await;
    assert_eq!(server::testing::quota_snapshot(&gateway).starts, 1);
    gateway.shutdown().await.unwrap();
}

#[tokio::test]
async fn queued_cache_wait_keeps_the_server_deadline() {
    let upstream =
        GatedUpstreamFixture::start_json(vec![], vec![JSON_RESPONSE.as_bytes().to_vec()], false)
            .await;
    let mut cfg = config(upstream.address());
    cfg.concurrency = 1;
    cfg.quota.rpm = config::Limit::Known(1.try_into().unwrap());
    let gateway = server::testing::spawn_with_timeouts(
        cfg,
        RuntimeCredentials::new(b"synthetic-control", None).unwrap(),
        Duration::from_millis(300),
        Duration::from_secs(1),
    )
    .await
    .unwrap();
    let leader = send(&gateway, PATH, &request(false), &[]).await;
    let response = send(&gateway, PATH, &request(false), &[]).await;
    assert_eq!(response.status(), 504);
    assert!(
        response
            .text()
            .await
            .unwrap()
            .contains("gateway_queue_deadline")
    );
    assert!(leader.bytes().await.is_err());
    wait_for_queue(&gateway, 0).await;
    let value = status(&gateway).await;
    assert_eq!(value["exact_cache"]["considered"], 2);
    assert_eq!(value["exact_cache"]["hits"], 0);
    assert_eq!(value["exact_cache"]["misses"], 2);
    assert_eq!(value["exact_cache"]["stores"], 0);
    assert_eq!(value["admission"]["active"], 0);
    assert_eq!(value["admission"]["tpm_held"], "0");
    assert_eq!(upstream.attempts(), 1);
    gateway.shutdown().await.unwrap();
}

#[tokio::test]
async fn retry_count_only_reruns_reuse_json_and_sse_and_forward_original_metadata() {
    for (stream, media, output) in [
        (false, "application/json", JSON_RESPONSE.to_owned()),
        (
            true,
            "text/event-stream",
            [DELTA, STOP, USAGE, DONE].concat(),
        ),
    ] {
        let upstream = UpstreamFixture::start(upstream_response(200, media, "", &output)).await;
        let gateway = spawn_gateway(config(upstream.address())).await;
        for retry in [Some("0"), Some("1"), None] {
            let headers: Vec<_> = retry
                .map(|v| ("x-stainless-retry-count", v))
                .into_iter()
                .collect();
            assert_eq!(
                send(&gateway, PATH, &request(stream), &headers)
                    .await
                    .bytes()
                    .await
                    .unwrap(),
                output
            );
        }
        assert_eq!(
            upstream.attempts(),
            1,
            "retry metadata must not split cache keys"
        );
        assert_eq!(
            upstream.capture().await.header("x-stainless-retry-count"),
            Some(b"0".as_slice())
        );
        let value = status(&gateway).await;
        assert_eq!(value["exact_cache"]["considered"], 3);
        assert_eq!(value["exact_cache"]["hits"], 2);
        assert_eq!(value["exact_cache"]["misses"], 1);
        assert_eq!(value["admission"]["reservation"]["samples"], 1);
        assert_eq!(value["admission"]["reservation"]["observed_tokens"], "23");
        assert_not_stored(&value["exact_cache"], "incomplete_or_unsafe", 0);
        gateway.shutdown().await.unwrap();
    }
}

#[tokio::test]
async fn original_vary_retry_metadata_forbids_json_and_sse_storage() {
    for (stream, media, output) in [
        (false, "application/json", JSON_RESPONSE.to_owned()),
        (
            true,
            "text/event-stream",
            [DELTA, STOP, USAGE, DONE].concat(),
        ),
    ] {
        let upstream = UpstreamFixture::start(upstream_response(200, media,
            "Vary: Accept-Encoding\r\nVary: x-other, X-Stainless-Retry-Count\r\nConnection: vary\r\n", &output)).await;
        let gateway = spawn_gateway(config(upstream.address())).await;
        for _ in 0..2 {
            assert_eq!(
                send(
                    &gateway,
                    PATH,
                    &request(stream),
                    &[("x-stainless-retry-count", "1")]
                )
                .await
                .bytes()
                .await
                .unwrap(),
                output
            );
        }
        assert_eq!(
            upstream.attempts(),
            2,
            "original Vary must prevent reuse even after hop-by-hop scrubbing"
        );
        assert_eq!(status(&gateway).await["exact_cache"]["stores"], 0);
        assert_not_stored(&status(&gateway).await["exact_cache"], "unsafe_headers", 2);
        gateway.shutdown().await.unwrap();
    }
}

#[tokio::test]
async fn cache_policy_denominator_includes_bypasses_but_excludes_invalid_ingress() {
    let upstream = UpstreamFixture::start(upstream_response(
        200,
        "application/json",
        "",
        JSON_RESPONSE,
    ))
    .await;
    let mut cfg = config(upstream.address());
    cfg.quota.tpm = config::Limit::Unknown;
    cfg.roots[0].endpoints.push(config::Endpoint::CountTokens);
    let gateway = spawn_gateway(cfg).await;
    let mut tools = request(false);
    tools["tools"] = json!([]);
    let mut history = request(false);
    history["messages"] = json!(vec![json!({"role":"user","content":"1"}); 4]);
    let mut unsupported = request(false);
    unsupported["n"] = json!(2);
    let mut size = request(false);
    size["messages"][0]["content"] = json!("x".repeat(33 * 1024));
    for (path, body, headers) in [
        (PATH, request(false), vec![]),
        (PATH, request(false), vec![]),
        (
            PATH,
            request(false),
            vec![
                ("cache-control", "no-cache"),
                ("connection", "cache-control, close"),
            ],
        ),
        (PATH, size, vec![]),
        ("/r/one/v1/messages/count_tokens", request(false), vec![]),
        (PATH, tools, vec![]),
        (PATH, history, vec![]),
        (PATH, unsupported, vec![]),
    ] {
        assert_eq!(
            send(&gateway, path, &body, &headers)
                .await
                .bytes()
                .await
                .unwrap(),
            JSON_RESPONSE
        );
    }
    assert_ne!(send(&gateway, PATH, &json!({}), &[]).await.status(), 200);
    let value = status(&gateway).await;
    let cache = &value["exact_cache"];
    assert_eq!(cache["considered"], 8);
    assert_eq!(cache["hits"], 1);
    assert_eq!(cache["misses"], 1);
    for reason in [
        "request_cache_control",
        "size",
        "endpoint",
        "tools_state",
        "history",
        "unsupported_shape",
    ] {
        assert_eq!(cache["bypasses"][reason], 1, "{reason}");
    }
    assert_eq!(upstream.attempts(), 7);
    assert_not_stored(cache, "incomplete_or_unsafe", 0);
    gateway.shutdown().await.unwrap();
}

#[tokio::test]
async fn exact_json_hit_skips_admission_and_scrubs_stale_response_metadata() {
    let upstream = UpstreamFixture::start(upstream_response(200, "application/json", "X-Request-Id: old\r\nX-RateLimit-Remaining: 3\r\nRetry-After: 9\r\nDate: Sat, 01 Jan 2000 00:00:00 GMT\r\n", JSON_RESPONSE)).await;
    let gateway = spawn_gateway(config(upstream.address())).await;
    let response = send(&gateway, PATH, &request(false), &[]).await;
    assert_eq!(response.headers()["x-request-id"], "old");
    assert_eq!(response.bytes().await.unwrap(), JSON_RESPONSE);
    let before = status(&gateway).await;
    let quota_before = server::testing::quota_snapshot(&gateway);
    assert_eq!(before["usage_known"], 1);
    assert_eq!(quota_before.tpm_debited, 23);
    let hit = send(&gateway, PATH, &request(false), &[]).await;
    assert_eq!(hit.status(), 200);
    for name in ["x-request-id", "x-ratelimit-remaining", "retry-after"] {
        assert!(hit.headers().get(name).is_none());
    }
    assert_ne!(
        hit.headers().get("date").unwrap(),
        "Sat, 01 Jan 2000 00:00:00 GMT"
    );
    assert_eq!(hit.bytes().await.unwrap(), JSON_RESPONSE);
    let after = status(&gateway).await;
    let quota_after = server::testing::quota_snapshot(&gateway);
    assert_eq!(upstream.attempts(), 1);
    assert_eq!(after["exact_cache"]["hits"], 1);
    assert_eq!(after["exact_cache"]["stores"], 1);
    assert_eq!(quota_before.starts, quota_after.starts);
    assert_eq!(quota_before.cleanups, quota_after.cleanups);
    for name in [
        "upstream_attempts",
        "usage_known",
        "usage_unknown",
        "observed_input_tokens",
        "observed_output_tokens",
        "terminal_total",
    ] {
        assert!(!before[name].is_null(), "missing counter {name}");
        assert_eq!(before[name], after[name], "{name}");
    }
    gateway.shutdown().await.unwrap();
}

#[tokio::test]
async fn keys_isolate_roots_credentials_query_options_and_original_body() {
    let upstream = UpstreamFixture::start(upstream_response(
        200,
        "application/json",
        "",
        JSON_RESPONSE,
    ))
    .await;
    let gateway = spawn_gateway(config(upstream.address())).await;
    let mut other_option = request(false);
    other_option["temperature"] = json!(0.7);
    let mut other_body = request(false);
    other_body["messages"][0]["content"] = json!("different classifier");
    let cases = [
        (PATH, request(false), vec![]),
        ("/r/two/v1/chat/completions", request(false), vec![]),
        (
            PATH,
            request(false),
            vec![("authorization", "Bearer synthetic-two")],
        ),
        (
            "/r/one/v1/chat/completions?variant=two",
            request(false),
            vec![],
        ),
        (PATH, other_option, vec![]),
        (PATH, other_body, vec![]),
        (
            PATH,
            request(false),
            vec![("x-provider-option", "different")],
        ),
    ];
    for (index, (path, body, headers)) in cases.iter().enumerate() {
        for _ in 0..2 {
            assert_eq!(
                send(&gateway, path, body, headers)
                    .await
                    .bytes()
                    .await
                    .unwrap(),
                JSON_RESPONSE
            );
            assert_eq!(upstream.attempts(), index + 1);
        }
    }
    assert_eq!(status(&gateway).await["exact_cache"]["hits"], cases.len());
    gateway.shutdown().await.unwrap();
}

#[tokio::test]
async fn cache_control_is_checked_before_connection_scrubbing_and_errors_never_cache() {
    let upstream = UpstreamFixture::start(upstream_response(
        200,
        "application/json",
        "",
        JSON_RESPONSE,
    ))
    .await;
    let gateway = spawn_gateway(config(upstream.address())).await;
    for _ in 0..2 {
        send(
            &gateway,
            PATH,
            &request(false),
            &[
                ("cache-control", "no-cache"),
                ("connection", "cache-control, close"),
            ],
        )
        .await
        .bytes()
        .await
        .unwrap();
    }
    assert_eq!(upstream.attempts(), 2);
    assert_eq!(status(&gateway).await["exact_cache"]["stores"], 0);
    gateway.shutdown().await.unwrap();
    for (code, header) in [
        (200, "Cache-Control: private, no-store\r\n"),
        (200, "Cache-Control: no-cache=\"Authorization\"\r\n"),
        (200, "Vary: *\r\n"),
        (200, "Set-Cookie: synthetic=yes\r\n"),
        (200, "Content-Encoding: gzip\r\n"),
        (500, ""),
    ] {
        let upstream = UpstreamFixture::start(upstream_response(
            code,
            "application/json",
            header,
            JSON_RESPONSE,
        ))
        .await;
        let gateway = spawn_gateway(config(upstream.address())).await;
        for _ in 0..2 {
            send(&gateway, PATH, &request(false), &[])
                .await
                .bytes()
                .await
                .unwrap();
        }
        assert_eq!(upstream.attempts(), 2, "{code} {header}");
        assert_eq!(status(&gateway).await["exact_cache"]["stores"], 0);
        gateway.shutdown().await.unwrap();
    }
}

#[tokio::test]
async fn short_text_eligibility_bypasses_history_tools_unknown_options_and_oversized_requests() {
    let upstream = UpstreamFixture::start(upstream_response(
        200,
        "application/json",
        "",
        JSON_RESPONSE,
    ))
    .await;
    let mut cfg = config(upstream.address());
    cfg.quota.tpm = config::Limit::Unknown;
    let gateway = spawn_gateway(cfg).await;
    let mut history = request(false);
    history["messages"] = json!([{"role":"user","content":"1"},{"role":"assistant","content":"2"},{"role":"user","content":"3"},{"role":"assistant","content":"4"}]);
    let mut tools = request(false);
    tools["tools"] = json!([]);
    let mut unknown = request(false);
    unknown["vendor_option"] = json!(true);
    let mut multi = request(false);
    multi["n"] = json!(2);
    let mut oversized = request(false);
    oversized["messages"][0]["content"] = json!("x".repeat(33 * 1024));
    let mut tool_history = request(false);
    tool_history["messages"][0]["role"] = json!("tool");
    for (index, body) in [history, tools, unknown, multi, oversized, tool_history]
        .iter()
        .enumerate()
    {
        for _ in 0..2 {
            assert_eq!(send(&gateway, PATH, body, &[]).await.status(), 200);
        }
        assert_eq!(upstream.attempts(), (index + 1) * 2);
    }
    assert_eq!(status(&gateway).await["exact_cache"]["stores"], 0);
    gateway.shutdown().await.unwrap();
}

#[tokio::test]
async fn normal_sse_hit_replays_bytes_but_does_not_double_settle_usage() {
    let sse = [DELTA, STOP, USAGE, DONE].concat();
    let upstream =
        UpstreamFixture::start(upstream_response(200, "text/event-stream", "", &sse)).await;
    let gateway = spawn_gateway(config(upstream.address())).await;
    assert_eq!(
        send(&gateway, PATH, &request(true), &[])
            .await
            .bytes()
            .await
            .unwrap(),
        sse
    );
    let before = status(&gateway).await;
    for _ in 0..2 {
        assert_eq!(
            send(&gateway, PATH, &request(true), &[])
                .await
                .bytes()
                .await
                .unwrap(),
            sse
        );
    }
    let after = status(&gateway).await;
    assert_eq!(upstream.attempts(), 1);
    assert_eq!(after["exact_cache"]["hits"], 2);
    assert_eq!(server::testing::quota_snapshot(&gateway).starts, 1);
    for name in [
        "usage_known",
        "usage_unknown",
        "observed_input_tokens",
        "observed_output_tokens",
        "upstream_attempts",
        "terminal_total",
        "response_body_eof",
        "first_body_byte",
        "terminal_marker",
    ] {
        assert!(!before[name].is_null(), "missing counter {name}");
        assert_eq!(before[name], after[name], "{name}");
    }
    gateway.shutdown().await.unwrap();
}

#[tokio::test]
async fn missing_usage_can_cache_complete_text_but_incomplete_or_tool_sse_cannot() {
    let normal = [DELTA, STOP, DONE].concat();
    let cases = [
        (normal.clone(), true),
        ([&DELTA.replace("\"refusal\":null", "\"refusal\":\"refused\""), STOP, DONE].concat(), false),
        ([&DELTA.replace("\"annotations\":[]", "\"annotations\":[{}]"), STOP, DONE].concat(), false),
        ([DELTA, DONE].concat(), false),
        ([DELTA, &STOP.replace("stop", "length"), DONE].concat(), false),
        ([DELTA, STOP, DONE, DELTA].concat(), false),
        ([DELTA, STOP, DONE, "data: {\"unfinished\":true}"].concat(), false),
        ([DELTA, STOP, DONE, "event: error\n"].concat(), false),
        ([DELTA, STOP, DONE, "data: {\"error\":\"synthetic\"}\n\n"].concat(), false),
        (["data: {\"choices\":[{\"delta\":{\"tool_calls\":[]},\"finish_reason\":\"tool_calls\"}]}\n\n", DONE].concat(), false),
        ([DELTA, STOP, DONE, &format!("data: {}\n\n", "x".repeat(270_000))].concat(), false),
    ];
    for (sse, should_cache) in cases {
        let upstream =
            UpstreamFixture::start(upstream_response(200, "text/event-stream", "", &sse)).await;
        let gateway = spawn_gateway(config(upstream.address())).await;
        for _ in 0..2 {
            assert_eq!(
                send(&gateway, PATH, &request(true), &[])
                    .await
                    .bytes()
                    .await
                    .unwrap(),
                sse
            );
        }
        assert_eq!(upstream.attempts(), if should_cache { 1 } else { 2 });
        assert_eq!(
            status(&gateway).await["exact_cache"]["stores"],
            usize::from(should_cache)
        );
        gateway.shutdown().await.unwrap();
    }
}

#[tokio::test]
async fn first_miss_streams_before_terminal_and_eof_and_only_then_becomes_a_hit() {
    let upstream = GatedUpstreamFixture::start(
        vec![DELTA.as_bytes().to_vec()],
        vec![[STOP, USAGE, DONE].concat().into_bytes()],
    )
    .await;
    let gateway = spawn_gateway(config(upstream.address())).await;
    let response = send(&gateway, PATH, &request(true), &[]).await;
    upstream.release_first();
    let mut response = response;
    let first = tokio::time::timeout(Duration::from_secs(1), response.chunk())
        .await
        .unwrap()
        .unwrap()
        .unwrap();
    assert!(String::from_utf8_lossy(&first).contains("cached answer"));
    assert_eq!(status(&gateway).await["exact_cache"]["stores"], 0);
    upstream.release_final();
    upstream.wait_for_final().await;
    assert_eq!(status(&gateway).await["exact_cache"]["stores"], 0);
    upstream.release_eof();
    response.bytes().await.unwrap();
    assert_eq!(
        send(&gateway, PATH, &request(true), &[])
            .await
            .bytes()
            .await
            .unwrap(),
        [DELTA, STOP, USAGE, DONE].concat()
    );
    assert_eq!(upstream.attempts(), 1);
    gateway.shutdown().await.unwrap();
}

#[tokio::test]
async fn abandoned_downstream_drains_without_committing_and_body_errors_never_cache() {
    let upstream = GatedUpstreamFixture::start(
        vec![DELTA.as_bytes().to_vec()],
        vec![[STOP, USAGE, DONE].concat().into_bytes()],
    )
    .await;
    let mut cfg = config(upstream.address());
    cfg.concurrency = 3;
    let gateway = spawn_gateway(cfg).await;
    let body = request(true).to_string();
    let raw = format!(
        "POST {PATH} HTTP/1.1\r\nHost: localhost\r\nContent-Type: application/json\r\nAccept: */*\r\nAuthorization: Bearer synthetic-one\r\nContent-Length: {}\r\n\r\n{body}",
        body.len()
    );
    let mut socket = open_raw(gateway.address(), raw.as_bytes()).await;
    upstream.release_first();
    read_until(&mut socket, b"cached answer").await;
    abort_socket(socket);
    tokio::time::timeout(Duration::from_secs(2), async {
        loop {
            if status(&gateway).await["draining"] == 1 {
                break;
            }
            tokio::time::sleep(Duration::from_millis(2)).await;
        }
    })
    .await
    .unwrap();
    assert_eq!(status(&gateway).await["exact_cache"]["inflight_keys"], 0);
    assert_not_stored(
        &status(&gateway).await["exact_cache"],
        "incomplete_or_unsafe",
        1,
    );
    let replacement = send(&gateway, PATH, &request(true), &[]).await;
    assert_eq!(
        upstream.attempts(),
        2,
        "draining owner must release its exact key before EOF"
    );
    upstream.release_final();
    upstream.release_eof();
    replacement.bytes().await.unwrap();
    tokio::time::timeout(Duration::from_secs(2), async {
        while server::testing::quota_snapshot(&gateway).active != 0 {
            tokio::time::sleep(Duration::from_millis(2)).await;
        }
    })
    .await
    .unwrap();
    assert_eq!(
        status(&gateway).await["exact_cache"]["stores"],
        1,
        "only the replacement commits"
    );
    assert_eq!(upstream.attempts(), 2);
    gateway.shutdown().await.unwrap();

    let upstream = GatedUpstreamFixture::start_with_body_error(
        vec![DELTA.as_bytes().to_vec()],
        vec![[STOP, DONE].concat().into_bytes()],
    )
    .await;
    let gateway = spawn_gateway(config(upstream.address())).await;
    upstream.release_first();
    upstream.release_final();
    upstream.release_eof();
    for _ in 0..2 {
        assert!(
            send(&gateway, PATH, &request(true), &[])
                .await
                .bytes()
                .await
                .is_err()
        );
    }
    assert_eq!(upstream.attempts(), 2);
    assert_eq!(status(&gateway).await["exact_cache"]["stores"], 0);
    assert_not_stored(
        &status(&gateway).await["exact_cache"],
        "incomplete_or_unsafe",
        2,
    );
    gateway.shutdown().await.unwrap();
}

#[tokio::test]
async fn cancelled_owner_before_headers_releases_follower_while_draining() {
    let upstream = GatedUpstreamFixture::start_before_headers(
        vec![DELTA.as_bytes().to_vec()],
        vec![[STOP, USAGE, DONE].concat().into_bytes()],
    )
    .await;
    let mut cfg = config(upstream.address());
    cfg.concurrency = 3;
    let gateway = spawn_gateway(cfg).await;
    let body = request(true).to_string();
    let raw = format!(
        "POST {PATH} HTTP/1.1\r\nHost: localhost\r\nContent-Type: application/json\r\nAccept: */*\r\nAuthorization: Bearer synthetic-one\r\nContent-Length: {}\r\n\r\n{body}",
        body.len()
    );
    let owner = open_raw(gateway.address(), raw.as_bytes()).await;
    tokio::time::timeout(Duration::from_secs(2), async {
        while upstream.attempts() != 1 {
            tokio::time::sleep(Duration::from_millis(2)).await;
        }
    })
    .await
    .unwrap();
    let mut follower = open_raw(gateway.address(), raw.as_bytes()).await;
    wait_for_cache_waiters(&gateway, 1).await;
    abort_socket(owner);
    tokio::time::timeout(Duration::from_secs(2), async {
        loop {
            let value = status(&gateway).await;
            if value["draining"] == 1 && upstream.attempts() == 2 {
                break;
            }
            tokio::time::sleep(Duration::from_millis(2)).await;
        }
    })
    .await
    .expect("the follower starts while its abandoned owner still drains before headers");
    assert_eq!(status(&gateway).await["exact_cache"]["waiters"], 0);
    upstream.release_first();
    upstream.release_final();
    upstream.release_eof();
    read_until(&mut follower, DONE.as_bytes()).await;
    tokio::time::timeout(Duration::from_secs(2), async {
        while server::testing::quota_snapshot(&gateway).active != 0 {
            tokio::time::sleep(Duration::from_millis(2)).await;
        }
    })
    .await
    .unwrap();
    let value = status(&gateway).await;
    assert_eq!(
        value["exact_cache"]["stores"], 1,
        "only the live follower stores its response"
    );
    assert_eq!(value["exact_cache"]["inflight_keys"], 0);
    assert_eq!(server::testing::quota_snapshot(&gateway).starts, 2);
    abort_socket(follower);
    gateway.shutdown().await.unwrap();
}

#[tokio::test]
async fn failed_owner_releases_follower_for_its_own_attempt_without_sharing_errors() {
    for stream in [false, true] {
        let upstream = if stream {
            GatedUpstreamFixture::start_with_body_error(
                vec![DELTA.as_bytes().to_vec()],
                vec![[STOP, DONE].concat().into_bytes()],
            )
            .await
        } else {
            GatedUpstreamFixture::start_json(vec![], vec![JSON_RESPONSE.as_bytes().to_vec()], true)
                .await
        };
        let mut cfg = config(upstream.address());
        cfg.concurrency = 3;
        let gateway = spawn_gateway(cfg).await;
        let leader = send(&gateway, PATH, &request(stream), &[]).await;
        let address = gateway.address();
        let follower = tokio::spawn(async move {
            let response = client()
                .post(format!("http://{address}{PATH}"))
                .header("content-type", "application/json")
                .bearer_auth("synthetic-one")
                .body(request(stream).to_string())
                .send()
                .await
                .unwrap();
            assert_eq!(response.status(), 200);
            assert!(response.bytes().await.is_err());
        });
        wait_for_cache_waiters(&gateway, 1).await;
        assert_eq!(upstream.attempts(), 1);
        upstream.release_first();
        upstream.release_final();
        upstream.release_eof();
        assert!(leader.bytes().await.is_err());
        tokio::time::timeout(Duration::from_secs(2), follower)
            .await
            .unwrap()
            .unwrap();
        let value = status(&gateway).await;
        assert_eq!(upstream.attempts(), 2);
        assert_eq!(value["exact_cache"]["stores"], 0);
        assert_eq!(value["exact_cache"]["hits"], 0);
        assert_eq!(value["exact_cache"]["waiters"], 0);
        assert_eq!(value["exact_cache"]["inflight_keys"], 0);
        assert_eq!(server::testing::quota_snapshot(&gateway).starts, 2);
        gateway.shutdown().await.unwrap();
    }
}

#[tokio::test]
async fn expired_entries_miss_and_disabled_cache_does_not_capture() {
    let upstream = UpstreamFixture::start(upstream_response(
        200,
        "application/json",
        "",
        JSON_RESPONSE,
    ))
    .await;
    let mut cfg = config(upstream.address());
    cfg.cache = Some(CacheConfig {
        ttl_secs: 1,
        max_history: 3,
    });
    let gateway = spawn_gateway(cfg).await;
    send(&gateway, PATH, &request(false), &[])
        .await
        .bytes()
        .await
        .unwrap();
    tokio::time::sleep(Duration::from_millis(1050)).await;
    send(&gateway, PATH, &request(false), &[])
        .await
        .bytes()
        .await
        .unwrap();
    assert_eq!(upstream.attempts(), 2);
    gateway.shutdown().await.unwrap();
    let mut cfg = config(upstream.address());
    cfg.cache = None;
    let gateway = spawn_gateway(cfg).await;
    for _ in 0..2 {
        send(&gateway, PATH, &request(false), &[])
            .await
            .bytes()
            .await
            .unwrap();
    }
    assert_eq!(upstream.attempts(), 4);
    let status = status(&gateway).await;
    assert_eq!(status["exact_cache"]["enabled"], false);
    assert_eq!(status["exact_cache"]["considered"], 0);
    assert_eq!(status["exact_cache"]["hits"], 0);
    assert_eq!(status["exact_cache"]["misses"], 0);
    assert!(
        status["exact_cache"]["bypasses"]
            .as_object()
            .unwrap()
            .values()
            .all(|v| v == 0)
    );
    assert_eq!(status["exact_cache"]["retained_bytes"], 0);
    assert_not_stored(&status["exact_cache"], "incomplete_or_unsafe", 0);
    gateway.shutdown().await.unwrap();
}

#[tokio::test]
async fn messages_and_responses_require_complete_text_and_stateless_requests() {
    let messages = json!({"type":"message","role":"assistant","content":[{"type":"text","text":"answer"}],"stop_reason":"end_turn"}).to_string();
    let response = json!({"status":"completed","error":null,"output":[{"type":"message","role":"assistant","status":"completed","content":[{"type":"output_text","text":"answer","annotations":[]}]}]}).to_string();
    for (endpoint, output, body) in [
        (
            "messages",
            messages,
            json!({"model":"synthetic","messages":[{"role":"user","content":"classify"}],"max_tokens":64}),
        ),
        (
            "responses",
            response,
            json!({"model":"synthetic","input":"classify","store":false,"max_output_tokens":64}),
        ),
    ] {
        let upstream =
            UpstreamFixture::start(upstream_response(200, "application/json", "", &output)).await;
        let gateway = spawn_gateway(config(upstream.address())).await;
        let path = format!("/r/one/v1/{endpoint}");
        for _ in 0..2 {
            assert_eq!(
                send(&gateway, &path, &body, &[])
                    .await
                    .bytes()
                    .await
                    .unwrap(),
                output
            );
        }
        assert_eq!(upstream.attempts(), 1);
        if endpoint == "responses" {
            let mut stored = body;
            stored.as_object_mut().unwrap().remove("store");
            for _ in 0..2 {
                send(&gateway, &path, &stored, &[])
                    .await
                    .bytes()
                    .await
                    .unwrap();
            }
            assert_eq!(upstream.attempts(), 3);
        }
        gateway.shutdown().await.unwrap();
    }
}

#[test]
fn typed_server_rejects_out_of_range_cache_settings() {
    let cfg = config("127.0.0.1:9".parse().unwrap());
    assert_eq!(cfg.cache, Some(CacheConfig::default()));
    for cache in [
        CacheConfig {
            ttl_secs: 0,
            max_history: 3,
        },
        CacheConfig {
            ttl_secs: 3601,
            max_history: 3,
        },
        CacheConfig {
            ttl_secs: 300,
            max_history: 0,
        },
        CacheConfig {
            ttl_secs: 300,
            max_history: 65,
        },
    ] {
        let mut invalid = cfg.clone();
        invalid.cache = Some(cache);
        assert!(server::validate_start_config(&invalid).is_err());
    }
}

#[tokio::test]
async fn message_stream_lifecycle_and_response_terminal_output_are_validated() {
    let start = "data: {\"type\":\"message_start\",\"message\":{\"role\":\"assistant\",\"content\":[],\"usage\":{\"input_tokens\":20,\"output_tokens\":0}}}\n\n";
    let block = "data: {\"type\":\"content_block_start\",\"index\":0,\"content_block\":{\"type\":\"text\",\"text\":\"\"}}\n\n";
    let delta = "data: {\"type\":\"content_block_delta\",\"index\":0,\"delta\":{\"type\":\"text_delta\",\"text\":\"answer\"}}\n\n";
    let block_stop = "data: {\"type\":\"content_block_stop\",\"index\":0}\n\n";
    let final_delta = "data: {\"type\":\"message_delta\",\"delta\":{\"stop_reason\":\"end_turn\"},\"usage\":{\"output_tokens\":3}}\n\n";
    let stop = "data: {\"type\":\"message_stop\"}\n\n";
    let message_body = json!({"model":"synthetic","messages":[{"role":"user","content":"classify"}],"max_tokens":64,"stream":true});
    let complete = json!({"type":"response.completed","response":{"status":"completed","error":null,"output":[{"type":"message","role":"assistant","status":"completed","content":[{"type":"output_text","text":"answer","annotations":[]}]}],"usage":{"input_tokens":20,"output_tokens":3}}});
    let response_sse = format!("data: {complete}\n\n");
    let mut tool = complete;
    tool["response"]["output"][0] = json!({"type":"function_call","name":"tool","arguments":"{}"});
    let response_body = json!({"model":"synthetic","input":"classify","store":false,"stream":true,"max_output_tokens":64});
    for (endpoint, body, sse, cached) in [
        (
            "messages",
            &message_body,
            [start, block, delta, block_stop, final_delta, stop].concat(),
            true,
        ),
        (
            "messages",
            &message_body,
            [delta, final_delta, stop].concat(),
            false,
        ),
        (
            "messages",
            &message_body,
            [start, block, delta, final_delta, stop].concat(),
            false,
        ),
        (
            "messages",
            &message_body,
            [start, start, block, delta, block_stop, final_delta, stop].concat(),
            false,
        ),
        (
            "messages",
            &message_body,
            [
                start,
                block,
                &delta.replace("\"index\":0", "\"index\":1"),
                block_stop,
                final_delta,
                stop,
            ]
            .concat(),
            false,
        ),
        (
            "messages",
            &message_body,
            [
                start,
                block,
                delta,
                block_stop,
                &final_delta.replace("end_turn", "max_tokens"),
                stop,
            ]
            .concat(),
            false,
        ),
        (
            "messages",
            &message_body,
            [start, block, delta, block_stop, final_delta, stop, delta].concat(),
            false,
        ),
        ("responses", &response_body, response_sse.clone(), true),
        (
            "responses",
            &response_body,
            format!("data: {tool}\n\n"),
            false,
        ),
        (
            "responses",
            &response_body,
            format!(
                "{response_sse}data: {{\"type\":\"response.output_text.delta\",\"delta\":\"late\"}}\n\n"
            ),
            false,
        ),
    ] {
        let upstream =
            UpstreamFixture::start(upstream_response(200, "text/event-stream", "", &sse)).await;
        let gateway = spawn_gateway(config(upstream.address())).await;
        let path = format!("/r/one/v1/{endpoint}");
        for _ in 0..2 {
            assert_eq!(
                send(&gateway, &path, body, &[])
                    .await
                    .bytes()
                    .await
                    .unwrap(),
                sse
            );
        }
        assert_eq!(
            upstream.attempts(),
            if cached { 1 } else { 2 },
            "{endpoint}"
        );
        gateway.shutdown().await.unwrap();
    }
}

#[tokio::test]
async fn oversized_chunked_capture_releases_its_budget_before_eof() {
    let oversized = format!(
        "data: {{\"choices\":[{{\"delta\":{{\"content\":\"{}\"}}}}]}}\n\n",
        "x".repeat(270_000)
    );
    let upstream = GatedUpstreamFixture::start(
        vec![oversized.clone().into_bytes()],
        vec![[STOP, DONE].concat().into_bytes()],
    )
    .await;
    let mut cfg = config(upstream.address());
    cfg.concurrency = 3;
    let gateway = spawn_gateway(cfg).await;
    upstream.release_first();
    let mut response = send(&gateway, PATH, &request(true), &[]).await;
    let mut received = 0;
    while received < oversized.len() {
        received += response.chunk().await.unwrap().unwrap().len();
    }
    let value = status(&gateway).await;
    assert_eq!(value["exact_cache"]["retained_bytes"], 0);
    assert_eq!(value["exact_cache"]["inflight_keys"], 0);
    assert_not_stored(&value["exact_cache"], "size", 1);
    let replacement = send(&gateway, PATH, &request(true), &[]).await;
    assert_eq!(
        upstream.attempts(),
        2,
        "overflow releases ownership before EOF"
    );
    upstream.release_final();
    upstream.release_eof();
    response.bytes().await.unwrap();
    replacement.bytes().await.unwrap();
    assert_eq!(upstream.attempts(), 2);
    assert_eq!(status(&gateway).await["exact_cache"]["stores"], 0);
    assert_not_stored(&status(&gateway).await["exact_cache"], "size", 2);
    gateway.shutdown().await.unwrap();
}

#[tokio::test]
async fn incomplete_tool_or_refusal_json_is_forwarded_but_never_stored() {
    for output in [
        JSON_RESPONSE.replace("\"stop\"", "\"length\""),
        JSON_RESPONSE.replace("\"refusal\":null", "\"refusal\":\"refused\""),
        JSON_RESPONSE.replace("\"annotations\":[]", "\"tool_calls\":[]"),
        JSON_RESPONSE[..JSON_RESPONSE.len() - 1].to_owned(),
    ] {
        let upstream =
            UpstreamFixture::start(upstream_response(200, "application/json", "", &output)).await;
        let gateway = spawn_gateway(config(upstream.address())).await;
        for _ in 0..2 {
            assert_eq!(
                send(&gateway, PATH, &request(false), &[])
                    .await
                    .bytes()
                    .await
                    .unwrap(),
                output
            );
        }
        assert_eq!(upstream.attempts(), 2);
        gateway.shutdown().await.unwrap();
    }
}

#[tokio::test]
async fn responses_incomplete_usage_settles_without_json_or_sse_cache_reuse() {
    for reason in ["max_output_tokens", "content_filter"] {
        let response = json!({"status":"incomplete","error":null,"incomplete_details":{"reason":reason},"output":[{"type":"message","role":"assistant","status":"completed","content":[{"type":"output_text","text":"partial answer","annotations":[]}]}],"usage":{"input_tokens":20,"output_tokens":3,"input_tokens_details":{"cached_tokens":7}}});
        for stream in [false, true] {
            let (media, output) = if stream {
                (
                    "text/event-stream",
                    format!(
                        "data: {}\n\n",
                        json!({"type":"response.incomplete","response":response})
                    ),
                )
            } else {
                ("application/json", response.to_string())
            };
            let body = json!({"model":"synthetic","input":"classify","store":false,"stream":stream,"max_output_tokens":64});
            let upstream = UpstreamFixture::start(upstream_response(200, media, "", &output)).await;
            let gateway = spawn_gateway(config(upstream.address())).await;
            for _ in 0..2 {
                let result = send(&gateway, "/r/one/v1/responses", &body, &[]).await;
                assert_eq!(result.status(), 200);
                assert_eq!(result.bytes().await.unwrap(), output);
            }
            assert_eq!(upstream.attempts(), 2);
            assert_eq!(server::testing::quota_snapshot(&gateway).tpm_debited, 46);
            let metrics = status(&gateway).await;
            assert_eq!(metrics["usage_known"], 2);
            assert_eq!(metrics["exact_cache"]["misses"], 2);
            assert_eq!(metrics["exact_cache"]["stores"], 0);
            assert_eq!(metrics["exact_cache"]["hits"], 0);
            assert_not_stored(&metrics["exact_cache"], "incomplete_or_unsafe", 2);
            gateway.shutdown().await.unwrap();
        }
    }
}

#[tokio::test]
async fn a_hit_returns_while_the_single_request_rpm_window_is_exhausted() {
    let upstream = UpstreamFixture::start(upstream_response(
        200,
        "application/json",
        "",
        JSON_RESPONSE,
    ))
    .await;
    let mut cfg = config(upstream.address());
    cfg.quota.rpm = config::Limit::Known(1.try_into().unwrap());
    let gateway = spawn_gateway(cfg).await;
    for _ in 0..2 {
        assert_eq!(
            send(&gateway, PATH, &request(false), &[])
                .await
                .bytes()
                .await
                .unwrap(),
            JSON_RESPONSE
        );
    }
    assert_eq!(upstream.attempts(), 1);
    assert_eq!(server::testing::quota_snapshot(&gateway).starts, 1);
    assert_eq!(status(&gateway).await["exact_cache"]["hits"], 1);
    gateway.shutdown().await.unwrap();
}

#[tokio::test]
async fn responses_reject_malformed_intermediate_payloads_before_valid_completion() {
    let final_response = json!({"status":"completed","error":null,"output":[{"type":"message","role":"assistant","status":"completed","content":[{"type":"output_text","text":"answer","annotations":[]}]}],"usage":{"input_tokens":20,"output_tokens":3}});
    let body = json!({"model":"synthetic","input":"classify","store":false,"stream":true,"max_output_tokens":64});
    let path = "/r/one/v1/responses";
    for (event, cached) in [
        (
            json!({"type":"response.output_text.delta","delta":42}),
            false,
        ),
        (json!({"type":"response.output_text.delta"}), false),
        (json!({"type":"response.output_text.done"}), false),
        (json!({"type":"response.output_text.done","text":42}), false),
        (
            json!({"type":"response.content_part.added","part":{"type":"output_text","text":42}}),
            false,
        ),
        (
            json!({"type":"response.content_part.done","part":{"type":"output_text","text":"answer","annotations":[{"type":"file_citation"}]}}),
            false,
        ),
        (
            json!({"type":"response.content_part.done","part":{"type":"output_text","text":"answer","logprobs":42}}),
            false,
        ),
        (
            json!({"type":"response.output_item.added","item":{"type":"message","role":"assistant","status":"in_progress","content":[{"type":"function_call"}]}}),
            false,
        ),
        (
            json!({"type":"response.output_item.done","item":{"type":"message","role":"assistant","status":"completed","content":[{"type":"output_text","text":42}]}}),
            false,
        ),
        (json!({"type":"response.created"}), false),
        (json!({"type":"response.created","response":42}), false),
        (
            json!({"type":"response.in_progress","response":null}),
            false,
        ),
        (
            json!({"type":"response.in_progress","response":{"status":"in_progress","output":[{"type":"function_call"}]}}),
            false,
        ),
        (
            json!({"type":"response.output_text.delta","delta":"answer"}),
            true,
        ),
        (
            json!({"type":"response.output_text.done","text":"answer"}),
            true,
        ),
        (
            json!({"type":"response.created","response":{"status":"in_progress","error":null,"output":[]}}),
            true,
        ),
        (
            json!({"type":"response.output_item.added","item":{"type":"message","role":"assistant","status":"in_progress","content":[]}}),
            true,
        ),
        (
            json!({"type":"response.content_part.added","part":{"type":"output_text","text":"","annotations":[],"logprobs":[]}}),
            true,
        ),
        (
            json!({"type":"response.output_item.done","item":final_response["output"][0]}),
            true,
        ),
    ] {
        let completion = json!({"type":"response.completed","response":final_response});
        let sse = format!("data: {event}\n\ndata: {completion}\n\n");
        let upstream =
            UpstreamFixture::start(upstream_response(200, "text/event-stream", "", &sse)).await;
        let gateway = spawn_gateway(config(upstream.address())).await;
        for _ in 0..2 {
            assert_eq!(
                send(&gateway, path, &body, &[])
                    .await
                    .bytes()
                    .await
                    .unwrap(),
                sse
            );
        }
        assert_eq!(
            upstream.attempts(),
            if cached { 1 } else { 2 },
            "event: {event}"
        );
        assert_eq!(
            status(&gateway).await["exact_cache"]["stores"],
            usize::from(cached),
            "event: {event}"
        );
        gateway.shutdown().await.unwrap();
    }
}
