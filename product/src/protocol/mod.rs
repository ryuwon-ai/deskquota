use std::collections::HashSet;

use axum::body::Body;
use axum::http::{Method, Response, StatusCode};

use crate::config::{Config, Endpoint, Method as ConfigMethod};

mod completions;
mod messages;
mod request;
mod responses;
pub use request::inspect_body;

pub const DATA_TOKEN_HEADER: &str = "x-llmgw-token";

#[derive(Clone, Copy, Eq, PartialEq)]
pub struct ObservedUsage {
    pub input_tokens: u64,
    pub output_tokens: u64,
    pub cache_creation_input_tokens: u64,
    pub cache_read_input_tokens: u64,
}

pub(crate) struct EndpointObserver {
    inner: EndpointObserverInner,
}

enum EndpointObserverInner {
    Completions(completions::Observer),
    Responses(responses::Observer),
    Messages(messages::Observer),
    Unsupported,
}

impl EndpointObserver {
    pub(crate) fn new(endpoint: Endpoint) -> Self {
        let inner = match endpoint {
            Endpoint::ChatCompletions => {
                EndpointObserverInner::Completions(completions::Observer::new())
            }
            Endpoint::Responses => EndpointObserverInner::Responses(responses::Observer::new()),
            Endpoint::Messages => EndpointObserverInner::Messages(messages::Observer::new()),
            Endpoint::CountTokens | Endpoint::Models => EndpointObserverInner::Unsupported,
        };
        Self { inner }
    }

    pub(crate) fn observe(&mut self, event: Option<&str>, data: &[u8]) {
        if event == Some("error") {
            self.mark_invalid();
        }
        match &mut self.inner {
            EndpointObserverInner::Completions(observer) => observer.observe(data),
            EndpointObserverInner::Responses(observer) => observer.observe(data),
            EndpointObserverInner::Messages(observer) => observer.observe(event, data),
            EndpointObserverInner::Unsupported => {}
        }
    }

    pub(crate) fn mark_invalid(&mut self) {
        match &mut self.inner {
            EndpointObserverInner::Completions(observer) => observer.mark_invalid(),
            EndpointObserverInner::Responses(observer) => observer.mark_invalid(),
            EndpointObserverInner::Messages(observer) => observer.mark_invalid(),
            EndpointObserverInner::Unsupported => {}
        }
    }

    pub(crate) fn first_output_delta(&self) -> bool {
        match &self.inner {
            EndpointObserverInner::Completions(observer) => observer.first_output_delta(),
            EndpointObserverInner::Responses(observer) => observer.first_output_delta(),
            EndpointObserverInner::Messages(observer) => observer.first_output_delta(),
            EndpointObserverInner::Unsupported => false,
        }
    }

    pub(crate) fn terminal_marker(&self) -> bool {
        match &self.inner {
            EndpointObserverInner::Completions(observer) => observer.terminal_marker(),
            EndpointObserverInner::Responses(observer) => observer.terminal_marker(),
            EndpointObserverInner::Messages(observer) => observer.terminal_marker(),
            EndpointObserverInner::Unsupported => false,
        }
    }

    pub(crate) fn finish(&self) -> Option<ObservedUsage> {
        match &self.inner {
            EndpointObserverInner::Completions(observer) => observer.finish(),
            EndpointObserverInner::Responses(observer) => observer.finish(),
            EndpointObserverInner::Messages(observer) => observer.finish(),
            EndpointObserverInner::Unsupported => None,
        }
    }
}

fn token(value: &serde_json::Value, key: &str) -> Result<Option<u64>, ()> {
    match value.get(key) {
        None => Ok(None),
        Some(value) => value.as_u64().map(Some).ok_or(()),
    }
}

fn nested_token(
    value: &serde_json::Value,
    object_key: &str,
    token_key: &str,
) -> Result<Option<u64>, ()> {
    match value.get(object_key) {
        None | Some(serde_json::Value::Null) => Ok(None),
        Some(details @ serde_json::Value::Object(_)) => token(details, token_key),
        Some(_) => Err(()),
    }
}

pub struct DataRoute {
    pub root_index: usize,
    pub endpoint: Endpoint,
}

pub struct ProtocolError {
    status: StatusCode,
    code: &'static str,
}

impl ProtocolError {
    pub fn into_response(self) -> Response<Body> {
        error(self.status, self.code)
    }
}

pub fn route(config: &Config, method: &Method, path: &str) -> Result<DataRoute, ProtocolError> {
    for (root_index, root) in config.roots.iter().enumerate() {
        for endpoint in &root.endpoints {
            let expected_path = format!("/r/{}/v1/{}", root.id, endpoint.path());
            if path == expected_path {
                if method_matches(endpoint.method(), method) {
                    return Ok(DataRoute {
                        root_index,
                        endpoint: *endpoint,
                    });
                }
                return Err(reject(StatusCode::METHOD_NOT_ALLOWED, "method_not_allowed"));
            }
        }
    }
    Err(reject(StatusCode::NOT_FOUND, "route_not_found"))
}

pub fn upstream_url(
    api_base: &url::Url,
    route: &DataRoute,
    request_query: Option<&str>,
) -> Result<url::Url, ProtocolError> {
    let base_query = api_base.query();
    if queries_conflict(base_query, request_query) {
        return Err(reject(StatusCode::BAD_REQUEST, "query_conflict"));
    }
    let composed_query = compose_query(base_query, request_query)?;
    let mut url = api_base.clone();
    let path = format!(
        "{}/{}",
        api_base.path().trim_end_matches('/'),
        route.endpoint.path()
    );
    url.set_path(&path);
    url.set_query(composed_query.as_deref());
    let serialized_query = raw_query_component(url.as_str());
    if url.query() != composed_query.as_deref() || serialized_query != composed_query.as_deref() {
        return Err(reject(
            StatusCode::BAD_REQUEST,
            "unsupported_query_encoding",
        ));
    }
    Ok(url)
}

pub fn error(status: StatusCode, code: &'static str) -> Response<Body> {
    let message = match code {
        "output_bound_required" => Some(
            "Known TPM requires a positive request output cap or configured model max_output_tokens.",
        ),
        "invalid_output_bound" => Some(
            "The inspected output cap must be a positive integer; null, zero, fractional and malformed values are unsupported.",
        ),
        "ambiguous_output_bound" => {
            Some("Use exactly one Chat output cap: max_completion_tokens or max_tokens.")
        }
        "unsupported_output_bound_field" => Some(
            "Use max_tokens for Messages, max_output_tokens for Responses, or one of max_tokens and max_completion_tokens for Chat.",
        ),
        "unsupported_multimodal_estimate" => Some(
            "Known TPM supports the documented text input forms only. Media or unsupported content cannot be estimated; TPM unknown leaves TPM unenforced.",
        ),
        "unsupported_generation_count" => {
            Some("Known TPM supports one Chat generation per request; omit n or set n to 1.")
        }
        "estimate_exceeds_budget" => Some(
            "The JSON UTF-8 byte proxy plus output reservation exceeds the configured local TPM budget. This is an estimate, not an actual token count.",
        ),
        _ => None,
    };
    let body = match message {
        Some(message) => format!("{{\"error\":{{\"code\":\"{code}\",\"message\":\"{message}\"}}}}"),
        None => format!("{{\"error\":{{\"code\":\"{code}\"}}}}"),
    };
    Response::builder()
        .status(status)
        .header("content-type", "application/json")
        .body(Body::from(body))
        .expect("static protocol response")
}

fn reject(status: StatusCode, code: &'static str) -> ProtocolError {
    ProtocolError { status, code }
}

fn method_matches(configured: ConfigMethod, actual: &Method) -> bool {
    matches!(
        (configured, actual),
        (ConfigMethod::Get, &Method::GET) | (ConfigMethod::Post, &Method::POST)
    )
}

fn queries_conflict(base: Option<&str>, request: Option<&str>) -> bool {
    let Some(base) = base else {
        return false;
    };
    let Some(request) = request else {
        return false;
    };
    let base_keys: HashSet<_> = url::form_urlencoded::parse(base.as_bytes())
        .map(|(key, _)| key.into_owned())
        .collect();
    url::form_urlencoded::parse(request.as_bytes()).any(|(key, _)| base_keys.contains(key.as_ref()))
}

fn compose_query(
    base: Option<&str>,
    request: Option<&str>,
) -> Result<Option<String>, ProtocolError> {
    if base == Some("") || request == Some("") {
        return Err(reject(
            StatusCode::BAD_REQUEST,
            "unsupported_query_encoding",
        ));
    }
    Ok(match (base, request) {
        (Some(base), Some(request)) => Some(format!("{base}&{request}")),
        (Some(base), None) => Some(base.to_owned()),
        (None, Some(request)) => Some(request.to_owned()),
        (None, None) => None,
    })
}

fn raw_query_component(value: &str) -> Option<&str> {
    value.split_once('?').map(|(_, query)| query)
}
