//! Complete nonstreaming responses share the streaming token/count semantics.
use serde_json::Value;

use super::{ObservedUsage, nested_token, token};
use crate::config::Endpoint;

pub(crate) fn json_usage(endpoint: Endpoint, value: &Value) -> Option<ObservedUsage> {
    if !value.get("error").is_none_or(Value::is_null) {
        return None;
    }
    let (input, output, details) = match endpoint {
        Endpoint::ChatCompletions => {
            let choices = value.get("choices")?.as_array()?;
            if choices.is_empty()
                || !choices.iter().all(|choice| {
                    choice.get("message").is_some_and(Value::is_object)
                        && choice
                            .get("finish_reason")
                            .and_then(Value::as_str)
                            .is_some_and(|reason| {
                                matches!(
                                    reason,
                                    "stop"
                                        | "length"
                                        | "tool_calls"
                                        | "function_call"
                                        | "content_filter"
                                )
                            })
                })
            {
                return None;
            }
            (
                "prompt_tokens",
                "completion_tokens",
                "prompt_tokens_details",
            )
        }
        Endpoint::Responses => {
            if value.get("status")?.as_str()? != "completed"
                || !value.get("incomplete_details").is_none_or(Value::is_null)
            {
                return None;
            }
            ("input_tokens", "output_tokens", "input_tokens_details")
        }
        Endpoint::Messages => {
            if value.get("type")?.as_str()? != "message"
                || value.get("role")?.as_str()? != "assistant"
                || value.get("stop_reason")?.as_str()?.is_empty()
            {
                return None;
            }
            ("input_tokens", "output_tokens", "")
        }
        Endpoint::Models | Endpoint::CountTokens => return None,
    };
    let usage = value.get("usage")?;
    let observed = ObservedUsage {
        input_tokens: token(usage, input).ok()??,
        output_tokens: token(usage, output).ok()??,
        cache_creation_input_tokens: if endpoint == Endpoint::Messages {
            token(usage, "cache_creation_input_tokens")
                .ok()?
                .unwrap_or(0)
        } else {
            0
        },
        cache_read_input_tokens: if endpoint == Endpoint::Messages {
            token(usage, "cache_read_input_tokens").ok()?.unwrap_or(0)
        } else {
            nested_token(usage, details, "cached_tokens")
                .ok()?
                .unwrap_or(0)
        },
    };
    observed.total(endpoint)?;
    Some(observed)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn complete_json_counts_preserve_zero_cache_semantics_and_reject_invalid_reports() {
        for endpoint in [
            Endpoint::ChatCompletions,
            Endpoint::Responses,
            Endpoint::Messages,
        ] {
            let mut value = match endpoint {
                Endpoint::ChatCompletions => {
                    json!({"choices":[{"message":{},"finish_reason":"tool_calls"}],"usage":{"prompt_tokens":0,"completion_tokens":0}})
                }
                Endpoint::Responses => {
                    json!({"status":"completed","output":[{"type":"function_call"}],"usage":{"input_tokens":0,"output_tokens":0}})
                }
                _ => {
                    json!({"type":"message","role":"assistant","stop_reason":"max_tokens","content":[],"usage":{"input_tokens":0,"output_tokens":0}})
                }
            };
            let input = if endpoint == Endpoint::ChatCompletions {
                "prompt_tokens"
            } else {
                "input_tokens"
            };
            let output = if endpoint == Endpoint::ChatCompletions {
                "completion_tokens"
            } else {
                "output_tokens"
            };
            assert_eq!(
                json_usage(endpoint, &value).unwrap().total(endpoint),
                Some(0)
            );
            for bad in [
                json!(-1),
                json!(1.5),
                json!(null),
                json!("1"),
                json!(true),
                serde_json::from_str::<Value>("18446744073709551616").unwrap(),
            ] {
                for field in [input, output] {
                    let mut bad_value = value.clone();
                    bad_value["usage"][field] = bad.clone();
                    assert!(
                        json_usage(endpoint, &bad_value).is_none(),
                        "{endpoint:?}: {bad_value}"
                    );
                }
            }
            value["usage"][input] = json!(u64::MAX);
            value["usage"][output] = json!(1);
            assert!(json_usage(endpoint, &value).is_none(), "sum overflow");
            value["usage"] = json!({"total_tokens":1});
            assert!(
                json_usage(endpoint, &value).is_none(),
                "total alone is unsupported"
            );
        }
        for (endpoint, details, mut value) in [
            (
                Endpoint::ChatCompletions,
                "prompt_tokens_details",
                json!({"choices":[{"message":{},"finish_reason":"length"}],"usage":{"prompt_tokens":3,"completion_tokens":1}}),
            ),
            (
                Endpoint::Responses,
                "input_tokens_details",
                json!({"status":"completed","usage":{"input_tokens":3,"output_tokens":1}}),
            ),
        ] {
            value["usage"][details] = json!({"cached_tokens":3});
            assert_eq!(
                json_usage(endpoint, &value).unwrap().total(endpoint),
                Some(4)
            );
            for bad in [
                json!({"cached_tokens":4}),
                json!({"cached_tokens":-1}),
                json!({"cached_tokens":null}),
                json!("3"),
            ] {
                value["usage"][details] = bad;
                assert!(json_usage(endpoint, &value).is_none());
            }
        }
        let mut message = json!({"type":"message","role":"assistant","stop_reason":"tool_use","usage":{"input_tokens":1,"output_tokens":2,"cache_creation_input_tokens":3,"cache_read_input_tokens":4}});
        assert_eq!(
            json_usage(Endpoint::Messages, &message)
                .unwrap()
                .total(Endpoint::Messages),
            Some(10)
        );
        for field in ["cache_creation_input_tokens", "cache_read_input_tokens"] {
            let original = message["usage"][field].clone();
            for bad in [json!(u64::MAX), json!(null), json!(-1), json!("4")] {
                message["usage"][field] = bad;
                assert!(json_usage(Endpoint::Messages, &message).is_none());
            }
            message["usage"][field] = original;
        }
    }
}
