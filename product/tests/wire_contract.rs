mod support;

use std::fs;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

use llmgw::config::{
    Accounting, Auth, CancelPolicy, Config, Endpoint, Limit, LoadedConfig, Model, Quota, Root,
    Upstream,
};
use llmgw::server::{self, GatewayHandle, RuntimeCredentials};
use support::fixture::{UpstreamFixture, response_body, response_header, send_raw, status};

const DATA_TOKEN: &[u8] = b"synthetic-data-token";
const CONTROL_TOKEN: &[u8] = b"synthetic-control-token";
const JSON: &[u8] = br#"{"model":"fixture-model","messages":[]}"#;
const GZIP_BODY: &[u8] = &[
    0x1f, 0x8b, 0x08, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x03, 0x2b, 0xae, 0xcc, 0x2b, 0xc9, 0x48,
    0x2d, 0xc9, 0x4c, 0x56, 0x48, 0xaf, 0xca, 0x2c, 0x50, 0x48, 0xca, 0x4f, 0xa9, 0x04, 0x00, 0x79,
    0x77, 0xca, 0xee, 0x13, 0x00, 0x00, 0x00,
];
static NEXT_STATE: AtomicU64 = AtomicU64::new(0);

fn upstream_response(status: &str, headers: &[(&str, &str)], body: &[u8]) -> Vec<u8> {
    let mut response =
        format!("HTTP/1.1 {status}\r\nContent-Length: {}\r\n", body.len()).into_bytes();
    for (name, value) in headers {
        response.extend_from_slice(format!("{name}: {value}\r\n").as_bytes());
    }
    response.extend_from_slice(b"Connection: close\r\n\r\n");
    response.extend_from_slice(body);
    response
}

fn config(upstream: &UpstreamFixture, auth: Auth, base_query: bool) -> Config {
    let suffix = if base_query {
        "/team/v1?api-version=2026-09"
    } else {
        "/team/v1"
    };
    Config {
        listen: "127.0.0.1:0".parse().expect("fixture listen"),
        concurrency: 1,
        cancel_policy: CancelPolicy::Drain,
        accounting: Accounting::Reserved,
        retry_transient_429: false,
        upstream: Upstream {
            api_base: format!("http://{}{suffix}", upstream.address())
                .parse()
                .expect("fixture URL"),
            auth,
            proxy: None,
            ca_bundle: None,
        },
        quota: Quota {
            rpm: Limit::Unlimited,
            tpm: Limit::Unknown,
        },
        models: vec![Model {
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

fn constructed_config(listen: &str) -> Config {
    Config {
        listen: listen.parse().expect("fixture listen"),
        concurrency: 1,
        cancel_policy: CancelPolicy::Drain,
        accounting: Accounting::Reserved,
        retry_transient_429: false,
        upstream: Upstream {
            api_base: "http://127.0.0.1:9/team/v1".parse().expect("fixture URL"),
            auth: Auth::None,
            proxy: None,
            ca_bundle: None,
        },
        quota: Quota {
            rpm: Limit::Unlimited,
            tpm: Limit::Unknown,
        },
        models: vec![Model {
            id: "fixture-model".to_owned(),
            max_output_tokens: None,
        }],
        roots: vec![Root {
            id: "pi-work".to_owned(),
            endpoints: vec![Endpoint::ChatCompletions],
            models: vec!["fixture-model".to_owned()],
        }],
    }
}

async fn gateway(
    upstream: &UpstreamFixture,
    auth: Auth,
    env: Option<&[u8]>,
    base_query: bool,
) -> GatewayHandle {
    let credentials =
        RuntimeCredentials::new(DATA_TOKEN, CONTROL_TOKEN, env).expect("fixture credentials");
    server::spawn(config(upstream, auth, base_query), credentials)
        .await
        .expect("start gateway")
}

fn post(path: &str, extra_headers: &[(&str, &str)], body: &[u8]) -> Vec<u8> {
    let mut request = format!(
        "POST {path} HTTP/1.1\r\nHost: localhost\r\nX-LLMGW-Token: {}\r\nContent-Length: {}\r\nConnection: close\r\n",
        std::str::from_utf8(DATA_TOKEN).expect("fixture token"),
        body.len()
    ).into_bytes();
    for (name, value) in extra_headers {
        request.extend_from_slice(format!("{name}: {value}\r\n").as_bytes());
    }
    request.extend_from_slice(b"\r\n");
    request.extend_from_slice(body);
    request
}

#[tokio::test]
async fn prefix_is_preserved() {
    let upstream = UpstreamFixture::start(upstream_response("200 OK", &[], b"{}")).await;
    let gateway = gateway(&upstream, Auth::None, None, true).await;
    let response = send_raw(
        gateway.address(),
        &post("/r/pi-work/v1/chat/completions?trace=%ED%95%9C", &[], JSON),
    )
    .await;
    assert_eq!(status(&response), 200);
    assert_eq!(
        upstream.capture().await.target,
        "/team/v1/chat/completions?api-version=2026-09&trace=%ED%95%9C"
    );
    gateway.shutdown().await.expect("shutdown gateway");
}

#[tokio::test]
async fn lossless_query_rejects_a_request_query_that_url_would_transform() {
    let upstream = UpstreamFixture::start(upstream_response("200 OK", &[], b"{}")).await;
    let gateway = gateway(&upstream, Auth::None, None, false).await;
    let response = send_raw(
        gateway.address(),
        &post("/r/pi-work/v1/chat/completions?trace=a'b", &[], JSON),
    )
    .await;

    assert_eq!(status(&response), 400);
    assert_eq!(upstream.attempts(), 0);
    gateway.shutdown().await.expect("shutdown gateway");
}

#[tokio::test]
async fn supported_encoded_queries_are_forwarded_without_reserialization() {
    let upstream = UpstreamFixture::start(upstream_response("200 OK", &[], b"{}")).await;
    let gateway = gateway(&upstream, Auth::None, None, false).await;
    let query = "apostrophe=a%27b&plus=a+b&lowercase=a%2bb";
    let response = send_raw(
        gateway.address(),
        &post(
            &format!("/r/pi-work/v1/chat/completions?{query}"),
            &[],
            JSON,
        ),
    )
    .await;

    assert_eq!(status(&response), 200);
    assert_eq!(
        upstream.capture().await.target,
        format!("/team/v1/chat/completions?{query}")
    );
    gateway.shutdown().await.expect("shutdown gateway");
}

#[tokio::test]
async fn empty_query_presence_is_rejected_instead_of_silently_dropped() {
    let upstream = UpstreamFixture::start(upstream_response("200 OK", &[], b"{}")).await;
    let gateway = gateway(&upstream, Auth::None, None, false).await;
    let response = send_raw(
        gateway.address(),
        &post("/r/pi-work/v1/chat/completions?", &[], JSON),
    )
    .await;

    assert_eq!(status(&response), 400);
    assert_eq!(upstream.attempts(), 0);
    gateway.shutdown().await.expect("shutdown gateway");
}

#[tokio::test]
async fn gzip_response_status_headers_and_compressed_bytes_are_unchanged() {
    let length = GZIP_BODY.len().to_string();
    let upstream = UpstreamFixture::start(upstream_response(
        "201 Created",
        &[("Content-Encoding", "gzip"), ("X-Synthetic", "kept")],
        GZIP_BODY,
    ))
    .await;
    let gateway = gateway(&upstream, Auth::None, None, false).await;
    let response = send_raw(
        gateway.address(),
        &post("/r/pi-work/v1/chat/completions", &[], JSON),
    )
    .await;

    assert_eq!(status(&response), 201);
    assert_eq!(
        response_header(&response, "content-encoding"),
        Some(b"gzip".as_slice())
    );
    assert_eq!(
        response_header(&response, "content-length"),
        Some(length.as_bytes())
    );
    assert_eq!(
        response_header(&response, "x-synthetic"),
        Some(b"kept".as_slice())
    );
    assert_eq!(response_body(&response), GZIP_BODY);
    gateway.shutdown().await.expect("shutdown gateway");
}

#[tokio::test]
async fn body_is_unchanged() {
    let upstream = UpstreamFixture::start(upstream_response("200 OK", &[], b"{}")).await;
    let gateway = gateway(&upstream, Auth::None, None, false).await;
    let body = b"{ \n  \"model\": \"fixture-model\", \"unknown\": \"\xED\x95\x9C\xEA\xB8\x80\" }";
    let response = send_raw(
        gateway.address(),
        &post(
            "/r/pi-work/v1/responses",
            &[
                ("Content-Type", "application/json"),
                ("Content-Encoding", "identity"),
            ],
            body,
        ),
    )
    .await;
    assert_eq!(status(&response), 200);
    let capture = upstream.capture().await;
    assert_eq!(capture.body, body);
    assert_eq!(
        capture.header("content-encoding"),
        Some(b"identity".as_slice())
    );
    gateway.shutdown().await.expect("shutdown gateway");
}

#[tokio::test]
async fn duplicate_top_level_inspected_fields_are_rejected_before_upstream() {
    let cases: &[(&str, &[u8])] = &[
        (
            "/r/pi-work/v1/chat/completions",
            br#"{"model":"not-allowed","model":"fixture-model"}"#,
        ),
        (
            "/r/pi-work/v1/chat/completions",
            br#"{"mo\u0064el":"not-allowed","model":"fixture-model"}"#,
        ),
        (
            "/r/pi-work/v1/responses",
            br#"{"model":"fixture-model","background":true,"background":false}"#,
        ),
        (
            "/r/pi-work/v1/responses",
            br#"{"model":"fixture-model","backgrou\u006ed":true,"background":false}"#,
        ),
    ];

    for (path, body) in cases {
        let upstream = UpstreamFixture::start(upstream_response("200 OK", &[], b"{}")).await;
        let gateway = gateway(&upstream, Auth::None, None, false).await;
        let response = send_raw(gateway.address(), &post(path, &[], body)).await;

        assert_eq!(status(&response), 400, "body: {body:?}");
        assert_eq!(
            response_body(&response),
            br#"{"error":{"code":"duplicate_inspected_field"}}"#,
            "body: {body:?}"
        );
        assert_eq!(upstream.attempts(), 0, "body: {body:?}");
        gateway.shutdown().await.expect("shutdown gateway");
    }
}

#[tokio::test]
async fn invalid_utf8_in_ignored_json_values_is_rejected_before_upstream() {
    let cases: &[&[u8]] = &[
        b"{\"model\":\"fixture-model\",\"unknown\":\"\xff\"}",
        b"{\"model\":\"fixture-model\",\"unknown\":{\"\xff\":\"value\"}}",
    ];

    for body in cases {
        let upstream = UpstreamFixture::start(upstream_response("200 OK", &[], b"{}")).await;
        let gateway = gateway(&upstream, Auth::None, None, false).await;
        let response = send_raw(
            gateway.address(),
            &post("/r/pi-work/v1/chat/completions", &[], body),
        )
        .await;

        assert_eq!(status(&response), 400, "body: {body:?}");
        assert_eq!(
            response_body(&response),
            br#"{"error":{"code":"invalid_json"}}"#,
            "body: {body:?}"
        );
        assert_eq!(upstream.attempts(), 0, "body: {body:?}");
        gateway.shutdown().await.expect("shutdown gateway");
    }
}

#[tokio::test]
async fn no_cross_host_redirect() {
    let destination = UpstreamFixture::start(upstream_response("200 OK", &[], b"followed")).await;
    let location = format!("http://{}/stolen", destination.address());
    let source = UpstreamFixture::start(upstream_response(
        "302 Found",
        &[("Location", &location)],
        b"redirect",
    ))
    .await;
    let gateway = gateway(&source, Auth::None, None, false).await;
    let response = send_raw(
        gateway.address(),
        &post("/r/pi-work/v1/chat/completions", &[], JSON),
    )
    .await;
    assert_eq!(status(&response), 302);
    assert_eq!(source.attempts(), 1);
    tokio::time::sleep(Duration::from_millis(50)).await;
    assert_eq!(destination.attempts(), 0);
    gateway.shutdown().await.expect("shutdown gateway");
}

#[tokio::test]
async fn env_auth_removes_both_old_credentials() {
    let upstream = UpstreamFixture::start(upstream_response("200 OK", &[], b"{}")).await;
    let gateway = gateway(
        &upstream,
        Auth::Env {
            header: "Authorization".to_owned(),
            name: "SYNTHETIC_TOKEN".to_owned(),
        },
        Some(b"Bearer injected-synthetic"),
        false,
    )
    .await;
    let response = send_raw(
        gateway.address(),
        &post(
            "/r/pi-work/v1/chat/completions",
            &[("Authorization", "Bearer old"), ("x-api-key", "old-key")],
            JSON,
        ),
    )
    .await;
    assert_eq!(status(&response), 200);
    let capture = upstream.capture().await;
    assert_eq!(
        capture.header("authorization"),
        Some(b"Bearer injected-synthetic".as_slice())
    );
    assert_eq!(capture.header("x-api-key"), None);
    gateway.shutdown().await.expect("shutdown gateway");
}

#[tokio::test]
async fn data_token_is_stripped() {
    let upstream = UpstreamFixture::start(upstream_response("200 OK", &[], b"{}")).await;
    let gateway = gateway(&upstream, Auth::None, None, false).await;
    assert_eq!(
        status(
            &send_raw(
                gateway.address(),
                &post("/r/pi-work/v1/chat/completions", &[], JSON)
            )
            .await
        ),
        200
    );
    let capture = upstream.capture().await;
    assert_eq!(capture.header("x-llmgw-token"), None);
    assert_eq!(capture.header("x-llmgw-control-token"), None);
    gateway.shutdown().await.expect("shutdown gateway");
}

#[tokio::test]
async fn control_rejects_data_token() {
    let upstream = UpstreamFixture::start(upstream_response("200 OK", &[], b"{}")).await;
    let gateway = gateway(&upstream, Auth::None, None, false).await;
    let request = format!(
        "GET /_llmgw/status HTTP/1.1\r\nHost: localhost\r\nX-LLMGW-Token: {}\r\nConnection: close\r\n\r\n",
        std::str::from_utf8(DATA_TOKEN).expect("fixture token")
    );
    assert_eq!(
        status(&send_raw(gateway.address(), request.as_bytes()).await),
        401
    );
    assert_eq!(upstream.attempts(), 0);
    gateway.shutdown().await.expect("shutdown gateway");
}

#[tokio::test]
async fn control_rejects_origin() {
    let upstream = UpstreamFixture::start(upstream_response("200 OK", &[], b"{}")).await;
    let gateway = gateway(&upstream, Auth::None, None, false).await;
    let request = format!(
        "GET /_llmgw/health HTTP/1.1\r\nHost: localhost\r\nX-LLMGW-Control-Token: {}\r\nOrigin: https://browser.example\r\nConnection: close\r\n\r\n",
        std::str::from_utf8(CONTROL_TOKEN).expect("fixture token")
    );
    assert_eq!(
        status(&send_raw(gateway.address(), request.as_bytes()).await),
        403
    );
    assert_eq!(upstream.attempts(), 0);
    gateway.shutdown().await.expect("shutdown gateway");
}

#[tokio::test]
async fn forward_auth_is_unchanged() {
    let upstream = UpstreamFixture::start(upstream_response("200 OK", &[], b"{}")).await;
    let gateway = gateway(&upstream, Auth::Forward, None, false).await;
    let response = send_raw(
        gateway.address(),
        &post(
            "/r/pi-work/v1/messages",
            &[
                ("Authorization", "Bearer original"),
                ("x-api-key", "original-key"),
            ],
            JSON,
        ),
    )
    .await;
    assert_eq!(status(&response), 200);
    let capture = upstream.capture().await;
    assert_eq!(
        capture.header("authorization"),
        Some(b"Bearer original".as_slice())
    );
    assert_eq!(
        capture.header("x-api-key"),
        Some(b"original-key".as_slice())
    );
    gateway.shutdown().await.expect("shutdown gateway");
}

#[tokio::test]
async fn query_conflict_is_rejected() {
    let upstream = UpstreamFixture::start(upstream_response("200 OK", &[], b"{}")).await;
    let gateway = gateway(&upstream, Auth::None, None, true).await;
    let response = send_raw(
        gateway.address(),
        &post("/r/pi-work/v1/responses?api-version=override", &[], JSON),
    )
    .await;
    assert_eq!(status(&response), 400);
    assert_eq!(upstream.attempts(), 0);
    gateway.shutdown().await.expect("shutdown gateway");
}

#[tokio::test]
async fn unknown_route_never_reaches_upstream() {
    let upstream = UpstreamFixture::start(upstream_response("200 OK", &[], b"{}")).await;
    let gateway = gateway(&upstream, Auth::None, None, false).await;
    for path in [
        "/r/not-configured/v1/chat/completions",
        "/r/pi-work/v1/unknown",
        "/r/pi%2Dwork/v1/chat/completions",
    ] {
        assert_eq!(
            status(&send_raw(gateway.address(), &post(path, &[], JSON)).await),
            404,
            "path {path}"
        );
    }
    assert_eq!(upstream.attempts(), 0);
    gateway.shutdown().await.expect("shutdown gateway");
}

#[tokio::test]
async fn duplicate_data_credentials_are_rejected() {
    let upstream = UpstreamFixture::start(upstream_response("200 OK", &[], b"{}")).await;
    let gateway = gateway(&upstream, Auth::None, None, false).await;
    let response = send_raw(
        gateway.address(),
        &post(
            "/r/pi-work/v1/chat/completions",
            &[("X-LLMGW-Token", "second-token-must-not-be-ignored")],
            JSON,
        ),
    )
    .await;
    assert_eq!(status(&response), 401);
    assert_eq!(upstream.attempts(), 0);
    gateway.shutdown().await.expect("shutdown gateway");
}

#[tokio::test]
async fn none_auth_strips_both_known_credentials() {
    let upstream = UpstreamFixture::start(upstream_response("200 OK", &[], b"{}")).await;
    let gateway = gateway(&upstream, Auth::None, None, false).await;
    let response = send_raw(
        gateway.address(),
        &post(
            "/r/pi-work/v1/chat/completions",
            &[("Authorization", "Bearer old"), ("x-api-key", "old-key")],
            JSON,
        ),
    )
    .await;
    assert_eq!(status(&response), 200);
    let capture = upstream.capture().await;
    assert_eq!(capture.header("authorization"), None);
    assert_eq!(capture.header("x-api-key"), None);
    gateway.shutdown().await.expect("shutdown gateway");
}

#[tokio::test]
async fn hop_by_hop_and_connection_nominated_headers_are_scrubbed_both_directions() {
    let upstream = UpstreamFixture::start(upstream_response(
        "200 OK",
        &[
            ("Connection", "x-response-remove"),
            ("X-Response-Remove", "secret-hop"),
            ("X-Response-Keep", "kept"),
        ],
        b"wire-response",
    ))
    .await;
    let gateway = gateway(&upstream, Auth::Forward, None, false).await;
    let response = send_raw(
        gateway.address(),
        &post(
            "/r/pi-work/v1/chat/completions",
            &[
                ("Connection", "x-request-remove"),
                ("X-Request-Remove", "secret-hop"),
                ("X-Request-Keep", "kept"),
            ],
            JSON,
        ),
    )
    .await;
    assert_eq!(status(&response), 200);
    let capture = upstream.capture().await;
    assert_eq!(capture.header("x-request-remove"), None);
    assert_eq!(capture.header("x-request-keep"), Some(b"kept".as_slice()));
    assert_eq!(
        capture.header("host"),
        Some(upstream.address().to_string().as_bytes())
    );
    assert_eq!(response_header(&response, "x-response-remove"), None);
    assert_eq!(
        response_header(&response, "x-response-keep"),
        Some(b"kept".as_slice())
    );
    assert_eq!(response_body(&response), b"wire-response");
    gateway.shutdown().await.expect("shutdown gateway");
}

#[tokio::test]
async fn unsupported_background_and_upgrade_never_reach_upstream() {
    let upstream = UpstreamFixture::start(upstream_response("200 OK", &[], b"{}")).await;
    let gateway = gateway(&upstream, Auth::None, None, false).await;
    let background = br#"{"model":"fixture-model","background":true,"unknown":"kept"}"#;
    assert_eq!(
        status(
            &send_raw(
                gateway.address(),
                &post("/r/pi-work/v1/responses", &[], background),
            )
            .await,
        ),
        400
    );
    assert_eq!(
        status(
            &send_raw(
                gateway.address(),
                &post("/r/pi-work/v1/responses", &[("Upgrade", "websocket")], JSON,),
            )
            .await,
        ),
        400
    );
    assert_eq!(upstream.attempts(), 0);
    gateway.shutdown().await.expect("shutdown gateway");
}

#[tokio::test]
async fn missing_token_is_rejected_before_body_read() {
    let upstream = UpstreamFixture::start(upstream_response("200 OK", &[], b"{}")).await;
    let gateway = gateway(&upstream, Auth::None, None, false).await;
    let request = b"POST /r/pi-work/v1/chat/completions HTTP/1.1\r\nHost: localhost\r\nContent-Length: 1048576\r\nConnection: close\r\n\r\n";
    assert_eq!(status(&send_raw(gateway.address(), request).await), 401);
    assert_eq!(upstream.attempts(), 0);
    gateway.shutdown().await.expect("shutdown gateway");
}

#[tokio::test]
async fn model_allowlist_and_body_limit_are_enforced_before_upstream() {
    let upstream = UpstreamFixture::start(upstream_response("200 OK", &[], b"{}")).await;
    let gateway = gateway(&upstream, Auth::None, None, false).await;
    let denied = br#"{"model":"not-allowed","messages":[]}"#;
    assert_eq!(
        status(
            &send_raw(
                gateway.address(),
                &post("/r/pi-work/v1/chat/completions", &[], denied),
            )
            .await,
        ),
        400
    );
    let request = format!(
        "POST /r/pi-work/v1/chat/completions HTTP/1.1\r\nHost: localhost\r\nX-LLMGW-Token: {}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
        std::str::from_utf8(DATA_TOKEN).expect("fixture token"),
        8 * 1024 * 1024 + 1
    );
    assert_eq!(
        status(&send_raw(gateway.address(), request.as_bytes()).await),
        413
    );
    assert_eq!(upstream.attempts(), 0);
    gateway.shutdown().await.expect("shutdown gateway");
}

#[tokio::test]
async fn models_metadata_does_not_require_a_generation_body() {
    let upstream = UpstreamFixture::start(upstream_response("200 OK", &[], b"models")).await;
    let gateway = gateway(&upstream, Auth::None, None, false).await;
    let request = format!(
        "GET /r/pi-work/v1/models HTTP/1.1\r\nHost: localhost\r\nX-LLMGW-Token: {}\r\nConnection: close\r\n\r\n",
        std::str::from_utf8(DATA_TOKEN).expect("fixture token")
    );
    let response = send_raw(gateway.address(), request.as_bytes()).await;
    assert_eq!(status(&response), 200);
    assert_eq!(response_body(&response), b"models");
    assert_eq!(upstream.capture().await.target, "/team/v1/models");
    gateway.shutdown().await.expect("shutdown gateway");
}

#[tokio::test]
async fn library_retry_is_disabled_on_response_failure() {
    let upstream = UpstreamFixture::start(Vec::new()).await;
    let gateway = gateway(&upstream, Auth::None, None, false).await;
    let response = send_raw(
        gateway.address(),
        &post("/r/pi-work/v1/chat/completions", &[], JSON),
    )
    .await;
    assert_eq!(status(&response), 502);
    tokio::time::sleep(Duration::from_millis(50)).await;
    assert_eq!(upstream.attempts(), 1);
    gateway.shutdown().await.expect("shutdown gateway");
}

#[tokio::test]
async fn many_small_chunked_frames_preserve_body_bytes() {
    let upstream = UpstreamFixture::start(upstream_response("200 OK", &[], b"{}")).await;
    let gateway = gateway(&upstream, Auth::None, None, false).await;
    let body = br#"{"model":"fixture-model","unknown":"small-frames"}"#;
    let mut request = format!(
        "POST /r/pi-work/v1/chat/completions HTTP/1.1\r\nHost: localhost\r\nX-LLMGW-Token: {}\r\nTransfer-Encoding: chunked\r\nConnection: close\r\n\r\n",
        std::str::from_utf8(DATA_TOKEN).expect("fixture token")
    )
    .into_bytes();
    for byte in body {
        request.extend_from_slice(b"1\r\n");
        request.push(*byte);
        request.extend_from_slice(b"\r\n");
    }
    request.extend_from_slice(b"0\r\n\r\n");
    assert_eq!(status(&send_raw(gateway.address(), &request).await), 200);
    let capture = upstream.capture().await;
    assert_eq!(capture.body, body);
    assert_eq!(capture.header("transfer-encoding"), None);
    assert_eq!(
        capture.header("content-length"),
        Some(body.len().to_string().as_bytes())
    );
    gateway.shutdown().await.expect("shutdown gateway");
}

#[tokio::test]
async fn reserved_env_auth_header_is_rejected_before_any_http_attempt() {
    let upstream = UpstreamFixture::start(upstream_response("200 OK", &[], b"{}")).await;
    let credentials =
        RuntimeCredentials::new(DATA_TOKEN, CONTROL_TOKEN, Some(b"reserved-header-secret"))
            .expect("fixture credentials");
    let result = server::spawn(
        config(
            &upstream,
            Auth::Env {
                header: "Host".to_owned(),
                name: "SYNTHETIC_TOKEN".to_owned(),
            },
            false,
        ),
        credentials,
    )
    .await;
    let error = match result {
        Ok(handle) => {
            handle
                .shutdown()
                .await
                .expect("shutdown unexpected gateway");
            panic!("reserved auth header must reject startup")
        }
        Err(error) => error,
    };
    assert!(error.to_string().contains("reserved"));
    assert!(!error.to_string().contains("reserved-header-secret"));
    assert_eq!(upstream.attempts(), 0);
}

#[test]
fn start_validation_rejects_a_constructed_non_loopback_config_before_bind() {
    let error = server::validate_start_config(&constructed_config("0.0.0.0:0"))
        .expect_err("constructed non-loopback listen must reject startup");

    assert_eq!(
        error.to_string(),
        "configured listen address must be loopback"
    );
}

#[tokio::test]
async fn public_spawn_rejects_non_loopback_before_other_startup_work() {
    let credentials = RuntimeCredentials::new(
        DATA_TOKEN,
        CONTROL_TOKEN,
        Some(b"synthetic-unused-upstream-token"),
    )
    .expect("synthetic credentials");
    let error = match server::spawn(constructed_config("0.0.0.0:0"), credentials).await {
        Ok(handle) => {
            handle
                .shutdown()
                .await
                .expect("shutdown unexpected gateway");
            panic!("public spawn must reject non-loopback before binding")
        }
        Err(error) => error,
    };

    assert!(matches!(error, server::StartError::NonLoopbackListen));
}

#[tokio::test]
async fn local_control_reports_bounded_metrics_and_stops_without_upstream() {
    let upstream = UpstreamFixture::start(upstream_response("200 OK", &[], b"{}")).await;
    let gateway = gateway(&upstream, Auth::None, None, false).await;
    assert_eq!(
        status(
            &send_raw(
                gateway.address(),
                &post("/r/pi-work/v1/chat/completions", &[], JSON),
            )
            .await,
        ),
        200
    );
    let control = |method: &str, path: &str| {
        format!(
            "{method} {path} HTTP/1.1\r\nHost: localhost\r\nX-LLMGW-Control-Token: {}\r\nConnection: close\r\n\r\n",
            std::str::from_utf8(CONTROL_TOKEN).expect("fixture token")
        )
    };
    let status_response = send_raw(
        gateway.address(),
        control("GET", "/_llmgw/status").as_bytes(),
    )
    .await;
    assert_eq!(status(&status_response), 200);
    assert!(response_body(&status_response).starts_with(b"{\"requests\":"));
    assert!(
        response_body(&status_response)
            .windows(21)
            .any(|window| window == b"\"upstream_attempts\":1")
    );
    assert!(
        !response_body(&status_response)
            .windows(DATA_TOKEN.len())
            .any(|window| window == DATA_TOKEN)
    );
    let stop_response = send_raw(
        gateway.address(),
        control("POST", "/_llmgw/stop").as_bytes(),
    )
    .await;
    assert_eq!(status(&stop_response), 200);
    assert_eq!(upstream.attempts(), 1);
    gateway.shutdown().await.expect("join stopped gateway");
}

#[tokio::test]
async fn oversized_header_is_bounded_by_the_actual_http_receiver() {
    let upstream = UpstreamFixture::start(upstream_response("200 OK", &[], b"{}")).await;
    let gateway = gateway(&upstream, Auth::None, None, false).await;
    let request = format!(
        "GET /r/pi-work/v1/models HTTP/1.1\r\nHost: localhost\r\nX-LLMGW-Token: {}\r\nX-Oversized: {}\r\nConnection: close\r\n\r\n",
        std::str::from_utf8(DATA_TOKEN).expect("fixture token"),
        "x".repeat(33 * 1024)
    );
    let response = send_raw(gateway.address(), request.as_bytes()).await;
    assert_eq!(status(&response), 431);
    assert_eq!(upstream.attempts(), 0);
    gateway.shutdown().await.expect("shutdown gateway");
}

#[test]
fn runtime_credentials_require_protected_preprovisioned_state_files() {
    let sequence = NEXT_STATE.fetch_add(1, Ordering::Relaxed);
    let directory = std::env::temp_dir().join(format!(
        "llmgw-task2-runtime-{}-{sequence}",
        std::process::id()
    ));
    fs::create_dir_all(&directory).expect("create temporary runtime fixture");
    let config_path = directory.join("gateway.toml");
    fs::write(
        &config_path,
        r#"
listen = "127.0.0.1:0"
[upstream]
api_base = "http://127.0.0.1:9/team/v1"
[upstream.auth]
mode = "none"
[quota.rpm]
kind = "unlimited"
[quota.tpm]
kind = "unknown"
[[models]]
id = "fixture-model"
[[roots]]
id = "pi-work"
endpoints = ["chat/completions"]
models = ["fixture-model"]
"#,
    )
    .expect("write temporary config");
    let loaded = LoadedConfig::load(&config_path).expect("load temporary config");
    let missing = match RuntimeCredentials::load(&loaded) {
        Ok(_) => panic!("missing state credentials must reject startup"),
        Err(error) => error,
    };
    assert!(missing.to_string().contains("data-token"));

    fs::create_dir_all(&loaded.state_paths.directory).expect("create temporary state directory");
    let data_secret = b"only-in-temporary-data-file";
    let control_secret = b"only-in-temporary-control-file";
    fs::write(&loaded.state_paths.data_token, data_secret).expect("write temporary data token");
    fs::write(&loaded.state_paths.control_token, control_secret)
        .expect("write temporary control token");
    protect(&loaded.state_paths.data_token);
    protect(&loaded.state_paths.control_token);
    RuntimeCredentials::load(&loaded).expect("protected distinct token files load");

    fs::write(&loaded.state_paths.control_token, data_secret)
        .expect("replace temporary control token");
    protect(&loaded.state_paths.control_token);
    let duplicate = match RuntimeCredentials::load(&loaded) {
        Ok(_) => panic!("duplicate state credentials must reject startup"),
        Err(error) => error,
    };
    let rendered = format!("{duplicate}\n{duplicate:?}");
    assert!(rendered.contains("distinct"));
    assert!(!rendered.contains(std::str::from_utf8(data_secret).expect("fixture secret")));
    fs::remove_dir_all(directory).expect("remove temporary runtime fixture");
}

#[cfg(unix)]
fn protect(path: &std::path::Path) {
    use std::os::unix::fs::PermissionsExt;
    fs::set_permissions(path, fs::Permissions::from_mode(0o600))
        .expect("protect temporary token file");
}

#[cfg(not(unix))]
fn protect(_path: &std::path::Path) {}

#[tokio::test]
async fn known_tpm_rejects_missing_output_bound_before_upstream() {
    let upstream = UpstreamFixture::start(upstream_response("200 OK", &[], b"{}")).await;
    let mut config = config(&upstream, Auth::None, false);
    config.quota.tpm = Limit::Known(10000.try_into().unwrap());
    let gateway = server::spawn(
        config,
        RuntimeCredentials::new(DATA_TOKEN, CONTROL_TOKEN, None).unwrap(),
    )
    .await
    .unwrap();
    let response = send_raw(
        gateway.address(),
        &post("/r/pi-work/v1/chat/completions", &[], JSON),
    )
    .await;
    assert_eq!(status(&response), 400);
    assert!(
        response_body(&response)
            .windows(b"output_bound_required".len())
            .any(|w| w == b"output_bound_required")
    );
    let error: serde_json::Value = serde_json::from_slice(response_body(&response)).unwrap();
    assert!(
        error["error"]["message"]
            .as_str()
            .is_some_and(
                |message| message.contains("request output cap") && message.contains("model")
            )
    );
    assert_eq!(upstream.attempts(), 0);
    gateway.shutdown().await.unwrap();
}

#[tokio::test]
async fn known_tpm_rejects_multimodal_and_oversized_estimate_without_attempt() {
    for (endpoint, body, code) in [
        (
            "chat/completions",
            r#"{"model":"fixture-model","max_tokens":10,"messages":[{"content":[{"type":"image_url","image_url":{"url":"synthetic"}}]}]}"#,
            "unsupported_multimodal_estimate",
        ),
        (
            "responses",
            r#"{"model":"fixture-model","max_output_tokens":10,"input":[{"content":[{"type":"input_file","file_id":"synthetic"}]}]}"#,
            "unsupported_multimodal_estimate",
        ),
        (
            "messages",
            r#"{"model":"fixture-model","max_tokens":10,"messages":[{"content":[{"type":"image","source":{}}]}]}"#,
            "unsupported_multimodal_estimate",
        ),
        (
            "chat/completions",
            r#"{"model":"fixture-model","max_tokens":999999,"messages":[]}"#,
            "estimate_exceeds_budget",
        ),
        (
            "chat/completions",
            r#"{"model":"fixture-model","max_tokens":10,"max_completion_tokens":20,"messages":[]}"#,
            "ambiguous_output_bound",
        ),
        (
            "chat/completions",
            r#"{"model":"fixture-model","max_tokens":"10","messages":[]}"#,
            "invalid_output_bound",
        ),
        (
            "chat/completions",
            r#"{"model":"fixture-model","max_tokens":10,"max_to\u006bens":10,"messages":[]}"#,
            "duplicate_inspected_field",
        ),
    ] {
        let upstream = UpstreamFixture::start(upstream_response("200 OK", &[], b"{}")).await;
        let mut config = config(&upstream, Auth::None, false);
        config.quota.tpm = Limit::Known(10000.try_into().unwrap());
        let gateway = server::spawn(
            config,
            RuntimeCredentials::new(DATA_TOKEN, CONTROL_TOKEN, None).unwrap(),
        )
        .await
        .unwrap();
        let response = send_raw(
            gateway.address(),
            &post(&format!("/r/pi-work/v1/{endpoint}"), &[], body.as_bytes()),
        )
        .await;
        assert_eq!(status(&response), 400, "{code}");
        assert!(
            response_body(&response)
                .windows(code.len())
                .any(|w| w == code.as_bytes()),
            "expected {code}"
        );
        assert_eq!(upstream.attempts(), 0);
        gateway.shutdown().await.unwrap();
    }
}

#[tokio::test]
async fn known_quota_production_startup_holds_before_any_http_attempt() {
    let upstream = UpstreamFixture::start(upstream_response("200 OK", &[], b"{}")).await;
    let mut config = config(&upstream, Auth::None, false);
    config.quota.rpm = Limit::Known(1.try_into().unwrap());
    let gateway = server::spawn(
        config,
        RuntimeCredentials::new(DATA_TOKEN, CONTROL_TOKEN, None).unwrap(),
    )
    .await
    .unwrap();
    let request = post("/r/pi-work/v1/chat/completions", &[], JSON);
    assert!(
        tokio::time::timeout(
            Duration::from_millis(30),
            send_raw(gateway.address(), &request)
        )
        .await
        .is_err()
    );
    assert_eq!(upstream.attempts(), 0);
    gateway.shutdown().await.unwrap();
}

async fn wait_quota(
    gateway: &GatewayHandle,
    predicate: impl Fn(&llmgw::admission::quota::Snapshot) -> bool,
) -> llmgw::admission::quota::Snapshot {
    tokio::time::timeout(Duration::from_secs(2), async {
        loop {
            let snapshot = server::testing::quota_snapshot(gateway);
            if predicate(&snapshot) {
                return snapshot;
            }
            tokio::task::yield_now().await;
        }
    })
    .await
    .expect("quota state within fixture deadline")
}

#[tokio::test]
async fn known_tpm_explicit_caps_and_text_only_inputs_forward_exact_bytes() {
    for (endpoint, body) in [
        (
            "chat/completions",
            r#" {"model":"fixture-model","max_tokens":10,"messages":[{"content":"image base64 한글"}],"opaque":{"type":"image"},"tools":[{"function":{"parameters":{"type":"image"}}}]} "#,
        ),
        (
            "chat/completions",
            r#"{"model":"fixture-model","max_completion_tokens":10,"messages":[{"content":[{"type":"text","text":"base64 image"}]}]}"#,
        ),
        (
            "responses",
            r#"{"model":"fixture-model","max_output_tokens":10,"input":"image base64"}"#,
        ),
        (
            "responses",
            r#"{"model":"fixture-model","max_output_tokens":10,"input":[{"type":"function_call","arguments":"{\"type\":\"image\"}"},{"role":"user","content":[{"type":"input_text","text":"hi"}]}]}"#,
        ),
        (
            "messages",
            r#"{"model":"fixture-model","max_tokens":10,"messages":[{"content":[{"type":"tool_use","input":{"type":"image"}},{"type":"tool_result","content":"base64"}]}]}"#,
        ),
    ] {
        let upstream = UpstreamFixture::start(upstream_response("200 OK", &[], b"{}")).await;
        let mut config = config(&upstream, Auth::None, false);
        config.quota.tpm = Limit::Known(10000.try_into().unwrap());
        let clock = llmgw::admission::ManualClock::default();
        let gateway = server::testing::spawn_with_clock(
            config,
            RuntimeCredentials::new(DATA_TOKEN, CONTROL_TOKEN, None).unwrap(),
            clock.clone(),
            None,
        )
        .await
        .unwrap();
        clock.advance_to(Duration::from_secs(60));
        let response = send_raw(
            gateway.address(),
            &post(&format!("/r/pi-work/v1/{endpoint}"), &[], body.as_bytes()),
        )
        .await;
        assert_eq!(status(&response), 200);
        assert_eq!(upstream.capture().await.body, body.as_bytes());
        assert_eq!(
            server::testing::quota_snapshot(&gateway).tpm_debited,
            body.len() as u128 + 10
        );
        gateway.shutdown().await.unwrap();
    }
}

#[tokio::test]
async fn model_default_and_explicit_cap_precedence_do_not_rewrite_body() {
    for body in [
        JSON,
        br#"{"model":"fixture-model","max_tokens":10,"messages":[]}"#,
    ] {
        let upstream = UpstreamFixture::start(upstream_response("200 OK", &[], b"{}")).await;
        let mut config = config(&upstream, Auth::None, false);
        config.quota.tpm = Limit::Known(10000.try_into().unwrap());
        config.models[0].max_output_tokens = Some(100.try_into().unwrap());
        let clock = llmgw::admission::ManualClock::default();
        let gateway = server::testing::spawn_with_clock(
            config,
            RuntimeCredentials::new(DATA_TOKEN, CONTROL_TOKEN, None).unwrap(),
            clock.clone(),
            None,
        )
        .await
        .unwrap();
        clock.advance_to(Duration::from_secs(60));
        assert_eq!(
            status(
                &send_raw(
                    gateway.address(),
                    &post("/r/pi-work/v1/chat/completions", &[], body)
                )
                .await
            ),
            200
        );
        assert_eq!(upstream.capture().await.body, body);
        assert_eq!(
            server::testing::quota_snapshot(&gateway).tpm_debited,
            body.len() as u128 + if body == JSON { 100 } else { 10 }
        );
        gateway.shutdown().await.unwrap();
    }
}

#[tokio::test]
async fn metadata_endpoints_have_zero_generation_tpm_and_share_rpm_across_roots() {
    let upstream = UpstreamFixture::start(upstream_response("200 OK", &[], b"{}")).await;
    let mut config = config(&upstream, Auth::None, false);
    config.quota.tpm = Limit::Known(1.try_into().unwrap());
    config.quota.rpm = Limit::Known(2.try_into().unwrap());
    let mut alias = config.roots[0].clone();
    alias.id = "alias".to_owned();
    config.roots.push(alias);
    let clock = llmgw::admission::ManualClock::default();
    let gateway = server::testing::spawn_with_clock(
        config,
        RuntimeCredentials::new(DATA_TOKEN, CONTROL_TOKEN, None).unwrap(),
        clock.clone(),
        None,
    )
    .await
    .unwrap();
    let get=b"GET /r/pi-work/v1/models HTTP/1.1\r\nHost: localhost\r\nX-LLMGW-Token: synthetic-data-token\r\nConnection: close\r\n\r\n";
    let first = send_raw(gateway.address(), get);
    tokio::pin!(first);
    assert!(
        tokio::time::timeout(Duration::from_millis(20), &mut first)
            .await
            .is_err()
    );
    assert_eq!(upstream.attempts(), 0);
    clock.advance_to(Duration::from_secs(60));
    assert_eq!(status(&first.await), 200);
    let count = post(
        "/r/alias/v1/messages/count_tokens",
        &[],
        br#"{"model":"fixture-model","messages":[{"content":[{"type":"image","source":{}}]}]}"#,
    );
    assert_eq!(status(&send_raw(gateway.address(), &count).await), 200);
    let snapshot = server::testing::quota_snapshot(&gateway);
    assert_eq!(snapshot.rpm_debited, 2);
    assert_eq!(snapshot.tpm_debited, 0);
    assert_eq!(snapshot.starts, 2);
    let next = send_raw(gateway.address(), get);
    tokio::pin!(next);
    assert!(
        tokio::time::timeout(Duration::from_millis(20), &mut next)
            .await
            .is_err()
    );
    assert_eq!(server::testing::quota_snapshot(&gateway).active, 0);
    clock.advance_to(Duration::from_secs(119));
    assert!(
        tokio::time::timeout(Duration::from_millis(20), &mut next)
            .await
            .is_err()
    );
    clock.advance_to(Duration::from_secs(120));
    assert_eq!(status(&next.await), 200);
    assert_eq!(upstream.attempts(), 3);
    gateway.shutdown().await.unwrap();
}

#[tokio::test]
async fn admitted_prestart_reset_releases_joint_hold_once_without_http_attempt() {
    use support::fixture::{abort_socket, open_raw};
    let upstream = UpstreamFixture::start(upstream_response("200 OK", &[], b"{}")).await;
    let mut config = config(&upstream, Auth::None, false);
    config.quota.rpm = Limit::Known(1.try_into().unwrap());
    config.quota.tpm = Limit::Known(10000.try_into().unwrap());
    config.models[0].max_output_tokens = Some(10.try_into().unwrap());
    let clock = llmgw::admission::ManualClock::default();
    let (gate, gate_rx) = tokio::sync::watch::channel(false);
    let gateway = server::testing::spawn_with_clock(
        config,
        RuntimeCredentials::new(DATA_TOKEN, CONTROL_TOKEN, None).unwrap(),
        clock.clone(),
        Some(gate_rx),
    )
    .await
    .unwrap();
    clock.advance_to(Duration::from_secs(60));
    let socket = open_raw(
        gateway.address(),
        &post("/r/pi-work/v1/chat/completions", &[], JSON),
    )
    .await;
    let snapshot = wait_quota(&gateway, |s| s.active == 1).await;
    assert_eq!(snapshot.starts, 0);
    assert_eq!(snapshot.rpm_debited, 0);
    clock.advance_to(Duration::from_secs(500));
    assert!(server::testing::quota_snapshot(&gateway).tpm_held > 0);
    abort_socket(socket);
    let snapshot = wait_quota(&gateway, |s| s.cleanups == 1).await;
    assert_eq!(snapshot.active, 0);
    assert_eq!(snapshot.tpm_held, 0);
    assert_eq!(snapshot.starts, 0);
    assert_eq!(upstream.attempts(), 0);
    gate.send(true).unwrap();
    assert_eq!(
        status(
            &send_raw(
                gateway.address(),
                &post("/r/pi-work/v1/chat/completions", &[], JSON)
            )
            .await
        ),
        200
    );
    let snapshot = server::testing::quota_snapshot(&gateway);
    assert_eq!(snapshot.starts, 1);
    assert_eq!(snapshot.cleanups, 2);
    assert_eq!(snapshot.rpm_debited, 1);
    gateway.shutdown().await.unwrap();
}

#[tokio::test]
async fn actual_wire_usage_cache_semantics_and_unknown_usage_keep_correct_debits() {
    for (endpoint, events, expected) in [
        (
            "chat/completions",
            "data: {\"choices\":[],\"usage\":{\"prompt_tokens\":20,\"completion_tokens\":3,\"prompt_tokens_details\":{\"cached_tokens\":7}}}\n\ndata: [DONE]\n\n",
            Some(23),
        ),
        (
            "responses",
            "data: {\"type\":\"response.completed\",\"response\":{\"usage\":{\"input_tokens\":13,\"output_tokens\":5,\"input_tokens_details\":{\"cached_tokens\":4}}}}\n\n",
            Some(18),
        ),
        (
            "messages",
            "event: message_start\ndata: {\"type\":\"message_start\",\"message\":{\"usage\":{\"input_tokens\":17,\"output_tokens\":1,\"cache_creation_input_tokens\":3,\"cache_read_input_tokens\":2}}}\n\nevent: message_delta\ndata: {\"type\":\"message_delta\",\"usage\":{\"output_tokens\":9}}\n\nevent: message_stop\ndata: {\"type\":\"message_stop\"}\n\n",
            Some(31),
        ),
        ("chat/completions", "data: [DONE]\n\n", None),
        (
            "chat/completions",
            "data: {\"choices\":[],\"usage\":{\"prompt_tokens\":-1,\"completion_tokens\":3}}\n\ndata: [DONE]\n\n",
            None,
        ),
    ] {
        let upstream = UpstreamFixture::start(upstream_response(
            "200 OK",
            &[("Content-Type", "text/event-stream")],
            events.as_bytes(),
        ))
        .await;
        let mut config = config(&upstream, Auth::None, false);
        config.accounting = Accounting::Actual;
        config.quota.tpm = Limit::Known(10000.try_into().unwrap());
        config.models[0].max_output_tokens = Some(10.try_into().unwrap());
        let clock = llmgw::admission::ManualClock::default();
        let gateway = server::testing::spawn_with_clock(
            config,
            RuntimeCredentials::new(DATA_TOKEN, CONTROL_TOKEN, None).unwrap(),
            clock.clone(),
            None,
        )
        .await
        .unwrap();
        clock.advance_to(Duration::from_secs(60));
        assert_eq!(
            status(
                &send_raw(
                    gateway.address(),
                    &post(&format!("/r/pi-work/v1/{endpoint}"), &[], JSON)
                )
                .await
            ),
            200
        );
        assert_eq!(
            server::testing::quota_snapshot(&gateway).tpm_debited,
            expected.unwrap_or(JSON.len() as u128 + 10)
        );
        gateway.shutdown().await.unwrap();
    }
}

#[tokio::test]
async fn known_quota_queue_deadline_holds_no_resources_and_starts_no_http() {
    let upstream = UpstreamFixture::start(upstream_response("200 OK", &[], b"{}")).await;
    let mut config = config(&upstream, Auth::None, false);
    config.quota.rpm = Limit::Known(1.try_into().unwrap());
    let gateway = server::testing::spawn_with_timeouts(
        config,
        RuntimeCredentials::new(DATA_TOKEN, CONTROL_TOKEN, None).unwrap(),
        Duration::from_millis(30),
        Duration::from_millis(100),
    )
    .await
    .unwrap();
    assert_eq!(
        status(
            &send_raw(
                gateway.address(),
                &post("/r/pi-work/v1/chat/completions", &[], JSON)
            )
            .await
        ),
        504
    );
    let s = server::testing::quota_snapshot(&gateway);
    assert_eq!(s.active, 0);
    assert_eq!(s.starts, 0);
    assert_eq!(s.cleanups, 0);
    assert_eq!(upstream.attempts(), 0);
    gateway.shutdown().await.unwrap();
}

#[tokio::test]
async fn stop_cancels_admitted_unstarted_worker_and_its_quota_hold() {
    let upstream = UpstreamFixture::start(upstream_response("200 OK", &[], b"{}")).await;
    let mut config = config(&upstream, Auth::None, false);
    config.quota.rpm = Limit::Known(1.try_into().unwrap());
    let clock = llmgw::admission::ManualClock::default();
    let (_gate, gate_rx) = tokio::sync::watch::channel(false);
    let gateway = server::testing::spawn_with_clock(
        config,
        RuntimeCredentials::new(DATA_TOKEN, CONTROL_TOKEN, None).unwrap(),
        clock.clone(),
        Some(gate_rx),
    )
    .await
    .unwrap();
    clock.advance_to(Duration::from_secs(60));
    let socket = support::fixture::open_raw(
        gateway.address(),
        &post("/r/pi-work/v1/chat/completions", &[], JSON),
    )
    .await;
    wait_quota(&gateway, |s| s.active == 1).await;
    let stop=b"POST /_llmgw/stop HTTP/1.1\r\nHost: localhost\r\nX-LLMGW-Control-Token: synthetic-control-token\r\nContent-Length: 0\r\nConnection: close\r\n\r\n";
    assert_eq!(status(&send_raw(gateway.address(), stop).await), 200);
    let s = wait_quota(&gateway, |s| s.cleanups == 1).await;
    assert_eq!(s.starts, 0);
    assert_eq!(s.active, 0);
    assert_eq!(s.rpm_debited, 0);
    assert_eq!(upstream.attempts(), 0);
    drop(socket);
    gateway.shutdown().await.unwrap();
}

#[tokio::test]
async fn known_tpm_cannot_be_bypassed_by_caps_modalities_or_nested_tool_media() {
    for body in [
        r#"{"model":"fixture-model","max_tokens":0}"#,
        r#"{"model":"fixture-model","max_tokens":null}"#,
        r#"{"model":"fixture-model","max_tokens":1.5}"#,
        r#"{"model":"fixture-model","max_tokens":18446744073709551615}"#,
        r#"{"model":"fixture-model","max_tokens":10,"n":2}"#,
        r#"{"model":"fixture-model","max_tokens":10,"modalities":["text","audio"]}"#,
        r#"{"model":"fixture-model","max_tokens":10,"messages":[{"content":[{"type":"tool_result","content":[{"type":"image","source":{}}]}]}]}"#,
    ] {
        let upstream = UpstreamFixture::start(upstream_response("200 OK", &[], b"{}")).await;
        let mut config = config(&upstream, Auth::None, false);
        config.quota.tpm = Limit::Known(1000.try_into().unwrap());
        config.models[0].max_output_tokens = Some(10.try_into().unwrap());
        let gateway = server::spawn(
            config,
            RuntimeCredentials::new(DATA_TOKEN, CONTROL_TOKEN, None).unwrap(),
        )
        .await
        .unwrap();
        assert_eq!(
            status(
                &send_raw(
                    gateway.address(),
                    &post("/r/pi-work/v1/chat/completions", &[], body.as_bytes())
                )
                .await
            ),
            400
        );
        assert_eq!(upstream.attempts(), 0);
        gateway.shutdown().await.unwrap();
    }
}

#[tokio::test]
async fn protocol_specific_opaque_fields_do_not_become_multimodal_inputs() {
    let upstream = UpstreamFixture::start(upstream_response("200 OK", &[], b"{}")).await;
    let mut config = config(&upstream, Auth::None, false);
    config.quota.tpm = Limit::Known(10000.try_into().unwrap());
    let clock = llmgw::admission::ManualClock::default();
    let gateway = server::testing::spawn_with_clock(
        config,
        RuntimeCredentials::new(DATA_TOKEN, CONTROL_TOKEN, None).unwrap(),
        clock.clone(),
        None,
    )
    .await
    .unwrap();
    clock.advance_to(Duration::from_secs(60));
    let body=br#"{"model":"fixture-model","max_tokens":10,"messages":[{"content":"text"}],"input":[{"type":"input_image"}]}"#;
    assert_eq!(
        status(
            &send_raw(
                gateway.address(),
                &post("/r/pi-work/v1/chat/completions", &[], body)
            )
            .await
        ),
        200
    );
    gateway.shutdown().await.unwrap();
}

#[tokio::test]
async fn responses_tool_output_media_is_rejected_before_admission() {
    let upstream = UpstreamFixture::start(upstream_response("200 OK", &[], b"{}")).await;
    let mut config = config(&upstream, Auth::None, false);
    config.quota.tpm = Limit::Known(10000.try_into().unwrap());
    let clock = llmgw::admission::ManualClock::default();
    let gateway = server::testing::spawn_with_clock(
        config,
        RuntimeCredentials::new(DATA_TOKEN, CONTROL_TOKEN, None).unwrap(),
        clock.clone(),
        None,
    )
    .await
    .unwrap();
    clock.advance_to(Duration::from_secs(60));
    let body=br#"{"model":"fixture-model","max_output_tokens":10,"input":[{"type":"function_call_output","output":[{"type":"input_image","image_url":"synthetic"}]}]}"#;
    assert_eq!(
        status(
            &send_raw(
                gateway.address(),
                &post("/r/pi-work/v1/responses", &[], body)
            )
            .await
        ),
        400
    );
    assert_eq!(upstream.attempts(), 0);
    gateway.shutdown().await.unwrap();
}
