//! Spec-review regressions: supported final usage is required for Actual refunds.
mod support;
use llmgw::admission::ManualClock;
use llmgw::config::{
    Accounting, Auth, CancelPolicy, Config, Endpoint, Limit, Model, Quota, Root, Upstream,
};
use llmgw::server::{self, RuntimeCredentials};
use serde_json::{Value, json};
use std::time::Duration;
use support::fixture::{UpstreamFixture, response_body, send_raw, status};

const CHAT_FINAL: &str = "data: {\"choices\":[],\"usage\":{\"prompt_tokens\":20,\"completion_tokens\":3,\"prompt_tokens_details\":{\"cached_tokens\":7}}}\n\n";
const DONE: &str = "data: [DONE]\n\n";
const RESP_FINAL: &str = "data: {\"type\":\"response.completed\",\"response\":{\"status\":\"completed\",\"error\":null,\"usage\":{\"input_tokens\":20,\"output_tokens\":3,\"input_tokens_details\":{\"cached_tokens\":7}}}}\n\n";
const MESSAGES_USAGE: &str = "event: message_start\ndata: {\"type\":\"message_start\",\"message\":{\"usage\":{\"input_tokens\":17,\"output_tokens\":1,\"cache_creation_input_tokens\":3,\"cache_read_input_tokens\":2}}}\n\nevent: message_delta\ndata: {\"type\":\"message_delta\",\"usage\":{\"output_tokens\":9}}\n\n";
const MESSAGE_STOP: &str = "event: message_stop\ndata: {\"type\":\"message_stop\"}\n\n";
const ERROR: &str = "data: {\"type\":\"error\",\"error\":{\"type\":\"synthetic_error\"}}\n\n";

fn incomplete_response(reason: &str) -> Value {
    json!({"status":"incomplete","error":null,"incomplete_details":{"reason":reason},"usage":{"input_tokens":20,"output_tokens":3,"input_tokens_details":{"cached_tokens":7}}})
}

fn incomplete_event(response: &Value) -> String {
    format!(
        "data: {}\n\n",
        json!({"type":"response.incomplete","response":response})
    )
}

async fn verify_incomplete(response: &Value, known: Option<u128>, accounting: Accounting) {
    verify_response(
        Endpoint::Responses,
        &response.to_string(),
        known,
        accounting,
        "200 OK",
        "Content-Type: application/json\r\n",
    )
    .await;
    verify_mode(
        Endpoint::Responses,
        &incomplete_event(response),
        known,
        accounting,
    )
    .await;
}

async fn verify(endpoint: Endpoint, sse: &str, known: Option<u128>) {
    verify_mode(endpoint, sse, known, Accounting::Actual).await;
}
async fn verify_mode(endpoint: Endpoint, sse: &str, known: Option<u128>, accounting: Accounting) {
    verify_response(
        endpoint,
        sse,
        known,
        accounting,
        "200 OK",
        "Content-Type: text/event-stream\r\n",
    )
    .await;
}
async fn verify_response(
    endpoint: Endpoint,
    payload: &str,
    known: Option<u128>,
    accounting: Accounting,
    status_line: &str,
    headers: &str,
) {
    let raw = format!(
        "HTTP/1.1 {status_line}\r\n{headers}Content-Length: {}\r\nConnection: close\r\n\r\n{payload}",
        payload.len()
    );
    let upstream = UpstreamFixture::start(raw.into_bytes()).await;
    let config = test_config(upstream.address(), endpoint, accounting);
    let clock = ManualClock::default();
    let gateway = server::testing::spawn_with_clock(
        config,
        RuntimeCredentials::new(b"synthetic-control", None).unwrap(),
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
    assert_eq!(status(&response), status_line[..3].parse::<u16>().unwrap());
    assert_eq!(
        response_body(&response),
        payload.as_bytes(),
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
async fn responses_incomplete_json_and_sse_settle_valid_usage_and_keep_reserved_estimate() {
    for reason in ["max_output_tokens", "content_filter"] {
        let mut response = incomplete_response(reason);
        for error_present in [true, false] {
            if !error_present {
                response.as_object_mut().unwrap().remove("error");
            }
            verify_incomplete(&response, Some(23), Accounting::Actual).await;
            verify_incomplete(&response, Some(23), Accounting::Reserved).await;
        }
        response["usage"]["output_tokens"] = json!(1200);
        verify_incomplete(&response, Some(1220), Accounting::Actual).await;
        response["usage"] = json!({"input_tokens":0,"output_tokens":0});
        verify_incomplete(&response, Some(0), Accounting::Actual).await;
    }
}

#[tokio::test]
async fn responses_valid_incomplete_repeats_settle_but_mixed_completed_terminals_do_not() {
    for reason in ["max_output_tokens", "content_filter"] {
        let terminal = incomplete_event(&incomplete_response(reason));
        verify(
            Endpoint::Responses,
            &format!("{terminal}data: {{\"type\":\"synthetic.notice\"}}\n\n{terminal}"),
            Some(23),
        )
        .await;
        for mixed in [
            format!("{RESP_FINAL}{terminal}"),
            format!("{terminal}{RESP_FINAL}"),
        ] {
            verify(Endpoint::Responses, &mixed, None).await;
        }
    }
}

#[tokio::test]
async fn responses_invalid_incomplete_envelopes_and_usage_stay_unknown() {
    let valid = incomplete_response("max_output_tokens");
    let terminal = incomplete_event(&valid);
    let mut invalid = Vec::new();
    for field in ["status", "incomplete_details", "usage"] {
        let mut response = valid.clone();
        response.as_object_mut().unwrap().remove(field);
        invalid.push(response);
    }
    for (pointer, replacement) in [
        ("/status", json!("completed")),
        ("/status", json!("failed")),
        ("/status", Value::Null),
        ("/error", json!({"type":"synthetic_error"})),
        ("/incomplete_details", Value::Null),
        ("/incomplete_details", json!({})),
        ("/incomplete_details/reason", json!("unknown_reason")),
        ("/incomplete_details/reason", json!(3)),
        ("/usage", Value::Null),
        ("/usage", json!({"total_tokens":23})),
        ("/usage/input_tokens", json!(-1)),
        ("/usage/output_tokens", json!(1.5)),
        ("/usage/output_tokens", json!("3")),
        ("/usage/output_tokens", Value::Null),
        ("/usage/output_tokens", json!(true)),
        ("/usage/input_tokens_details", json!("invalid")),
        ("/usage/input_tokens_details/cached_tokens", json!(21)),
        ("/usage/input_tokens_details/cached_tokens", json!(-1)),
    ] {
        let mut response = valid.clone();
        *response.pointer_mut(pointer).unwrap() = replacement;
        invalid.push(response);
    }
    for response in invalid {
        verify_incomplete(&response, None, Accounting::Actual).await;
        let bad = incomplete_event(&response);
        for sse in [format!("{bad}{terminal}"), format!("{terminal}{bad}")] {
            verify(Endpoint::Responses, &sse, None).await;
        }
    }
}

#[tokio::test]
async fn responses_incomplete_errors_and_decreasing_repeats_keep_reservation() {
    for reason in ["max_output_tokens", "content_filter"] {
        let response = incomplete_response(reason);
        let terminal = incomplete_event(&response);
        for error in [
            ERROR,
            "data: {\"type\":\"response.failed\"}\n\n",
            "event: error\ndata: {\"message\":\"synthetic failure\"}\n\n",
        ] {
            for sse in [format!("{error}{terminal}"), format!("{terminal}{error}")] {
                verify(Endpoint::Responses, &sse, None).await;
            }
        }
        for pointer in [
            "/usage/input_tokens",
            "/usage/output_tokens",
            "/usage/input_tokens_details/cached_tokens",
        ] {
            let mut decreasing = response.clone();
            let count = decreasing.pointer_mut(pointer).unwrap();
            *count = json!(count.as_u64().unwrap() - 1);
            verify(
                Endpoint::Responses,
                &format!("{terminal}{}{terminal}", incomplete_event(&decreasing)),
                None,
            )
            .await;
        }
    }
}

#[tokio::test]
async fn responses_overflow_is_unknown_and_cannot_be_repaired_by_a_later_terminal() {
    for status in ["completed", "incomplete"] {
        let mut response = incomplete_response("max_output_tokens");
        response["status"] = json!(status);
        if status == "completed" {
            response["incomplete_details"] = Value::Null;
        }
        let terminal = format!(
            "data: {}\n\n",
            json!({"type":format!("response.{status}"),"response":response})
        );
        response["usage"]["input_tokens"] = json!(u64::MAX);
        response["usage"]["output_tokens"] = json!(1);
        verify_response(
            Endpoint::Responses,
            &response.to_string(),
            None,
            Accounting::Actual,
            "200 OK",
            "Content-Type: application/json\r\n",
        )
        .await;
        let overflow = format!(
            "data: {}\n\n",
            json!({"type":format!("response.{status}"),"response":response})
        );
        for sse in [overflow.clone(), format!("{overflow}{terminal}")] {
            verify(Endpoint::Responses, &sse, None).await;
        }
    }
}

#[tokio::test]
async fn responses_incomplete_usage_waits_for_clean_json_and_sse_eof() {
    use support::fixture::{GatedUpstreamFixture, open_raw, read_until};
    use tokio::io::AsyncReadExt;

    let response = incomplete_response("max_output_tokens");
    for stream in [false, true] {
        let payload = if stream {
            incomplete_event(&response)
        } else {
            response.to_string()
        };
        for fail_body in [false, true] {
            let chunks = vec![payload.as_bytes().to_vec()];
            let upstream = if !stream {
                GatedUpstreamFixture::start_json(chunks, vec![], fail_body).await
            } else if fail_body {
                GatedUpstreamFixture::start_with_body_error(chunks, vec![]).await
            } else {
                GatedUpstreamFixture::start(chunks, vec![]).await
            };
            let mut config =
                test_config(upstream.address(), Endpoint::Responses, Accounting::Actual);
            config.startup_hold_secs = 0;
            let gateway = server::spawn(
                config,
                RuntimeCredentials::new(b"synthetic-control", None).unwrap(),
            )
            .await
            .unwrap();
            let body = r#"{"model":"synthetic","input":"synthetic"}"#;
            let request = format!(
                "POST /r/review-fix/v1/responses HTTP/1.1\r\nHost: localhost\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                body.len()
            );
            let mut socket = open_raw(gateway.address(), request.as_bytes()).await;
            upstream.release_first();
            let mut received = read_until(&mut socket, payload.as_bytes()).await;
            let before = server::testing::quota_snapshot(&gateway);
            assert_eq!(before.cleanups, 0);
            assert_eq!(before.active, 1, "terminal usage cannot release the slot");
            assert_eq!(before.tpm_debited, body.len() as u128 + 300);
            upstream.release_final();
            upstream.release_eof();
            tokio::time::timeout(Duration::from_secs(2), socket.read_to_end(&mut received))
                .await
                .unwrap()
                .unwrap();
            let after = server::testing::quota_snapshot(&gateway);
            assert_eq!(after.cleanups, 1);
            assert_eq!(after.active, 0);
            assert_eq!(
                after.tpm_debited,
                if fail_body {
                    body.len() as u128 + 300
                } else {
                    23
                }
            );
            let metrics: Value = serde_json::from_slice(
                &server::testing::shutdown_with_status(gateway)
                    .await
                    .unwrap(),
            )
            .unwrap();
            assert_eq!(metrics["usage_known"], u64::from(!fail_body));
        }
    }
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

const CHAT_JSON: &str = r#"{"choices":[{"finish_reason":"stop","message":{"role":"assistant","content":"x"}}],"usage":{"prompt_tokens":20,"completion_tokens":3,"prompt_tokens_details":{"cached_tokens":7}}}"#;
const RESP_JSON: &str = r#"{"status":"completed","error":null,"incomplete_details":null,"output":[],"usage":{"input_tokens":20,"output_tokens":3,"input_tokens_details":{"cached_tokens":7}}}"#;
const MSG_JSON: &str = r#"{"type":"message","role":"assistant","stop_reason":"end_turn","content":[],"usage":{"input_tokens":17,"output_tokens":9,"cache_creation_input_tokens":3,"cache_read_input_tokens":2}}"#;

#[tokio::test]
async fn json_final_usage_reconciles_all_three_endpoints() {
    for (endpoint, body, total) in [
        (Endpoint::ChatCompletions, CHAT_JSON, 23),
        (Endpoint::Responses, RESP_JSON, 23),
        (Endpoint::Messages, MSG_JSON, 31),
    ] {
        verify_response(
            endpoint,
            body,
            Some(total),
            Accounting::Actual,
            "200 OK",
            "Content-Type: application/json; charset=utf-8\r\n",
        )
        .await;
    }
}

#[tokio::test]
async fn json_unknown_representations_and_incomplete_bodies_keep_reservations() {
    for headers in [
        "Content-Type: application/json\r\nContent-Encoding: gzip\r\n",
        "Content-Type: application/json\r\nContent-Encoding: identity\r\nContent-Encoding: identity\r\n",
        "Content-Type: application/json\r\nContent-Type: application/json\r\n",
        "Content-Type: application/json\r\nContent-Type: text/event-stream\r\n",
        "Content-Type: application/json\r\nContent-Encoding: identity, gzip\r\n",
        "Content-Type: text/plain\r\n",
        "",
    ] {
        verify_response(
            Endpoint::ChatCompletions,
            CHAT_JSON,
            None,
            Accounting::Actual,
            "200 OK",
            headers,
        )
        .await;
    }
    for status in ["206 Partial Content", "500 Synthetic"] {
        verify_response(
            Endpoint::ChatCompletions,
            CHAT_JSON,
            None,
            Accounting::Actual,
            status,
            "Content-Type: application/json\r\n",
        )
        .await;
    }
    for body in [
        CHAT_JSON[..CHAT_JSON.len() - 1].to_owned(),
        format!("{CHAT_JSON}{{}}"),
        format!("{CHAT_JSON}{}", " ".repeat(256 * 1024)),
    ] {
        verify_response(
            Endpoint::ChatCompletions,
            &body,
            None,
            Accounting::Actual,
            "200 OK",
            "Content-Type: application/json\r\n",
        )
        .await;
    }
    for (endpoint, body) in [
        (Endpoint::ChatCompletions, CHAT_JSON),
        (Endpoint::Responses, RESP_JSON),
        (Endpoint::Messages, MSG_JSON),
    ] {
        let value: serde_json::Value = serde_json::from_str(body).unwrap();
        for (field, replacement) in [
            ("usage", serde_json::Value::Null),
            ("error", serde_json::json!({"message":"synthetic"})),
        ] {
            let mut bad = value.clone();
            bad[field] = replacement;
            verify_response(
                endpoint,
                &bad.to_string(),
                None,
                Accounting::Actual,
                "200 OK",
                "Content-Type: application/json\r\n",
            )
            .await;
        }
        let mut bad = value.clone();
        match endpoint {
            Endpoint::ChatCompletions => {
                bad["choices"][0]["finish_reason"] = serde_json::Value::Null
            }
            Endpoint::Responses => bad["status"] = serde_json::json!("in_progress"),
            _ => bad["stop_reason"] = serde_json::Value::Null,
        }
        verify_response(
            endpoint,
            &bad.to_string(),
            None,
            Accounting::Actual,
            "200 OK",
            "Content-Type: application/json\r\n",
        )
        .await;
    }
}

#[tokio::test]
async fn json_actual_creates_debt_reserved_keeps_estimate_and_tool_stops_settle() {
    for (endpoint, body, total, old, new) in [
        (
            Endpoint::ChatCompletions,
            CHAT_JSON,
            23,
            "stop",
            "tool_calls",
        ),
        (Endpoint::Responses, RESP_JSON, 23, "completed", "completed"),
        (Endpoint::Messages, MSG_JSON, 31, "end_turn", "max_tokens"),
    ] {
        verify_response(
            endpoint,
            &body.replace(old, new),
            Some(total),
            Accounting::Actual,
            "200 OK",
            "Content-Type: application/json\r\nContent-Encoding: identity\r\n",
        )
        .await;
        verify_response(
            endpoint,
            body,
            Some(total),
            Accounting::Reserved,
            "200 OK",
            "Content-Type: application/json\r\n",
        )
        .await;
    }
    verify_response(
        Endpoint::ChatCompletions,
        &CHAT_JSON.replace("\"completion_tokens\":3", "\"completion_tokens\":1200"),
        Some(1220),
        Accounting::Actual,
        "200 OK",
        "Content-Type: application/json\r\n",
    )
    .await;
}

fn test_config(
    address: std::net::SocketAddr,
    endpoint: Endpoint,
    accounting: Accounting,
) -> Config {
    Config {
        listen: "127.0.0.1:0".parse().unwrap(),
        concurrency: 1,
        startup_hold_secs: 60,
        cache: None,
        cancel_policy: CancelPolicy::Drain,
        accounting,
        retry_transient_429: false,
        upstream: Upstream {
            api_base: format!("http://{address}/v1").parse().unwrap(),
            auth: Auth::None,
            proxy: None,
            ca_bundle: None,
        },
        quota: Quota {
            rpm: Limit::Unlimited,
            tpm: Limit::Known(1000.try_into().unwrap()),
        },
        models: vec![Model {
            input_estimator: Default::default(),
            input_token_overhead: 0,
            id: "synthetic".into(),
            max_output_tokens: Some(300.try_into().unwrap()),
        }],
        roots: vec![Root {
            id: "review-fix".into(),
            endpoints: vec![endpoint],
            models: vec!["synthetic".into()],
        }],
    }
}

#[tokio::test]
async fn json_chunks_forward_before_eof_and_close_error_deadline_never_refund() {
    use support::fixture::{GatedUpstreamFixture, abort_socket, open_raw, read_until};
    use tokio::io::AsyncReadExt;
    for outcome in ["eof", "drain", "close", "body_error", "deadline"] {
        let split = CHAT_JSON.len() / 2;
        let upstream = GatedUpstreamFixture::start_json(
            vec![CHAT_JSON.as_bytes()[..split].to_vec()],
            vec![CHAT_JSON.as_bytes()[split..].to_vec()],
            outcome == "body_error",
        )
        .await;
        let mut config = test_config(
            upstream.address(),
            Endpoint::ChatCompletions,
            Accounting::Actual,
        );
        config.startup_hold_secs = 0;
        config.cache = Some(Default::default());
        config.cancel_policy = if outcome == "close" {
            CancelPolicy::Close
        } else {
            CancelPolicy::Drain
        };
        let gateway = server::testing::spawn_with_timeouts(
            config,
            RuntimeCredentials::new(b"synthetic-control", None).unwrap(),
            if outcome == "deadline" {
                Duration::from_millis(200)
            } else {
                Duration::from_secs(5)
            },
            Duration::from_millis(200),
        )
        .await
        .unwrap();
        let body = r#"{"model":"synthetic","messages":[{"role":"user","content":"x"}]}"#;
        let request = format!(
            "POST /r/review-fix/v1/chat/completions HTTP/1.1\r\nHost: localhost\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
            body.len()
        );
        let mut socket = open_raw(gateway.address(), request.as_bytes()).await;
        upstream.release_first();
        let mut received = read_until(&mut socket, &CHAT_JSON.as_bytes()[..split]).await;
        let before = server::testing::quota_snapshot(&gateway);
        assert_eq!(
            before.cleanups, 0,
            "must retain worker and reservation until HTTP EOF"
        );
        assert_eq!(before.tpm_debited, body.len() as u128 + 300);
        let aborted = matches!(outcome, "drain" | "close");
        if aborted {
            abort_socket(socket);
            if outcome == "close" {
                upstream.wait_for_disconnect().await;
            } else {
                tokio::time::timeout(Duration::from_secs(2), async {
                    loop {
                        let client = reqwest::Client::builder().no_proxy().build().unwrap();
                        let response = client
                            .get(format!("http://{}/_llmgw/status", gateway.address()))
                            .header("x-llmgw-control-token", "synthetic-control")
                            .send()
                            .await
                            .unwrap();
                        let metrics: serde_json::Value =
                            serde_json::from_slice(&response.bytes().await.unwrap()).unwrap();
                        if metrics["draining"] == 1 {
                            break;
                        }
                        tokio::task::yield_now().await;
                    }
                })
                .await
                .unwrap();
            }
        } else {
            upstream.release_final();
            received.extend(read_until(&mut socket, &CHAT_JSON.as_bytes()[split..]).await);
            assert_eq!(server::testing::quota_snapshot(&gateway).cleanups, 0);
            if outcome != "deadline" {
                upstream.release_eof();
            }
            tokio::time::timeout(Duration::from_secs(2), socket.read_to_end(&mut received))
                .await
                .unwrap()
                .unwrap();
        }
        if outcome == "drain" {
            upstream.release_final();
            upstream.release_eof();
        }
        tokio::time::timeout(Duration::from_secs(2), async {
            while server::testing::quota_snapshot(&gateway).cleanups == 0 {
                tokio::task::yield_now().await;
            }
        })
        .await
        .unwrap();
        let known = matches!(outcome, "eof" | "drain");
        assert_eq!(
            server::testing::quota_snapshot(&gateway).tpm_debited,
            if known { 23 } else { body.len() as u128 + 300 },
            "{outcome}"
        );
        let control = send_raw(gateway.address(), b"GET /_llmgw/status HTTP/1.1\r\nHost: localhost\r\nX-LLMGW-Control-Token: synthetic-control\r\nConnection: close\r\n\r\n").await;
        let metrics: serde_json::Value = serde_json::from_slice(response_body(&control)).unwrap();
        gateway.shutdown().await.unwrap();
        assert_eq!(metrics["usage_known"], u64::from(known), "{outcome}");
        assert_eq!(
            metrics["exact_cache"]["stores"],
            u64::from(outcome == "eof"),
            "{outcome}"
        );
    }
}
