//! Fixed classifications only: no error messages, request IDs, or provider labels retained.
use serde_json::Value;

use crate::config::Endpoint;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ResponseError {
    Server,
    RateLimit,
    Client,
    Unknown,
}

pub(crate) fn response_error(
    endpoint: Endpoint,
    event: Option<&str>,
    value: &Value,
) -> Option<ResponseError> {
    let kind = value.get("type").and_then(Value::as_str);
    let nested = (endpoint == Endpoint::Responses)
        .then(|| value.pointer("/response/error"))
        .flatten();
    let error = value
        .get("error")
        .filter(|error| !error.is_null())
        .or_else(|| nested.filter(|error| !error.is_null()));
    let failed = event == Some("error")
        || kind == Some("error")
        || (endpoint == Endpoint::Responses
            && (kind == Some("response.failed")
                || value.get("status").and_then(Value::as_str) == Some("failed")))
        || (endpoint == Endpoint::ChatCompletions
            && value
                .get("choices")
                .and_then(Value::as_array)
                .is_some_and(|choices| {
                    choices.iter().any(|choice| {
                        choice.get("finish_reason").and_then(Value::as_str) == Some("error")
                    })
                }));
    if error.is_none() && !failed {
        return None;
    }
    let error = error.unwrap_or(value);
    let mut classified = None;
    for name in ["status", "status_code", "code", "type"] {
        let Some(signal) = error.get(name).and_then(|value| classify(endpoint, value)) else {
            continue;
        };
        if classified.is_some_and(|previous| previous != signal) {
            return Some(ResponseError::Unknown);
        }
        classified = Some(signal);
    }
    Some(classified.unwrap_or(ResponseError::Unknown))
}

fn classify(endpoint: Endpoint, value: &Value) -> Option<ResponseError> {
    use ResponseError::*;
    if let Some(code) = value
        .as_u64()
        .or_else(|| value.as_str()?.parse::<u64>().ok())
    {
        return Some(match code {
            500 | 502 | 503 | 504 | 529 => Server,
            429 => RateLimit,
            400..=499 => Client,
            _ => Unknown,
        });
    }
    Some(match value.as_str()? {
        "server_error" | "internal_server_error" => Server,
        "api_error" | "overloaded_error" | "timeout_error" if endpoint == Endpoint::Messages => {
            Server
        }
        "rate_limit_error" | "rate_limit_exceeded" => RateLimit,
        "invalid_request_error"
        | "authentication_error"
        | "permission_error"
        | "not_found_error"
        | "billing_error"
        | "conflict_error"
        | "request_too_large"
        | "context_length_exceeded"
        | "insufficient_quota" => Client,
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn only_explicit_machine_codes_classify_outages() {
        use Endpoint::*;
        use ResponseError::*;
        for (endpoint, event, value, expected) in [
            (
                ChatCompletions,
                None,
                json!({"error":{"status_code":"401"}}),
                Some(Client),
            ),
            (
                ChatCompletions,
                None,
                json!({"error":{"status_code":"401","type":"server_error"}}),
                Some(Unknown),
            ),
            (
                ChatCompletions,
                None,
                json!({"error":{"code":500}}),
                Some(Server),
            ),
            (
                Responses,
                None,
                json!({"type":"response.failed","response":{"error":{"code":"server_error"}}}),
                Some(Server),
            ),
            (
                Messages,
                Some("error"),
                json!({"error":{"type":"overloaded_error"}}),
                Some(Server),
            ),
            (
                Messages,
                None,
                json!({"type":"error","error":{"type":"api_error"}}),
                Some(Server),
            ),
            (
                ChatCompletions,
                None,
                json!({"error":{"code":"429"}}),
                Some(RateLimit),
            ),
            (
                Messages,
                None,
                json!({"error":{"type":"rate_limit_error"}}),
                Some(RateLimit),
            ),
            (Responses, None, json!({"error":{"code":401}}), Some(Client)),
            (
                ChatCompletions,
                None,
                json!({"error":{"type":"invalid_request_error"}}),
                Some(Client),
            ),
            (
                ChatCompletions,
                None,
                json!({"error":{"code":"opaque","message":"500 server_error"}}),
                Some(Unknown),
            ),
            (
                ChatCompletions,
                None,
                json!({"error":{"code":500,"type":"rate_limit_error"}}),
                Some(Unknown),
            ),
            (
                ChatCompletions,
                None,
                json!({"choices":[{"finish_reason":"error"}]}),
                Some(Unknown),
            ),
            (
                Responses,
                None,
                json!({"status":"failed","error":null}),
                Some(Unknown),
            ),
            (
                Responses,
                None,
                json!({"type":"response.incomplete","response":{"status":"incomplete","incomplete_details":{"reason":"max_output_tokens"}}}),
                None,
            ),
            (
                ChatCompletions,
                None,
                json!({"error":null,"choices":[{"finish_reason":"content_filter"}]}),
                None,
            ),
            (
                ChatCompletions,
                None,
                json!({"choices":[{"finish_reason":"length","message":{"content":"server_error"}}]}),
                None,
            ),
        ] {
            assert_eq!(response_error(endpoint, event, &value), expected, "{value}");
        }
    }
}
