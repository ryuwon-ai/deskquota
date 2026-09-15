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
    let gateway = spawn_gateway(config(upstream.address())).await;
    let body = request(true).to_string();
    let raw = format!(
        "POST {PATH} HTTP/1.1\r\nHost: localhost\r\nContent-Type: application/json\r\nAuthorization: Bearer synthetic-one\r\nContent-Length: {}\r\n\r\n{body}",
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
            tokio::task::yield_now().await;
        }
    })
    .await
    .unwrap();
    upstream.release_final();
    upstream.release_eof();
    tokio::time::timeout(Duration::from_secs(2), async {
        loop {
            if server::testing::quota_snapshot(&gateway).active == 0 {
                break;
            }
            tokio::task::yield_now().await;
        }
    })
    .await
    .unwrap();
    assert_eq!(status(&gateway).await["exact_cache"]["stores"], 0);
    send(&gateway, PATH, &request(true), &[])
        .await
        .bytes()
        .await
        .unwrap();
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
    gateway.shutdown().await.unwrap();
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
    assert_eq!(status["exact_cache"]["retained_bytes"], 0);
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
    let gateway = spawn_gateway(config(upstream.address())).await;
    upstream.release_first();
    let mut response = send(&gateway, PATH, &request(true), &[]).await;
    let mut received = 0;
    while received < oversized.len() {
        received += response.chunk().await.unwrap().unwrap().len();
    }
    assert_eq!(status(&gateway).await["exact_cache"]["retained_bytes"], 0);
    upstream.release_final();
    upstream.release_eof();
    response.bytes().await.unwrap();
    send(&gateway, PATH, &request(true), &[])
        .await
        .bytes()
        .await
        .unwrap();
    assert_eq!(upstream.attempts(), 2);
    assert_eq!(status(&gateway).await["exact_cache"]["stores"], 0);
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
