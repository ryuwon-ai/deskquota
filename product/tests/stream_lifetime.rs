mod support;

use std::time::Duration;

use llmgw::config::{
    Accounting, Auth, CancelPolicy, Config, Endpoint, Limit, Model, Quota, Root, Upstream,
};
use llmgw::server::{self, GatewayHandle, RuntimeCredentials};
use serde_json::Value;
use support::fixture::{
    GatedUpstreamFixture, abort_socket, open_raw, read_until, response_body, send_raw, status,
};
use tokio::io::{AsyncReadExt, AsyncWriteExt};

const DATA_TOKEN: &[u8] = b"synthetic-data-token";
const CONTROL_TOKEN: &[u8] = b"synthetic-control-token";
const CHAT_REQUEST: &[u8] = br#"{"model":"fixture-model","messages":[]}"#;
const FIRST_EVENT: &[u8] = b"data: {\"choices\":[{\"delta\":{\"content\":\"first\"}}]}\n\n";
const DONE_EVENT: &[u8] = b"data: [DONE]\n\n";

fn config(upstream: &GatedUpstreamFixture, policy: CancelPolicy) -> Config {
    Config {
        listen: "127.0.0.1:0".parse().expect("fixture listen"),
        concurrency: 1,
        startup_hold_secs: 60,
        cache: None,
        cancel_policy: policy,
        accounting: Accounting::Reserved,
        retry_transient_429: false,
        upstream: Upstream {
            api_base: format!("http://{}/team/v1", upstream.address())
                .parse()
                .expect("fixture URL"),
            auth: Auth::None,
            proxy: None,
            ca_bundle: None,
        },
        quota: Quota {
            rpm: Limit::Unlimited,
            tpm: Limit::Unknown,
        },
        models: vec![Model {
            input_estimator: Default::default(),
            input_token_overhead: 0,
            id: "fixture-model".to_owned(),
            max_output_tokens: None,
        }],
        roots: vec![Root {
            id: "pi-work".to_owned(),
            endpoints: vec![
                Endpoint::ChatCompletions,
                Endpoint::Responses,
                Endpoint::Messages,
                Endpoint::CountTokens,
                Endpoint::Models,
            ],
            models: vec!["fixture-model".to_owned()],
        }],
    }
}

async fn gateway(upstream: &GatedUpstreamFixture, policy: CancelPolicy) -> GatewayHandle {
    let credentials = RuntimeCredentials::new(CONTROL_TOKEN, None).expect("synthetic credentials");
    server::spawn(config(upstream, policy), credentials)
        .await
        .expect("start gateway")
}

async fn gateway_with_timeouts(
    upstream: &GatedUpstreamFixture,
    policy: CancelPolicy,
    request: Duration,
    shutdown: Duration,
) -> GatewayHandle {
    let credentials = RuntimeCredentials::new(CONTROL_TOKEN, None).expect("synthetic credentials");
    server::testing::spawn_with_timeouts(config(upstream, policy), credentials, request, shutdown)
        .await
        .expect("start gateway with fixture timeouts")
}

fn post(path: &str, body: &[u8]) -> Vec<u8> {
    post_with_connection(path, body, "close")
}

fn post_with_connection(path: &str, body: &[u8], connection: &str) -> Vec<u8> {
    let mut request = format!(
        "POST {path} HTTP/1.1\r\nHost: localhost\r\nContent-Type: application/json\r\nX-LLMGW-Token: {}\r\nContent-Length: {}\r\nConnection: {connection}\r\n\r\n",
        std::str::from_utf8(DATA_TOKEN).expect("fixture token"),
        body.len()
    )
    .into_bytes();
    request.extend_from_slice(body);
    request
}

fn get(path: &str) -> Vec<u8> {
    format!(
        "GET {path} HTTP/1.1\r\nHost: localhost\r\nX-LLMGW-Token: {}\r\nConnection: close\r\n\r\n",
        std::str::from_utf8(DATA_TOKEN).expect("fixture token")
    )
    .into_bytes()
}

async fn status_snapshot(gateway: &GatewayHandle) -> Value {
    let request = format!(
        "GET /_llmgw/status HTTP/1.1\r\nHost: localhost\r\nX-LLMGW-Control-Token: {}\r\nConnection: close\r\n\r\n",
        std::str::from_utf8(CONTROL_TOKEN).expect("fixture token")
    );
    let response = send_raw(gateway.address(), request.as_bytes()).await;
    assert_eq!(status(&response), 200);
    serde_json::from_slice(response_body(&response)).expect("status JSON")
}

async fn wait_for_metric(gateway: &GatewayHandle, name: &str, expected: u64) -> Value {
    tokio::time::timeout(Duration::from_secs(3), async {
        loop {
            let snapshot = status_snapshot(gateway).await;
            if snapshot[name].as_u64() == Some(expected) {
                return snapshot;
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    })
    .await
    .expect("metric reaches expected value")
}

async fn complete_stream(
    endpoint: &str,
    first_chunks: Vec<Vec<u8>>,
    final_chunks: Vec<Vec<u8>>,
) -> Value {
    let upstream = GatedUpstreamFixture::start(first_chunks, final_chunks).await;
    let gateway = gateway(&upstream, CancelPolicy::Drain).await;
    let request = post(&format!("/r/pi-work/v1/{endpoint}"), CHAT_REQUEST);
    let reader = tokio::spawn({
        let address = gateway.address();
        async move { send_raw(address, &request).await }
    });
    upstream.release_first();
    upstream.wait_for_first().await;
    upstream.release_final();
    upstream.wait_for_final().await;
    assert_eq!(
        upstream.attempts(),
        1,
        "completion marker does not release slot"
    );
    upstream.release_eof();
    let response = reader.await.expect("stream reader");
    assert_eq!(status(&response), 200);
    let snapshot = wait_for_metric(&gateway, "active", 0).await;
    gateway.shutdown().await.expect("shutdown gateway");
    snapshot
}

fn decode_chunked_response(response: &[u8]) -> Vec<u8> {
    let mut cursor = response
        .windows(4)
        .position(|window| window == b"\r\n\r\n")
        .expect("response header terminator")
        + 4;
    let mut body = Vec::new();
    loop {
        let line_end = response[cursor..]
            .windows(2)
            .position(|window| window == b"\r\n")
            .expect("chunk size terminator")
            + cursor;
        let size = usize::from_str_radix(
            std::str::from_utf8(&response[cursor..line_end]).expect("chunk size text"),
            16,
        )
        .expect("chunk size");
        cursor = line_end + 2;
        if size == 0 {
            return body;
        }
        body.extend_from_slice(&response[cursor..cursor + size]);
        cursor += size;
        assert_eq!(&response[cursor..cursor + 2], b"\r\n");
        cursor += 2;
    }
}

fn has_successful_chunked_eof(response: &[u8]) -> bool {
    response.ends_with(b"0\r\n\r\n")
}

#[tokio::test]
async fn first_event_arrives_before_eof() {
    let upstream =
        GatedUpstreamFixture::start(vec![FIRST_EVENT.to_vec()], vec![DONE_EVENT.to_vec()]).await;
    let gateway = gateway(&upstream, CancelPolicy::Drain).await;
    let mut downstream = open_raw(
        gateway.address(),
        &post("/r/pi-work/v1/chat/completions", CHAT_REQUEST),
    )
    .await;

    upstream.release_first();
    let prefix = read_until(&mut downstream, b"first").await;
    assert!(prefix.windows(5).any(|window| window == b"first"));
    assert!(!upstream.body_eof());

    upstream.release_final();
    upstream.release_eof();
    downstream
        .read_to_end(&mut Vec::new())
        .await
        .expect("response EOF");
    gateway.shutdown().await.expect("shutdown gateway");
}

#[tokio::test]
async fn split_utf8_and_sse_bytes_preserved() {
    let first = b"data: {\"choices\":[{\"delta\":{\"content\":\"\xed".to_vec();
    let final_bytes = b"\x95\x9c\"}}]}\n\ndata: [DONE]\n\n".to_vec();
    let expected = [first.clone(), final_bytes.clone()].concat();
    let upstream = GatedUpstreamFixture::start(vec![first], vec![final_bytes]).await;
    let gateway = gateway(&upstream, CancelPolicy::Drain).await;
    let mut downstream = open_raw(
        gateway.address(),
        &post("/r/pi-work/v1/chat/completions", CHAT_REQUEST),
    )
    .await;

    upstream.release_first();
    upstream.wait_for_first().await;
    assert!(!upstream.body_eof());
    upstream.release_final();
    upstream.wait_for_final().await;
    assert_eq!(upstream.attempts(), 1, "terminal marker is not body EOF");
    assert_eq!(status_snapshot(&gateway).await["terminal_marker"], 1);
    upstream.release_eof();
    let mut response = Vec::new();
    downstream
        .read_to_end(&mut response)
        .await
        .expect("read split response");

    assert_eq!(decode_chunked_response(&response), expected);
    gateway.shutdown().await.expect("shutdown gateway");
}

#[tokio::test]
async fn disconnect_drains_without_releasing_slot() {
    let upstream =
        GatedUpstreamFixture::start(vec![FIRST_EVENT.to_vec()], vec![DONE_EVENT.to_vec()]).await;
    let gateway = gateway(&upstream, CancelPolicy::Drain).await;
    let request = post("/r/pi-work/v1/chat/completions", CHAT_REQUEST);
    let mut first = open_raw(gateway.address(), &request).await;
    upstream.release_first();
    let _ = read_until(&mut first, b"first").await;
    abort_socket(first);

    let mut second = open_raw(gateway.address(), &request).await;
    tokio::time::sleep(Duration::from_millis(100)).await;
    assert_eq!(
        upstream.attempts(),
        1,
        "draining request owns the only slot"
    );
    let snapshot = status_snapshot(&gateway).await;
    assert_eq!(snapshot["active"], 1);
    assert_eq!(snapshot["draining"], 1);

    upstream.release_final();
    upstream.wait_for_final().await;
    assert_eq!(
        upstream.attempts(),
        1,
        "completion marker does not release slot"
    );
    assert_eq!(status_snapshot(&gateway).await["active"], 1);
    upstream.release_eof();
    upstream.wait_for_attempts(2).await;
    second
        .read_to_end(&mut Vec::new())
        .await
        .expect("second response EOF");
    let snapshot = wait_for_metric(&gateway, "active", 0).await;
    assert_eq!(snapshot["terminal_total"], 2);
    gateway.shutdown().await.expect("shutdown gateway");
}

#[tokio::test]
async fn close_policy_terminates_socket() {
    let upstream =
        GatedUpstreamFixture::start(vec![FIRST_EVENT.to_vec()], vec![DONE_EVENT.to_vec()]).await;
    let gateway = gateway(&upstream, CancelPolicy::Close).await;
    let mut downstream = open_raw(
        gateway.address(),
        &post("/r/pi-work/v1/chat/completions", CHAT_REQUEST),
    )
    .await;
    upstream.release_first();
    let _ = read_until(&mut downstream, b"first").await;
    abort_socket(downstream);

    upstream.wait_for_disconnect().await;
    let snapshot = wait_for_metric(&gateway, "active", 0).await;
    assert_eq!(snapshot["terminal_downstream_close"], 1);
    assert!(!upstream.body_eof());
    gateway.shutdown().await.expect("shutdown gateway");
}

#[tokio::test]
async fn huge_event_marks_usage_unknown() {
    let mut huge = b"data: {\"padding\":\"".to_vec();
    huge.extend(std::iter::repeat_n(b'x', 256 * 1024));
    huge.extend_from_slice(b"\",\"usage\":{\"prompt_tokens\":1,\"completion_tokens\":1}}\n\n");
    let expected = [huge.clone(), DONE_EVENT.to_vec()].concat();
    let upstream = GatedUpstreamFixture::start(vec![huge], vec![DONE_EVENT.to_vec()]).await;
    let gateway = gateway(&upstream, CancelPolicy::Drain).await;
    let request = post("/r/pi-work/v1/chat/completions", CHAT_REQUEST);
    let response_task = tokio::spawn({
        let address = gateway.address();
        async move { send_raw(address, &request).await }
    });

    upstream.release_first();
    upstream.wait_for_first().await;
    upstream.release_final();
    upstream.release_eof();
    let response = response_task.await.expect("response reader");
    assert_eq!(decode_chunked_response(&response), expected);
    let snapshot = wait_for_metric(&gateway, "active", 0).await;
    assert_eq!(snapshot["observer_overflow"], 1);
    assert_eq!(snapshot["usage_unknown"], 1);
    assert_eq!(snapshot["usage_known"], 0);
    gateway.shutdown().await.expect("shutdown gateway");
}

#[tokio::test]
async fn chat_empty_choices_final_usage_is_observed() {
    let snapshot = complete_stream(
        "chat/completions",
        vec![b"data: {\"choices\":[{\"delta\":{\"role\":\"assistant\"}}],\"usage\":null}\n\n".to_vec()],
        vec![
            b"data: {\"choices\":[],\"usage\":{\"prompt_tokens\":11,\"completion_tokens\":7,\"cached_tokens\":99}}\n\n".to_vec(),
            DONE_EVENT.to_vec(),
        ],
    )
    .await;
    assert_eq!(snapshot["usage_known"], 1);
    assert_eq!(snapshot["observed_input_tokens"], 11);
    assert_eq!(snapshot["observed_output_tokens"], 7);
    assert_eq!(snapshot["first_observed_output_delta"], 0);
}

#[tokio::test]
async fn chat_cached_usage_is_preserved_without_double_counting_input() {
    let usage = b"data: {\"choices\":[],\"usage\":{\"prompt_tokens\":20,\"completion_tokens\":3,\"prompt_tokens_details\":{\"cached_tokens\":7}}}\n\n".to_vec();
    let expected = [usage.clone(), DONE_EVENT.to_vec()].concat();
    let upstream = GatedUpstreamFixture::start(Vec::new(), vec![usage, DONE_EVENT.to_vec()]).await;
    let gateway = gateway(&upstream, CancelPolicy::Drain).await;
    let request = post("/r/pi-work/v1/chat/completions", CHAT_REQUEST);
    let response = tokio::spawn({
        let address = gateway.address();
        async move { send_raw(address, &request).await }
    });
    upstream.release_first();
    upstream.release_final();
    upstream.wait_for_final().await;
    upstream.release_eof();
    let response = response.await.expect("cached usage response");
    assert_eq!(decode_chunked_response(&response), expected);
    let snapshot = wait_for_metric(&gateway, "active", 0).await;
    assert_eq!(snapshot["usage_known"], 1);
    assert_eq!(snapshot["observed_input_tokens"], 20);
    assert_eq!(snapshot["observed_output_tokens"], 3);
    assert_eq!(snapshot["observed_cache_read_tokens"], 7);
    gateway.shutdown().await.expect("shutdown gateway");
}

#[tokio::test]
async fn chat_explicit_zero_cached_usage_is_known() {
    let snapshot = complete_stream(
        "chat/completions",
        Vec::new(),
        vec![
            b"data: {\"choices\":[],\"usage\":{\"prompt_tokens\":4,\"completion_tokens\":2,\"prompt_tokens_details\":{\"cached_tokens\":0}}}\n\n".to_vec(),
            DONE_EVENT.to_vec(),
        ],
    )
    .await;
    assert_eq!(snapshot["usage_known"], 1);
    assert_eq!(snapshot["observed_cache_read_tokens"], 0);
}

#[tokio::test]
async fn chat_malformed_cache_or_decreasing_usage_is_unknown() {
    for final_chunks in [
        vec![
            b"data: {\"choices\":[],\"usage\":{\"prompt_tokens\":20,\"completion_tokens\":3,\"prompt_tokens_details\":{\"cached_tokens\":-7}}}\n\n".to_vec(),
            DONE_EVENT.to_vec(),
        ],
        vec![
            b"data: {\"choices\":[],\"usage\":{\"prompt_tokens\":20,\"completion_tokens\":3,\"prompt_tokens_details\":\"malformed\"}}\n\n".to_vec(),
            DONE_EVENT.to_vec(),
        ],
        vec![
            b"data: {\"choices\":[],\"usage\":{\"prompt_tokens\":20,\"completion_tokens\":10}}\n\n".to_vec(),
            b"data: {\"choices\":[],\"usage\":{\"prompt_tokens\":20,\"completion_tokens\":2}}\n\n".to_vec(),
            DONE_EVENT.to_vec(),
        ],
    ] {
        let snapshot = complete_stream("chat/completions", Vec::new(), final_chunks).await;
        assert_eq!(snapshot["usage_known"], 0);
        assert_eq!(snapshot["usage_unknown"], 1);
    }
}

#[tokio::test]
async fn responses_completed_nested_usage_is_observed() {
    let snapshot = complete_stream(
        "responses",
        vec![b"data: {\"type\":\"response.output_text.delta\",\"delta\":\"hello\"}\n\n".to_vec()],
        vec![b"data: {\"type\":\"response.completed\",\"response\":{\"usage\":{\"input_tokens\":13,\"output_tokens\":5,\"input_tokens_details\":{\"cached_tokens\":4}}}}\n\n".to_vec()],
    )
    .await;
    assert_eq!(snapshot["usage_known"], 1);
    assert_eq!(snapshot["observed_input_tokens"], 13);
    assert_eq!(snapshot["observed_output_tokens"], 5);
    assert_eq!(snapshot["first_observed_output_delta"], 1);
    assert_eq!(snapshot["terminal_marker"], 1);
}

#[tokio::test]
async fn responses_malformed_cached_usage_is_unknown() {
    let snapshot = complete_stream(
        "responses",
        Vec::new(),
        vec![b"data: {\"type\":\"response.completed\",\"response\":{\"usage\":{\"input_tokens\":13,\"output_tokens\":5,\"input_tokens_details\":{\"cached_tokens\":-1}}}}\n\n".to_vec()],
    )
    .await;
    assert_eq!(snapshot["usage_known"], 0);
    assert_eq!(snapshot["usage_unknown"], 1);
}

#[tokio::test]
async fn anthropic_output_usage_is_cumulative_not_summed() {
    let snapshot = complete_stream(
        "messages",
        vec![b"event: message_start\ndata: {\"type\":\"message_start\",\"message\":{\"usage\":{\"input_tokens\":17,\"output_tokens\":1,\"cache_creation_input_tokens\":3,\"cache_read_input_tokens\":2}}}\n\n".to_vec()],
        vec![
            b"event: message_delta\ndata: {\"type\":\"message_delta\",\"usage\":{\"output_tokens\":4}}\n\nevent: message_delta\ndata: {\"type\":\"message_delta\",\"usage\":{\"output_tokens\":9}}\n\n".to_vec(),
            b"event: message_stop\ndata: {\"type\":\"message_stop\"}\n\n".to_vec(),
        ],
    )
    .await;
    assert_eq!(snapshot["usage_known"], 1);
    assert_eq!(snapshot["observed_input_tokens"], 17);
    assert_eq!(snapshot["observed_output_tokens"], 9);
    assert_eq!(snapshot["observed_cache_creation_tokens"], 3);
    assert_eq!(snapshot["observed_cache_read_tokens"], 2);
}

#[tokio::test]
async fn sse_bom_crlf_and_multiline_data_are_observed_without_rewriting() {
    let chunks = vec![
        b"\xef".to_vec(),
        b"\xbb\xbf: ping\r".to_vec(),
        b"\ndata: {\"choices\": [],\r".to_vec(),
        b"\ndata: \"usage\":{\"prompt_tokens\":19,\"completion_tokens\":6}}\r\n\r".to_vec(),
    ];
    let final_chunks = vec![b"\ndata: [DONE]\r\n\r\n".to_vec()];
    let expected = [chunks.concat(), final_chunks.concat()].concat();
    let upstream = GatedUpstreamFixture::start(chunks, final_chunks).await;
    let gateway = gateway(&upstream, CancelPolicy::Drain).await;
    let request = post("/r/pi-work/v1/chat/completions", CHAT_REQUEST);
    let response = tokio::spawn({
        let address = gateway.address();
        async move { send_raw(address, &request).await }
    });
    upstream.release_first();
    upstream.wait_for_first().await;
    upstream.release_final();
    upstream.release_eof();
    let response = response.await.expect("SSE response reader");
    assert_eq!(decode_chunked_response(&response), expected);
    let snapshot = wait_for_metric(&gateway, "active", 0).await;
    assert_eq!(snapshot["usage_known"], 1);
    assert_eq!(snapshot["observed_input_tokens"], 19);
    assert_eq!(snapshot["observed_output_tokens"], 6);
    gateway.shutdown().await.expect("shutdown gateway");
}

#[tokio::test]
async fn huge_comment_does_not_poison_later_usage() {
    let mut comment = b":".to_vec();
    comment.extend(std::iter::repeat_n(b'p', 300 * 1024));
    comment.extend_from_slice(b"\n\n");
    let usage =
        b"data: {\"choices\":[],\"usage\":{\"prompt_tokens\":3,\"completion_tokens\":4}}\n\n"
            .to_vec();
    let snapshot = complete_stream(
        "chat/completions",
        vec![comment],
        vec![usage, DONE_EVENT.to_vec()],
    )
    .await;
    assert_eq!(snapshot["observer_overflow"], 0);
    assert_eq!(snapshot["usage_known"], 1);
}

#[tokio::test]
async fn anthropic_provisional_output_without_final_delta_usage_is_unknown() {
    let snapshot = complete_stream(
        "messages",
        vec![b"event: message_start\ndata: {\"type\":\"message_start\",\"message\":{\"usage\":{\"input_tokens\":8,\"output_tokens\":1}}}\n\n".to_vec()],
        vec![b"event: message_stop\ndata: {\"type\":\"message_stop\"}\n\n".to_vec()],
    )
    .await;
    assert_eq!(snapshot["usage_known"], 0);
    assert_eq!(snapshot["usage_unknown"], 1);
}

#[tokio::test]
async fn anthropic_decreasing_cumulative_usage_is_unknown() {
    let snapshot = complete_stream(
        "messages",
        vec![b"event: message_start\ndata: {\"type\":\"message_start\",\"message\":{\"usage\":{\"input_tokens\":8,\"output_tokens\":1}}}\n\n".to_vec()],
        vec![
            b"event: message_delta\ndata: {\"type\":\"message_delta\",\"usage\":{\"output_tokens\":10}}\n\nevent: message_delta\ndata: {\"type\":\"message_delta\",\"usage\":{\"output_tokens\":2}}\n\n".to_vec(),
            b"event: message_stop\ndata: {\"type\":\"message_stop\"}\n\n".to_vec(),
        ],
    )
    .await;
    assert_eq!(snapshot["usage_known"], 0);
    assert_eq!(snapshot["usage_unknown"], 1);
}

#[tokio::test]
async fn incomplete_trailing_sse_event_is_not_dispatched() {
    let snapshot = complete_stream(
        "chat/completions",
        vec![b": ping\n\nevent: future_event\ndata: {\"ignored\":true}\n\n".to_vec()],
        vec![
            b"data: {\"choices\":[],\"usage\":{\"prompt_tokens\":2,\"completion_tokens\":3}}"
                .to_vec(),
        ],
    )
    .await;
    assert_eq!(snapshot["usage_known"], 0);
    assert_eq!(snapshot["usage_unknown"], 1);
    assert_eq!(snapshot["terminal_marker"], 0);
}

#[tokio::test]
async fn terminal_cleanup_happens_once() {
    let upstream =
        GatedUpstreamFixture::start(vec![FIRST_EVENT.to_vec()], vec![DONE_EVENT.to_vec()]).await;
    let gateway = gateway(&upstream, CancelPolicy::Drain).await;
    let mut downstream = open_raw(
        gateway.address(),
        &post("/r/pi-work/v1/chat/completions", CHAT_REQUEST),
    )
    .await;
    upstream.release_first();
    let _ = read_until(&mut downstream, b"first").await;

    upstream.release_final();
    upstream.release_eof();
    abort_socket(downstream);
    let snapshot = wait_for_metric(&gateway, "active", 0).await;
    assert_eq!(snapshot["terminal_total"], 1);
    let reasons = snapshot["terminal_body_eof"].as_u64().unwrap_or(0)
        + snapshot["terminal_upstream_error"].as_u64().unwrap_or(0)
        + snapshot["terminal_deadline"].as_u64().unwrap_or(0)
        + snapshot["terminal_downstream_close"].as_u64().unwrap_or(0)
        + snapshot["terminal_shutdown"].as_u64().unwrap_or(0);
    assert_eq!(reasons, 1);
    gateway.shutdown().await.expect("shutdown gateway");
}

#[tokio::test]
async fn slow_reader_has_a_bounded_application_queue() {
    let final_body = vec![b'x'; 4 * 1024 * 1024];
    let upstream = GatedUpstreamFixture::start(vec![FIRST_EVENT.to_vec()], vec![final_body]).await;
    let gateway = gateway(&upstream, CancelPolicy::Drain).await;
    let mut downstream = open_raw(
        gateway.address(),
        &post("/r/pi-work/v1/chat/completions", CHAT_REQUEST),
    )
    .await;
    upstream.release_first();
    let _ = read_until(&mut downstream, b"first").await;
    upstream.release_final();
    upstream.release_eof();
    tokio::time::sleep(Duration::from_millis(100)).await;

    let snapshot = status_snapshot(&gateway).await;
    assert_eq!(snapshot["active"], 1);
    assert!(
        snapshot["queued_response_bytes"]
            .as_u64()
            .unwrap_or(u64::MAX)
            <= 64 * 1024
    );
    assert!(
        snapshot["max_queued_response_bytes"]
            .as_u64()
            .unwrap_or(u64::MAX)
            <= 64 * 1024
    );

    abort_socket(downstream);
    let _ = wait_for_metric(&gateway, "active", 0).await;
    gateway.shutdown().await.expect("shutdown gateway");
}

#[tokio::test]
async fn queued_disconnect_never_starts_upstream() {
    let upstream =
        GatedUpstreamFixture::start(vec![FIRST_EVENT.to_vec()], vec![DONE_EVENT.to_vec()]).await;
    let gateway = gateway(&upstream, CancelPolicy::Drain).await;
    let request = post("/r/pi-work/v1/chat/completions", CHAT_REQUEST);
    let mut active = open_raw(gateway.address(), &request).await;
    upstream.release_first();
    let _ = read_until(&mut active, b"first").await;

    let queued = open_raw(gateway.address(), &request).await;
    abort_socket(queued);
    tokio::time::sleep(Duration::from_millis(100)).await;
    assert_eq!(upstream.attempts(), 1);
    upstream.release_final();
    upstream.release_eof();
    active
        .read_to_end(&mut Vec::new())
        .await
        .expect("active response EOF");
    tokio::time::sleep(Duration::from_millis(100)).await;
    assert_eq!(
        upstream.attempts(),
        1,
        "cancelled waiter never starts upstream"
    );
    gateway.shutdown().await.expect("shutdown gateway");
}

#[tokio::test]
async fn pre_header_disconnect_drains_and_holds_slot() {
    let upstream = GatedUpstreamFixture::start_before_headers(
        vec![FIRST_EVENT.to_vec()],
        vec![DONE_EVENT.to_vec()],
    )
    .await;
    let gateway = gateway(&upstream, CancelPolicy::Drain).await;
    let request = post("/r/pi-work/v1/chat/completions", CHAT_REQUEST);
    let first = open_raw(gateway.address(), &request).await;
    upstream.wait_for_attempts(1).await;
    abort_socket(first);

    let mut second = open_raw(gateway.address(), &request).await;
    tokio::time::sleep(Duration::from_millis(100)).await;
    assert_eq!(upstream.attempts(), 1, "pre-header worker owns capacity");
    let snapshot = status_snapshot(&gateway).await;
    assert_eq!(snapshot["active"], 1);
    assert_eq!(snapshot["draining"], 1);

    upstream.release_first();
    upstream.wait_for_first().await;
    upstream.release_final();
    upstream.wait_for_final().await;
    assert_eq!(upstream.attempts(), 1, "terminal marker is not body EOF");
    upstream.release_eof();
    upstream.wait_for_attempts(2).await;
    second
        .read_to_end(&mut Vec::new())
        .await
        .expect("second response EOF");
    let snapshot = wait_for_metric(&gateway, "active", 0).await;
    assert_eq!(snapshot["terminal_total"], 2);
    gateway.shutdown().await.expect("shutdown gateway");
}

#[tokio::test]
async fn metadata_endpoint_shares_stream_capacity() {
    let upstream =
        GatedUpstreamFixture::start(vec![FIRST_EVENT.to_vec()], vec![DONE_EVENT.to_vec()]).await;
    let gateway = gateway(&upstream, CancelPolicy::Drain).await;
    let mut active = open_raw(
        gateway.address(),
        &post("/r/pi-work/v1/chat/completions", CHAT_REQUEST),
    )
    .await;
    upstream.release_first();
    let _ = read_until(&mut active, b"first").await;

    let mut metadata = open_raw(gateway.address(), &get("/r/pi-work/v1/models")).await;
    tokio::time::sleep(Duration::from_millis(100)).await;
    assert_eq!(upstream.attempts(), 1, "metadata request shares capacity");
    upstream.release_final();
    upstream.wait_for_final().await;
    assert_eq!(upstream.attempts(), 1, "terminal marker retains capacity");
    upstream.release_eof();
    upstream.wait_for_attempts(2).await;
    metadata
        .read_to_end(&mut Vec::new())
        .await
        .expect("metadata response EOF");
    active
        .read_to_end(&mut Vec::new())
        .await
        .expect("active response EOF");
    gateway.shutdown().await.expect("shutdown gateway");
}

#[tokio::test]
async fn public_spawn_rejects_constructed_concurrency_outside_fixed_range() {
    let upstream = GatedUpstreamFixture::start(Vec::new(), Vec::new()).await;
    for concurrency in [0, 17, 255] {
        let mut constructed = config(&upstream, CancelPolicy::Drain);
        constructed.concurrency = concurrency;
        let credentials =
            RuntimeCredentials::new(CONTROL_TOKEN, None).expect("synthetic credentials");
        match server::spawn(constructed, credentials).await {
            Err(_) => {}
            Ok(handle) => {
                handle.shutdown().await.expect("shutdown rejected fixture");
                panic!("constructed concurrency {concurrency} was accepted");
            }
        }
    }
    for concurrency in [1, 16] {
        let mut constructed = config(&upstream, CancelPolicy::Drain);
        constructed.concurrency = concurrency;
        let credentials =
            RuntimeCredentials::new(CONTROL_TOKEN, None).expect("synthetic credentials");
        let handle = server::spawn(constructed, credentials)
            .await
            .expect("valid constructed concurrency");
        handle.shutdown().await.expect("shutdown valid fixture");
    }
}

#[tokio::test]
async fn pre_header_close_policy_closes_upstream_socket() {
    let upstream = GatedUpstreamFixture::start_before_headers(
        vec![FIRST_EVENT.to_vec()],
        vec![DONE_EVENT.to_vec()],
    )
    .await;
    let gateway = gateway(&upstream, CancelPolicy::Close).await;
    let request = post("/r/pi-work/v1/chat/completions", CHAT_REQUEST);
    let downstream = open_raw(gateway.address(), &request).await;
    upstream.wait_for_attempts(1).await;
    abort_socket(downstream);

    upstream.wait_for_disconnect().await;
    let snapshot = wait_for_metric(&gateway, "active", 0).await;
    assert_eq!(snapshot["terminal_downstream_close"], 1);
    gateway.shutdown().await.expect("shutdown gateway");
}

#[tokio::test]
async fn request_write_half_close_remains_valid_while_upstream_is_silent() {
    let upstream = GatedUpstreamFixture::start_before_headers(
        vec![FIRST_EVENT.to_vec()],
        vec![DONE_EVENT.to_vec()],
    )
    .await;
    let gateway = gateway(&upstream, CancelPolicy::Close).await;
    let request = post("/r/pi-work/v1/chat/completions", CHAT_REQUEST);
    let response = tokio::spawn({
        let address = gateway.address();
        async move { send_raw(address, &request).await }
    });
    upstream.wait_for_attempts(1).await;
    tokio::time::sleep(Duration::from_millis(100)).await;
    assert_eq!(status_snapshot(&gateway).await["active"], 1);

    upstream.release_first();
    upstream.wait_for_first().await;
    upstream.release_final();
    upstream.release_eof();
    let response = response.await.expect("half-close response task");
    assert_eq!(status(&response), 200);
    assert!(decode_chunked_response(&response).starts_with(FIRST_EVENT));
    gateway.shutdown().await.expect("shutdown gateway");
}

#[tokio::test]
async fn absolute_deadline_covers_silent_upstream_headers() {
    let upstream = GatedUpstreamFixture::start_before_headers(
        vec![FIRST_EVENT.to_vec()],
        vec![DONE_EVENT.to_vec()],
    )
    .await;
    let gateway = gateway_with_timeouts(
        &upstream,
        CancelPolicy::Drain,
        Duration::from_millis(150),
        Duration::from_millis(100),
    )
    .await;
    let response = send_raw(
        gateway.address(),
        &post("/r/pi-work/v1/chat/completions", CHAT_REQUEST),
    )
    .await;
    assert_eq!(status(&response), 504);
    upstream.wait_for_disconnect().await;
    let snapshot = wait_for_metric(&gateway, "active", 0).await;
    assert_eq!(snapshot["terminal_deadline"], 1);
    assert_eq!(snapshot["circuit_breaker"]["failures"], 1);
    assert_eq!(snapshot["circuit_breaker"]["tracked_attempts"], 0);
    gateway.shutdown().await.expect("shutdown gateway");
}

#[tokio::test]
async fn absolute_deadline_covers_silent_upstream_body() {
    let upstream =
        GatedUpstreamFixture::start(vec![FIRST_EVENT.to_vec()], vec![DONE_EVENT.to_vec()]).await;
    let gateway = gateway_with_timeouts(
        &upstream,
        CancelPolicy::Drain,
        Duration::from_millis(150),
        Duration::from_millis(100),
    )
    .await;
    let mut downstream = open_raw(
        gateway.address(),
        &post("/r/pi-work/v1/chat/completions", CHAT_REQUEST),
    )
    .await;
    let headers = read_until(&mut downstream, b"\r\n\r\n").await;
    assert_eq!(status(&headers), 200);
    upstream.wait_for_disconnect().await;
    let snapshot = wait_for_metric(&gateway, "active", 0).await;
    assert_eq!(snapshot["terminal_deadline"], 1);
    assert_eq!(snapshot["circuit_breaker"]["failures"], 0);
    assert_eq!(snapshot["circuit_breaker"]["tracked_attempts"], 0);
    abort_socket(downstream);
    gateway.shutdown().await.expect("shutdown gateway");
}

#[tokio::test]
async fn absolute_deadline_covers_blocked_downstream_delivery() {
    let final_body = vec![b'x'; 16 * 1024 * 1024];
    let upstream = GatedUpstreamFixture::start(vec![FIRST_EVENT.to_vec()], vec![final_body]).await;
    let gateway = gateway_with_timeouts(
        &upstream,
        CancelPolicy::Drain,
        Duration::from_millis(300),
        Duration::from_millis(100),
    )
    .await;
    let mut downstream = open_raw(
        gateway.address(),
        &post("/r/pi-work/v1/chat/completions", CHAT_REQUEST),
    )
    .await;
    upstream.release_first();
    let _ = read_until(&mut downstream, b"first").await;
    upstream.release_final();

    upstream.wait_for_disconnect().await;
    let snapshot = wait_for_metric(&gateway, "active", 0).await;
    assert_eq!(snapshot["terminal_deadline"], 1);
    assert_eq!(snapshot["max_queued_response_bytes"], 64 * 1024);
    assert!(!upstream.body_eof());
    let mut response = Vec::new();
    let read = downstream.read_to_end(&mut response).await;
    assert!(
        read.is_err() || !has_successful_chunked_eof(&response),
        "deadline must remain a downstream transport failure"
    );
    gateway.shutdown().await.expect("shutdown gateway");
}

#[tokio::test]
async fn upstream_body_error_is_not_a_clean_downstream_eof() {
    let upstream = GatedUpstreamFixture::start_with_body_error(
        vec![FIRST_EVENT.to_vec()],
        vec![DONE_EVENT.to_vec()],
    )
    .await;
    let gateway = gateway(&upstream, CancelPolicy::Drain).await;
    let mut downstream = open_raw(
        gateway.address(),
        &post("/r/pi-work/v1/chat/completions", CHAT_REQUEST),
    )
    .await;
    upstream.release_first();
    upstream.wait_for_first().await;
    upstream.release_final();
    upstream.wait_for_final().await;
    upstream.release_eof();

    let mut response = Vec::new();
    let read = downstream.read_to_end(&mut response).await;
    assert!(
        read.is_err() || !has_successful_chunked_eof(&response),
        "upstream body error must remain a downstream transport failure"
    );
    let snapshot = wait_for_metric(&gateway, "active", 0).await;
    assert_eq!(snapshot["terminal_upstream_error"], 1);
    assert!(!upstream.body_eof());
    gateway.shutdown().await.expect("shutdown gateway");
}

#[tokio::test]
async fn forced_shutdown_joins_a_worker_blocked_before_body_eof() {
    let upstream =
        GatedUpstreamFixture::start(vec![FIRST_EVENT.to_vec()], vec![DONE_EVENT.to_vec()]).await;
    let gateway = gateway_with_timeouts(
        &upstream,
        CancelPolicy::Drain,
        Duration::from_secs(5),
        Duration::from_millis(100),
    )
    .await;
    let mut downstream = open_raw(
        gateway.address(),
        &post("/r/pi-work/v1/chat/completions", CHAT_REQUEST),
    )
    .await;
    upstream.release_first();
    let _ = read_until(&mut downstream, b"first").await;

    let started = tokio::time::Instant::now();
    let status = server::testing::shutdown_with_status(gateway)
        .await
        .expect("forced shutdown gateway");
    let elapsed = started.elapsed();
    assert!(elapsed >= Duration::from_millis(80));
    assert!(elapsed < Duration::from_secs(1));
    let snapshot: Value = serde_json::from_slice(&status).expect("post-shutdown status JSON");
    assert_eq!(snapshot["active"], 0);
    assert_eq!(snapshot["terminal_total"], 1);
    assert_eq!(snapshot["terminal_shutdown"], 1);
    upstream.wait_for_disconnect().await;
    assert!(!upstream.body_eof());
    let mut response = Vec::new();
    let read = downstream.read_to_end(&mut response).await;
    assert!(
        read.is_err() || !has_successful_chunked_eof(&response),
        "forced worker interruption must remain a transport failure"
    );
}

#[tokio::test]
async fn stop_drains_active_worker_before_returning() {
    let upstream =
        GatedUpstreamFixture::start(vec![FIRST_EVENT.to_vec()], vec![DONE_EVENT.to_vec()]).await;
    let gateway = gateway(&upstream, CancelPolicy::Drain).await;
    let address = gateway.address();
    let mut downstream = open_raw(
        address,
        &post_with_connection("/r/pi-work/v1/chat/completions", CHAT_REQUEST, "keep-alive"),
    )
    .await;
    upstream.release_first();
    let _ = read_until(&mut downstream, b"first").await;

    let shutdown = tokio::spawn(gateway.shutdown());
    tokio::time::sleep(Duration::from_millis(100)).await;
    assert!(
        !shutdown.is_finished(),
        "shutdown waits for the active worker"
    );
    let rejected = send_raw(
        address,
        &post("/r/pi-work/v1/chat/completions", CHAT_REQUEST),
    )
    .await;
    assert_eq!(status(&rejected), 503, "draining rejects new data ingress");
    let control = send_raw(address, b"GET /_llmgw/status HTTP/1.1\r\nHost: localhost\r\nx-llmgw-control-token: synthetic-control-token\r\nConnection: close\r\n\r\n").await;
    assert_eq!(status(&control), 200);
    let control: Value = serde_json::from_slice(response_body(&control)).unwrap();
    assert_eq!(control["state"], "draining");
    let _ = downstream
        .write_all(&post("/r/pi-work/v1/chat/completions", CHAT_REQUEST))
        .await;
    upstream.release_final();
    upstream.release_eof();
    let _ = downstream.read_to_end(&mut Vec::new()).await;
    assert_eq!(
        upstream.attempts(),
        1,
        "stopped keepalive admits no request"
    );
    shutdown
        .await
        .expect("shutdown task")
        .expect("shutdown gateway");
}

#[tokio::test]
async fn known_tpm_drain_long_stream_charges_late_usage_and_blocks_next_attempt() {
    let upstream=GatedUpstreamFixture::start(vec![FIRST_EVENT.to_vec()],vec![b"data: {\"choices\":[],\"usage\":{\"prompt_tokens\":140,\"completion_tokens\":10}}\n\ndata: [DONE]\n\n".to_vec()]).await;
    let mut config = config(&upstream, CancelPolicy::Drain);
    config.accounting = Accounting::Actual;
    config.quota.rpm = Limit::Known(2.try_into().unwrap());
    config.quota.tpm = Limit::Known(100.try_into().unwrap());
    config.models[0].max_output_tokens = Some(10.try_into().unwrap());
    let clock = llmgw::admission::ManualClock::default();
    let gateway = server::testing::spawn_with_clock(
        config,
        RuntimeCredentials::new(CONTROL_TOKEN, None).unwrap(),
        clock.clone(),
        None,
    )
    .await
    .unwrap();
    clock.advance_to(Duration::from_secs(60));
    let request = post("/r/pi-work/v1/chat/completions", CHAT_REQUEST);
    let mut first = open_raw(gateway.address(), &request).await;
    upstream.release_first();
    read_until(&mut first, b"first").await;
    abort_socket(first);
    wait_for_metric(&gateway, "draining", 1).await;
    let mut second = open_raw(gateway.address(), &request).await;
    clock.advance_to(Duration::from_secs(130));
    let q = server::testing::quota_snapshot(&gateway);
    assert_eq!(q.active, 1);
    assert_eq!(q.tpm_debited, 0);
    assert_eq!(q.tpm_held, 0);
    upstream.release_final();
    upstream.wait_for_final().await;
    wait_for_metric(&gateway, "terminal_marker", 1).await;
    assert_eq!(server::testing::quota_snapshot(&gateway).active, 1);
    assert_eq!(upstream.attempts(), 1);
    upstream.release_eof();
    wait_for_metric(&gateway, "terminal_total", 1).await;
    assert_eq!(server::testing::quota_snapshot(&gateway).tpm_debited, 150);
    assert_eq!(server::testing::quota_snapshot(&gateway).active, 0);
    assert_eq!(upstream.attempts(), 1);
    clock.advance_to(Duration::from_secs(189));
    assert!(
        tokio::time::timeout(Duration::from_millis(20), read_until(&mut second, b"first"))
            .await
            .is_err()
    );
    assert_eq!(upstream.attempts(), 1);
    clock.advance_to(Duration::from_secs(190));
    upstream.wait_for_attempts(2).await;
    second.read_to_end(&mut Vec::new()).await.unwrap();
    let s = wait_for_metric(&gateway, "terminal_total", 2).await;
    assert_eq!(s["observed_input_tokens"], 280);
    assert_eq!(s["observed_output_tokens"], 20);
    let q = server::testing::quota_snapshot(&gateway);
    assert_eq!(q.cleanups, 2);
    assert_eq!(q.tpm_debited, 150);
    assert_eq!(q.starts, 2);
    gateway.shutdown().await.unwrap();
}
