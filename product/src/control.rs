use axum::body::Body;
use axum::http::{HeaderMap, Method, Response, StatusCode};
use tokio::sync::watch;

use crate::protocol;

pub const CONTROL_TOKEN_HEADER: &str = "x-llmgw-control-token";

pub fn handle(
    method: &Method,
    path: &str,
    headers: &HeaderMap,
    authenticated: bool,
    instance_nonce: Option<&str>,
    status_json: impl FnOnce() -> Vec<u8>,
    stop: &watch::Sender<bool>,
) -> Response<Body> {
    if headers.contains_key("origin") {
        return protocol::error(StatusCode::FORBIDDEN, "control_origin_forbidden");
    }
    if !authenticated {
        return protocol::error(StatusCode::UNAUTHORIZED, "invalid_control_token");
    }
    // An owned CLI's conditional stop cannot cross a worker restart between
    // health and stop. Token-only manual control remains a deliberate API.
    if path == "/_llmgw/stop" && headers.contains_key("x-llmgw-instance-nonce") {
        let mut nonces = headers.get_all("x-llmgw-instance-nonce").iter();
        if !instance_nonce.is_some_and(|expected| {
            nonces
                .next()
                .is_some_and(|value| value.as_bytes() == expected.as_bytes())
        }) || nonces.next().is_some()
        {
            return protocol::error(StatusCode::CONFLICT, "instance_changed");
        }
    }
    match (method, path) {
        (&Method::GET, "/_llmgw/health") => json(StatusCode::OK, status_json()),
        (&Method::GET, "/_llmgw/status") => json(StatusCode::OK, status_json()),
        (&Method::POST, "/_llmgw/stop") => {
            let _ = stop.send(true);
            json(StatusCode::OK, br#"{"status":"stopping"}"#.to_vec())
        }
        _ => protocol::error(StatusCode::NOT_FOUND, "control_route_not_found"),
    }
}

fn json(status: StatusCode, body: Vec<u8>) -> Response<Body> {
    Response::builder()
        .status(status)
        .header("content-type", "application/json")
        .body(Body::from(body))
        .expect("static control response")
}
