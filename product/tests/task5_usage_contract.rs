//! Spec-review regressions: supported final usage is required for Actual refunds.
mod support;
use llmgw::admission::ManualClock;
use llmgw::config::{
    Accounting, Auth, CancelPolicy, Config, Endpoint, Limit, Model, Quota, Root, Upstream,
};
use llmgw::server::{self, RuntimeCredentials};
use std::time::Duration;
use support::fixture::{UpstreamFixture, response_body, send_raw, status};

const CHAT_FINAL: &str = "data: {\"choices\":[],\"usage\":{\"prompt_tokens\":20,\"completion_tokens\":3,\"prompt_tokens_details\":{\"cached_tokens\":7}}}\n\n";
const DONE: &str = "data: [DONE]\n\n";
const RESP_FINAL: &str = "data: {\"type\":\"response.completed\",\"response\":{\"status\":\"completed\",\"error\":null,\"usage\":{\"input_tokens\":20,\"output_tokens\":3,\"input_tokens_details\":{\"cached_tokens\":7}}}}\n\n";
const MESSAGES_USAGE: &str = "event: message_start\ndata: {\"type\":\"message_start\",\"message\":{\"usage\":{\"input_tokens\":17,\"output_tokens\":1,\"cache_creation_input_tokens\":3,\"cache_read_input_tokens\":2}}}\n\nevent: message_delta\ndata: {\"type\":\"message_delta\",\"usage\":{\"output_tokens\":9}}\n\n";
const MESSAGE_STOP: &str = "event: message_stop\ndata: {\"type\":\"message_stop\"}\n\n";
const ERROR: &str = "data: {\"type\":\"error\",\"error\":{\"type\":\"synthetic_error\"}}\n\n";

async fn verify(endpoint: Endpoint, sse: &str, known: Option<u128>) {
    verify_mode(endpoint, sse, known, Accounting::Actual).await;
}
async fn verify_mode(endpoint: Endpoint, sse: &str, known: Option<u128>, accounting: Accounting) {
    let raw = format!(
        "HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{sse}",
        sse.len()
    );
    let upstream = UpstreamFixture::start(raw.into_bytes()).await;
    let config = Config {
        listen: "127.0.0.1:0".parse().unwrap(),
        concurrency: 1,
        cancel_policy: CancelPolicy::Drain,
        accounting,
        retry_transient_429: false,
        upstream: Upstream {
            api_base: format!("http://{}/v1", upstream.address()).parse().unwrap(),
            auth: Auth::None,
            proxy: None,
            ca_bundle: None,
        },
        quota: Quota {
            rpm: Limit::Unlimited,
            tpm: Limit::Known(1000.try_into().unwrap()),
        },
        models: vec![Model {
            id: "synthetic".into(),
            max_output_tokens: Some(300.try_into().unwrap()),
        }],
        roots: vec![Root {
            id: "review-fix".into(),
            endpoints: vec![endpoint],
            models: vec!["synthetic".into()],
        }],
    };
    let clock = ManualClock::default();
    let gateway = server::testing::spawn_with_clock(
        config,
        RuntimeCredentials::new(b"synthetic-data", b"synthetic-control", None).unwrap(),
        clock.clone(),
        None,
    )
    .await
    .unwrap();
    clock.advance_to(Duration::from_secs(60));
    let body = if endpoint == Endpoint::Responses {
        r#"{"model":"synthetic","input":"synthetic"}"#
    } else {
        r#"{"model":"synthetic","messages":[]}"#
    };
    let estimate = body.len() as u128 + 300;
    let request = format!(
        "POST /r/review-fix/v1/{} HTTP/1.1\r\nHost: localhost\r\nX-LLMGW-Token: synthetic-data\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
        endpoint.path(),
        body.len()
    );
    let response = send_raw(gateway.address(), request.as_bytes()).await;
    assert_eq!(status(&response), 200);
    assert_eq!(
        response_body(&response),
        sse.as_bytes(),
        "observation must not change wire bytes"
    );
    let snapshot = server::testing::quota_snapshot(&gateway);
    assert_eq!(snapshot.starts, 1);
    assert_eq!(snapshot.cleanups, 1);
    assert_eq!(snapshot.active, 0);
    assert_eq!(upstream.attempts(), 1);
    let metrics: serde_json::Value = serde_json::from_slice(
        &server::testing::shutdown_with_status(gateway)
            .await
            .unwrap(),
    )
    .unwrap();
    let expected = if accounting == Accounting::Reserved {
        estimate
    } else {
        known.unwrap_or(estimate)
    };
    assert_eq!(
        snapshot.tpm_debited, expected,
        "unsupported final usage must retain the original reservation"
    );
    assert_eq!(metrics["usage_known"], u64::from(known.is_some()));
    assert_eq!(metrics["usage_unknown"], u64::from(known.is_none()));
}

#[tokio::test]
async fn chat_nonempty_choices_usage_is_not_a_final_report() {
    verify(Endpoint::ChatCompletions,&format!("data: {{\"choices\":[{{\"delta\":{{\"content\":\"synthetic\"}}}}],\"usage\":{{\"prompt_tokens\":1,\"completion_tokens\":1}}}}\n\n{DONE}"),None).await;
}
#[tokio::test]
async fn chat_missing_choices_usage_is_not_a_final_report() {
    verify(
        Endpoint::ChatCompletions,
        &format!("data: {{\"usage\":{{\"prompt_tokens\":1,\"completion_tokens\":1}}}}\n\n{DONE}"),
        None,
    )
    .await;
}
#[tokio::test]
async fn chat_malformed_choices_usage_is_not_a_final_report() {
    for choices in ["null", "{}", "\"malformed\"", "0"] {
        verify(Endpoint::ChatCompletions,&format!("data: {{\"choices\":{choices},\"usage\":{{\"prompt_tokens\":1,\"completion_tokens\":1}}}}\n\n{DONE}"),None).await;
    }
}
#[tokio::test]
async fn chat_null_usage_delta_then_final_report_preserves_cached_subset() {
    verify(Endpoint::ChatCompletions,&format!("data: {{\"choices\":[{{\"delta\":{{\"content\":\"synthetic\"}}}}],\"usage\":null}}\n\n{CHAT_FINAL}{DONE}"),Some(23)).await;
}
#[tokio::test]
async fn chat_numeric_interim_usage_does_not_replace_supported_final_report() {
    verify(Endpoint::ChatCompletions,&format!("data: {{\"choices\":[{{\"delta\":{{\"content\":\"synthetic\"}}}}],\"usage\":{{\"prompt_tokens\":1,\"completion_tokens\":1}}}}\n\n{CHAT_FINAL}{DONE}"),Some(23)).await;
}
#[tokio::test]
async fn chat_final_usage_must_precede_done() {
    verify(
        Endpoint::ChatCompletions,
        &format!("{DONE}{CHAT_FINAL}"),
        None,
    )
    .await;
}
#[tokio::test]
async fn chat_error_cannot_leave_successful_usage_for_refund() {
    for sse in [
        format!("{CHAT_FINAL}{ERROR}{DONE}"),
        format!("{ERROR}{CHAT_FINAL}{DONE}"),
    ] {
        verify(Endpoint::ChatCompletions, &sse, None).await;
    }
}
#[tokio::test]
async fn responses_failed_after_completed_keeps_reservation() {
    verify(
        Endpoint::Responses,
        &format!("{RESP_FINAL}data: {{\"type\":\"response.failed\"}}\n\n"),
        None,
    )
    .await;
}
#[tokio::test]
async fn responses_failed_before_completed_keeps_reservation() {
    verify(
        Endpoint::Responses,
        &format!("data: {{\"type\":\"response.failed\"}}\n\n{RESP_FINAL}"),
        None,
    )
    .await;
}
#[tokio::test]
async fn responses_incomplete_after_completed_keeps_reservation() {
    verify(
        Endpoint::Responses,
        &format!("{RESP_FINAL}data: {{\"type\":\"response.incomplete\"}}\n\n"),
        None,
    )
    .await;
}
#[tokio::test]
async fn responses_incomplete_before_completed_keeps_reservation() {
    verify(
        Endpoint::Responses,
        &format!("data: {{\"type\":\"response.incomplete\"}}\n\n{RESP_FINAL}"),
        None,
    )
    .await;
}
#[tokio::test]
async fn responses_error_before_or_after_completed_keeps_reservation() {
    for sse in [
        format!("{ERROR}{RESP_FINAL}"),
        format!("{RESP_FINAL}{ERROR}"),
    ] {
        verify(Endpoint::Responses, &sse, None).await;
    }
}
#[tokio::test]
async fn responses_conflicting_nested_status_or_error_keeps_reservation() {
    for detail in [
        r#""status":"failed""#,
        r#""status":"incomplete""#,
        r#""error":{"type":"synthetic_error"}"#,
    ] {
        verify(Endpoint::Responses,&format!("data: {{\"type\":\"response.completed\",\"response\":{{{detail},\"usage\":{{\"input_tokens\":1,\"output_tokens\":1}}}}}}\n\n"),None).await;
    }
}
#[tokio::test]
async fn responses_later_completed_without_usage_cannot_reuse_stale_usage() {
    verify(
        Endpoint::Responses,
        &format!("{RESP_FINAL}data: {{\"type\":\"response.completed\",\"response\":{{}}}}\n\n"),
        None,
    )
    .await;
}
#[tokio::test]
async fn consistent_repeated_terminal_reports_and_unknown_events_remain_supported() {
    let unknown = "data: {\"type\":\"synthetic.notice\"}\n\n";
    verify(
        Endpoint::Responses,
        &format!("{RESP_FINAL}{unknown}{RESP_FINAL}"),
        Some(23),
    )
    .await;
    verify(
        Endpoint::ChatCompletions,
        &format!("{CHAT_FINAL}{CHAT_FINAL}{DONE}{DONE}"),
        Some(23),
    )
    .await;
    verify(
        Endpoint::Messages,
        &format!("{MESSAGES_USAGE}{unknown}{MESSAGE_STOP}{MESSAGE_STOP}"),
        Some(31),
    )
    .await;
}
#[tokio::test]
async fn messages_error_before_final_stop_keeps_reservation() {
    verify(
        Endpoint::Messages,
        &format!("{MESSAGES_USAGE}{ERROR}{MESSAGE_STOP}"),
        None,
    )
    .await;
}
#[tokio::test]
async fn messages_error_after_final_stop_keeps_reservation() {
    verify(
        Endpoint::Messages,
        &format!("{MESSAGES_USAGE}{MESSAGE_STOP}{ERROR}"),
        None,
    )
    .await;
}
#[tokio::test]
async fn messages_named_error_event_without_json_type_keeps_reservation() {
    verify(Endpoint::Messages,&format!("{MESSAGES_USAGE}{MESSAGE_STOP}event: error\ndata: {{\"error\":{{\"type\":\"synthetic_error\"}}}}\n\n"),None).await;
}
#[tokio::test]
async fn reserved_accounting_keeps_estimate_with_valid_supported_final_usage() {
    verify_mode(
        Endpoint::ChatCompletions,
        &format!("{CHAT_FINAL}{DONE}"),
        Some(23),
        Accounting::Reserved,
    )
    .await;
    verify_mode(
        Endpoint::Responses,
        RESP_FINAL,
        Some(23),
        Accounting::Reserved,
    )
    .await;
    verify_mode(
        Endpoint::Messages,
        &format!("{MESSAGES_USAGE}{MESSAGE_STOP}"),
        Some(31),
        Accounting::Reserved,
    )
    .await;
}

#[tokio::test]
async fn named_error_event_without_json_type_invalidates_responses_usage() {
    verify(
        Endpoint::Responses,
        &format!("{RESP_FINAL}event: error\ndata: {{\"message\":\"synthetic failure\"}}\n\n"),
        None,
    )
    .await;
}
#[tokio::test]
async fn named_error_event_without_json_error_field_invalidates_chat_usage() {
    verify(
        Endpoint::ChatCompletions,
        &format!("{CHAT_FINAL}event: error\ndata: {{\"message\":\"synthetic failure\"}}\n\n{DONE}"),
        None,
    )
    .await;
}

const LOW_MESSAGE_START: &str = "event: message_start\ndata: {\"type\":\"message_start\",\"message\":{\"usage\":{\"input_tokens\":1,\"output_tokens\":0}}}\n\n";
const LOW_MESSAGE_DELTA: &str =
    "event: message_delta\ndata: {\"type\":\"message_delta\",\"usage\":{\"output_tokens\":1}}\n\n";

#[tokio::test]
async fn quality_messages_start_stop_delta_cannot_qualify_final_usage() {
    verify(
        Endpoint::Messages,
        &format!("{LOW_MESSAGE_START}{MESSAGE_STOP}{LOW_MESSAGE_DELTA}"),
        None,
    )
    .await;
}
#[tokio::test]
async fn quality_messages_stop_start_delta_cannot_qualify_final_usage() {
    verify(
        Endpoint::Messages,
        &format!("{MESSAGE_STOP}{LOW_MESSAGE_START}{LOW_MESSAGE_DELTA}"),
        None,
    )
    .await;
}
#[tokio::test]
async fn quality_messages_qualified_usage_then_post_stop_delta_is_unknown() {
    for output in [10, 9] {
        verify(Endpoint::Messages,&format!("{MESSAGES_USAGE}{MESSAGE_STOP}event: message_delta\ndata: {{\"type\":\"message_delta\",\"usage\":{{\"output_tokens\":{output}}}}}\n\n"),None).await;
    }
}
#[tokio::test]
async fn quality_messages_qualified_usage_then_post_stop_start_is_unknown() {
    for input in [18, 17] {
        verify(Endpoint::Messages,&format!("{MESSAGES_USAGE}{MESSAGE_STOP}event: message_start\ndata: {{\"type\":\"message_start\",\"message\":{{\"usage\":{{\"input_tokens\":{input},\"output_tokens\":9,\"cache_creation_input_tokens\":3,\"cache_read_input_tokens\":2}}}}}}\n\n"),None).await;
    }
}
#[tokio::test]
async fn quality_messages_ordered_start_delta_stop_settles_known_usage() {
    verify(
        Endpoint::Messages,
        &format!("{LOW_MESSAGE_START}{LOW_MESSAGE_DELTA}{MESSAGE_STOP}"),
        Some(2),
    )
    .await;
}
#[tokio::test]
async fn quality_messages_non_accounting_events_after_stop_preserve_final_usage() {
    verify(Endpoint::Messages,&format!("{MESSAGES_USAGE}{MESSAGE_STOP}event: message_delta\ndata: {{\"type\":\"message_delta\",\"delta\":{{\"stop_reason\":\"end_turn\"}}}}\n\nevent: message_start\ndata: {{\"type\":\"message_start\",\"message\":{{}}}}\n\ndata: {{\"type\":\"synthetic.notice\"}}\n\n{MESSAGE_STOP}"),Some(31)).await;
}
