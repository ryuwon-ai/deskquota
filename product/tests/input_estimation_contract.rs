use llmgw::config;

fn config_text(model_fields: &str) -> String {
    format!(
        r#"
listen = "127.0.0.1:4141"
startup_hold_secs = 0
[upstream]
api_base = "http://127.0.0.1:9/v1"
[upstream.auth]
mode = "none"
[quota.rpm]
kind = "unlimited"
[quota.tpm]
kind = "known"
value = 1000
[[models]]
id = "fixture"
{model_fields}
[[roots]]
id = "test"
endpoints = ["chat/completions", "responses", "messages", "models", "messages/count_tokens"]
models = ["fixture"]
"#
    )
}

#[cfg(not(feature = "bpe"))]
#[test]
fn byte_only_build_rejects_bpe_config_for_every_quota_kind() {
    for mode in ["cl100k_base", "o200k_base"] {
        for quota in [
            "kind = \"known\"\nvalue = 1000",
            "kind = \"unknown\"",
            "kind = \"unlimited\"",
        ] {
            let text = config_text(&format!("input_estimator = \"{mode}\""))
                .replace("kind = \"known\"\nvalue = 1000", quota);
            let error = config::parse(text.as_bytes()).unwrap_err().to_string();
            assert!(error.contains(mode), "{error}");
            assert!(error.contains("--features bpe"), "{error}");
        }
    }
}

#[cfg(not(feature = "bpe"))]
#[test]
fn byte_only_build_does_not_fall_back_to_byte_counts() {
    use llmgw::input_estimate::InputEstimator::{Cl100kBase, O200kBase};
    for mode in [Cl100kBase, O200kBase] {
        assert_eq!(mode.estimate("hello world", 0), None);
        assert_eq!(mode.estimate("hello world", 32), None);
    }
}

#[cfg(not(feature = "bpe"))]
#[tokio::test]
async fn byte_only_build_rejects_typed_bpe_before_binding() {
    use llmgw::input_estimate::InputEstimator::{Cl100kBase, O200kBase};
    let occupied = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    for mode in [Cl100kBase, O200kBase] {
        for quota in [
            config::Limit::Known(1000.try_into().unwrap()),
            config::Limit::Unknown,
            config::Limit::Unlimited,
        ] {
            let mut config = config::parse(config_text("").as_bytes()).unwrap();
            config.listen = occupied.local_addr().unwrap();
            config.models[0].input_estimator = mode;
            config.quota.tpm = quota;
            let error = llmgw::server::validate_start_config(&config)
                .unwrap_err()
                .to_string();
            assert!(error.contains(mode.name()), "{error}");
            assert!(error.contains("--features bpe"), "{error}");
            // An occupied port must not mask the unsupported build capability.
            let result = llmgw::server::spawn(
                config,
                llmgw::server::RuntimeCredentials::new(b"control", None).unwrap(),
            )
            .await;
            let error = match result {
                Err(error) => error.to_string(),
                Ok(gateway) => {
                    gateway.shutdown().await.unwrap();
                    panic!("unsupported estimator started a listener");
                }
            };
            assert!(error.contains("--features bpe"), "{error}");
        }
    }
}

#[cfg(feature = "bpe")]
#[test]
fn explicit_bpe_configuration_is_accepted() {
    for mode in ["cl100k_base", "o200k_base"] {
        let text = config_text(&format!(
            "input_estimator = \"{mode}\"\ninput_token_overhead = 32"
        ));
        assert!(config::parse(text.as_bytes()).is_ok());
    }
}

#[test]
fn estimator_defaults_and_invalid_config_are_explicit() {
    use llmgw::input_estimate::InputEstimator;
    let defaults = config::parse(config_text("").as_bytes()).unwrap();
    assert_eq!(
        defaults.models[0].input_estimator,
        InputEstimator::default()
    );
    assert_eq!(
        defaults.models[0].input_token_overhead,
        InputEstimator::default().default_overhead()
    );
    #[cfg(feature = "bpe")]
    for mode in ["cl100k_base", "o200k_base"] {
        let fields = format!("input_estimator = \"{mode}\"");
        let configured = config::parse(config_text(&fields).as_bytes()).unwrap();
        assert_eq!(configured.models[0].input_token_overhead, 32);
        let configured =
            config::parse(config_text(&format!("{fields}\ninput_token_overhead = 0")).as_bytes())
                .unwrap();
        assert_eq!(configured.models[0].input_token_overhead, 0);
    }
    for fields in [
        "input_estimator = \"automatic\"",
        "input_estimator = \"utf8_bytes\"\ninput_token_overhead = 1",
        #[cfg(not(feature = "bpe"))]
        "input_token_overhead = 1",
        "input_estimator = \"cl100k_base\"\ninput_token_overhead = -1",
        "input_estimator = \"cl100k_base\"\ninput_token_overhead = 0.5",
        "input_estimator = \"cl100k_base\"\ninput_token_overhead = \"32\"",
    ] {
        assert!(
            config::parse(config_text(fields).as_bytes()).is_err(),
            "{fields}"
        );
    }
}

#[test]
fn bytes_count_and_invalid_allowance_stay_available() {
    use llmgw::input_estimate::InputEstimator::Utf8Bytes;
    for text in ["hello world", "안녕하세요", "<|endoftext|>"] {
        assert_eq!(Utf8Bytes.estimate(text, 0), Some(text.len() as u64));
    }
    assert_eq!(Utf8Bytes.estimate("hello", 1), None);
}

#[cfg(feature = "bpe")]
#[test]
fn known_encoding_vectors_and_checked_allowance() {
    use llmgw::input_estimate::InputEstimator::{Cl100kBase, O200kBase};
    // Fixed ordinary-text vectors; special-looking text is not a control token.
    for (text, cl100k, o200k) in [
        ("hello world", 2, 2),
        ("안녕하세요", 5, 2),
        ("<|endoftext|>", 7, 7),
    ] {
        assert_eq!(Cl100kBase.estimate(text, 0), Some(cl100k));
        assert_eq!(O200kBase.estimate(text, 0), Some(o200k));
        assert_eq!(Cl100kBase.estimate(text, 32), Some(cl100k + 32));
    }
    assert_eq!(Cl100kBase.estimate("hello", u64::MAX), None);
}

#[cfg(feature = "bpe")]
#[test]
fn normal_build_defaults_to_multilingual_tokenization() {
    let config = config::parse(config_text("").as_bytes()).unwrap();
    assert_eq!(
        config.models[0].input_estimator,
        llmgw::input_estimate::InputEstimator::Cl100kBase
    );
    assert_eq!(config.models[0].input_token_overhead, 32);
}

#[cfg(feature = "bpe")]
#[test]
fn json_formatting_and_unicode_escapes_do_not_change_estimation() {
    use llmgw::input_estimate::InputEstimator::{Cl100kBase, O200kBase};
    let plain = r#"{"model":"fixture","messages":[{"role":"user","content":"안녕 世界 😀"}],"max_tokens":16}"#;
    let escaped = r#"{"model":"fixture","messages":[{"role":"user","content":"\uc548\ub155 \u4e16\u754c \ud83d\ude00"}],"max_tokens":16}"#;
    let padded = format!("{}{}", " ".repeat(460_000), escaped);
    let value: serde_json::Value = serde_json::from_str(plain).unwrap();
    assert_eq!(
        value,
        serde_json::from_str::<serde_json::Value>(escaped).unwrap()
    );
    // Change whitespace only; object order is part of the text being estimated.
    let pretty = plain.replace(',', ",\n  ").replace(':', ": ");
    for mode in [Cl100kBase, O200kBase] {
        let expected = mode.estimate_json(plain, 32).unwrap();
        for body in [escaped, &padded, &pretty] {
            assert_eq!(mode.estimate_json(body, 32), Some(expected));
        }
        assert_eq!(mode.estimate_json("{invalid", 32), None);
        assert_eq!(mode.estimate_json(plain, u64::MAX), None);
    }
}

#[cfg(feature = "bpe")]
#[test]
fn opaque_json_estimation_preserves_deep_values_and_numeric_literals() {
    use llmgw::input_estimate::InputEstimator::{Cl100kBase, O200kBase};
    let deep = format!("{}\"한글\"{}", "[".repeat(10_000), "]".repeat(10_000));
    let numeric = format!(
        "{{\"z\":1e400,\"a\":{},\"z\":-0,\"decimal\":1.2300e+04}}",
        "9".repeat(400)
    );
    for mode in [Cl100kBase, O200kBase] {
        for compact in [&deep, &numeric] {
            assert_eq!(mode.estimate_json(compact, 32), mode.estimate(compact, 32));
        }
        let escaped = r#"["\u005c","\u0022","\ud55c","\ud83d\ude00","\u000a","\u0001","\/"]"#;
        let decoded = r#"["\\","\"","한","😀","\n","\u0001","/"]"#;
        assert_eq!(mode.estimate_json(escaped, 32), mode.estimate(decoded, 32));
        for invalid in [
            &deep[..deep.len() - 1],
            "{} {}",
            r#"{"x":1,}"#,
            r#"{"x":1e+}"#,
            r#"{"x":NaN}"#,
            r#"{"x":'single'}"#,
            r#"{"x":0xff}"#,
            r#"{"x":1/*comment*/}"#,
            r#"{"x":"\ud800"}"#,
            r#"{"x":"\udc00"}"#,
        ] {
            assert_eq!(mode.estimate_json(invalid, 32), None, "{invalid}");
        }
    }
}

#[cfg(feature = "bpe")]
#[tokio::test]
async fn opaque_tool_schemas_do_not_become_quota_errors() {
    let upstream = UpstreamFixture::start(
        b"HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: 2\r\nConnection: close\r\n\r\n{}".to_vec(),
    )
    .await;
    let deep = format!(
        "{}{{\"type\":\"string\"}}{}",
        "{\"properties\":{\"x\":".repeat(65),
        "}}".repeat(65)
    );
    let mut forwarded = 0;
    for mode in ["cl100k_base", "o200k_base"] {
        let mut config =
            config::parse(config_text(&format!("input_estimator = \"{mode}\"")).as_bytes())
                .unwrap();
        config.listen.set_port(0);
        config.quota.tpm = config::Limit::Known(450_000.try_into().unwrap());
        config.upstream.api_base = format!("http://{}/v1", upstream.address()).parse().unwrap();
        let gateway = llmgw::server::spawn(
            config,
            llmgw::server::RuntimeCredentials::new(b"control", None).unwrap(),
        )
        .await
        .unwrap();
        for endpoint in ["messages", "chat/completions", "responses"] {
            for (schema, expected_status) in [
                (deep.as_str(), 200),
                (r#"{"type":"number","maximum":1e400}"#, 200),
                (r#"{"type":"string","pattern":"\ud800"}"#, 400),
                (r#"{"type":"string","pattern":"\udc00"}"#, 400),
            ] {
                let input = if endpoint == "responses" {
                    r#""input":"synthetic","max_output_tokens":16"#
                } else {
                    r#""messages":[{"role":"user","content":"synthetic"}],"max_tokens":16"#
                };
                let tool = match endpoint {
                    "messages" => format!(r#"{{"name":"synthetic","input_schema":{schema}}}"#),
                    "responses" => {
                        format!(r#"{{"type":"function","name":"synthetic","parameters":{schema}}}"#)
                    }
                    _ => format!(
                        r#"{{"type":"function","function":{{"name":"synthetic","parameters":{schema}}}}}"#
                    ),
                };
                let body =
                    format!(r#"{{"model":"fixture",{input},"tools":[{tool}]}}"#).into_bytes();
                let mut request = format!("POST /r/test/v1/{endpoint} HTTP/1.1\r\nHost: localhost\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n", body.len()).into_bytes();
                request.extend_from_slice(&body);
                let response = send_raw(gateway.address(), &request).await;
                assert_eq!(
                    status(&response),
                    expected_status,
                    "{mode} {endpoint}: {}",
                    String::from_utf8_lossy(response_body(&response))
                );
                if expected_status == 200 {
                    forwarded += 1;
                    assert_eq!(upstream.captures(forwarded).await[forwarded - 1].body, body);
                } else {
                    assert!(
                        String::from_utf8_lossy(response_body(&response)).contains("invalid_json")
                    );
                }
            }
        }
        gateway.shutdown().await.unwrap();
    }
    assert_eq!(upstream.attempts(), 12);
}

#[cfg(feature = "bpe")]
#[tokio::test]
async fn long_multilingual_messages_reach_upstream_and_reconcile_usage() {
    let stream = br#"event: message_start
data: {"type":"message_start","message":{"id":"fixture","type":"message","role":"assistant","model":"fixture","content":[],"stop_reason":null,"usage":{"input_tokens":110000,"output_tokens":0}}}

event: message_delta
data: {"type":"message_delta","delta":{"stop_reason":"end_turn"},"usage":{"output_tokens":2}}

event: message_stop
data: {"type":"message_stop"}

"#;
    let mut response = format!("HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\nContent-Length: {}\r\nConnection: close\r\n\r\n", stream.len()).into_bytes();
    response.extend_from_slice(stream);
    let upstream = UpstreamFixture::start(response).await;
    for (index, text) in [
        "The assistant reads the source code and fixes the error. ",
        "한글로 작성한 소스 코드와 대화 기록을 분석합니다. ",
        "这是用于验证多语言请求的中文内容。",
        "日本語の会話履歴とソースコードを確認します。",
        "هذه رسالة لاختبار تقدير الرموز في اللغة العربية. ",
        "😀 🚀 ✅ 🐱 ",
        "fn main() { println!(\"안녕하세요 世界 hello\"); }\n",
    ]
    .into_iter()
    .enumerate()
    {
        let body = serde_json::to_vec(&serde_json::json!({
            "model": "fixture", "stream": true, "max_tokens": 16384,
            "messages": [{"role": "user", "content": text.repeat(460_000 / text.len() + 1)}]
        }))
        .unwrap();
        assert!(body.len() > 450_000);
        // No explicit estimator: this catches regressions in shipped defaults.
        let mut config = config::parse(config_text("").as_bytes()).unwrap();
        config.listen.set_port(0);
        config.quota.tpm = config::Limit::Known(450_000.try_into().unwrap());
        config.upstream.api_base = format!("http://{}/v1", upstream.address()).parse().unwrap();
        let gateway = llmgw::server::spawn(
            config,
            llmgw::server::RuntimeCredentials::new(b"control", None).unwrap(),
        )
        .await
        .unwrap();
        let mut request = format!("POST /r/test/v1/messages HTTP/1.1\r\nHost: localhost\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n", body.len()).into_bytes();
        request.extend_from_slice(&body);
        let response = send_raw(gateway.address(), &request).await;
        assert_eq!(
            status(&response),
            200,
            "language {index}: {}",
            String::from_utf8_lossy(response_body(&response))
        );
        assert_eq!(response_body(&response), stream);
        assert_eq!(upstream.captures(index + 1).await[index].body, body);
        assert_eq!(
            llmgw::server::testing::quota_snapshot(&gateway).tpm_debited,
            110_002
        );
        gateway.shutdown().await.unwrap();
    }
    assert_eq!(upstream.attempts(), 7);
}

mod support;
use support::fixture::{UpstreamFixture, response_body, send_raw, status};

#[tokio::test]
async fn estimator_admission_preserves_body_output_and_stream_usage() {
    let stream = br#"data: {"id":"fixture","object":"chat.completion.chunk","choices":[{"index":0,"delta":{"content":"ok"},"finish_reason":null}]}

data: {"id":"fixture","object":"chat.completion.chunk","choices":[{"index":0,"delta":{},"finish_reason":"stop"}]}

data: {"id":"fixture","object":"chat.completion.chunk","choices":[],"usage":{"prompt_tokens":13,"completion_tokens":2,"total_tokens":15}}

data: [DONE]

"#;
    let mut response = format!("HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\nContent-Length: {}\r\nConnection: close\r\n\r\n", stream.len()).into_bytes();
    response.extend_from_slice(stream);
    let upstream = UpstreamFixture::start(response).await;
    let body = format!(
        r#"{{ "model":"fixture", "messages":[{{"role":"user","content":"{}"}}], "max_completion_tokens":16, "stream":true, "stream_options":{{"include_usage":true}} }}"#,
        "hello world ".repeat(30)
    );
    assert!(body.len() > 200);
    for (mode, expected_status) in [
        ("utf8_bytes", 400),
        #[cfg(feature = "bpe")]
        ("cl100k_base", 200),
    ] {
        let mut config =
            config::parse(config_text(&format!("input_estimator = \"{mode}\"")).as_bytes())
                .unwrap();
        config.listen.set_port(0);
        config.upstream.auth = config::Auth::Forward;
        config.quota.tpm = config::Limit::Known(200.try_into().unwrap());
        config.upstream.api_base = format!("http://{}/v1", upstream.address()).parse().unwrap();
        let gateway = llmgw::server::spawn(
            config,
            llmgw::server::RuntimeCredentials::new(b"control", None).unwrap(),
        )
        .await
        .unwrap();
        let mut request = format!("POST /r/test/v1/chat/completions HTTP/1.1\r\nHost: localhost\r\nContent-Type: application/json\r\nAuthorization: Bearer synthetic\r\nContent-Length: {}\r\nConnection: close\r\n\r\n", body.len()).into_bytes();
        request.extend_from_slice(body.as_bytes());
        let response = send_raw(gateway.address(), &request).await;
        assert_eq!(status(&response), expected_status);
        if expected_status == 200 {
            assert_eq!(response_body(&response), stream);
            let capture = upstream.capture().await;
            assert_eq!(capture.body, body.as_bytes());
            assert_eq!(
                capture.header("authorization"),
                Some(b"Bearer synthetic".as_slice())
            );
            let snapshot = llmgw::server::testing::quota_snapshot(&gateway);
            assert_eq!(snapshot.tpm_debited, 15);
        }
        gateway.shutdown().await.unwrap();
    }
    assert_eq!(upstream.attempts(), usize::from(cfg!(feature = "bpe")));
}
