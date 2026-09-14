use std::collections::HashSet;

use axum::http::header::{AUTHORIZATION, CONNECTION, HOST};
use axum::http::{HeaderMap, HeaderName, HeaderValue};

use crate::config::Auth;
use crate::control::CONTROL_TOKEN_HEADER;
use crate::protocol::DATA_TOKEN_HEADER;

const HOP_BY_HOP: &[&str] = &[
    "connection",
    "keep-alive",
    "proxy-authenticate",
    "proxy-authorization",
    "te",
    "trailer",
    "transfer-encoding",
    "upgrade",
];

pub fn stored_size(headers: &HeaderMap) -> usize {
    headers
        .iter()
        .map(|(name, value)| name.as_str().len() + value.as_bytes().len() + 4)
        .sum::<usize>()
        + 2
}

pub fn requests_upgrade(headers: &HeaderMap) -> bool {
    headers.contains_key("upgrade")
        || headers.contains_key("sec-websocket-key")
        || nominated(headers).contains("upgrade")
}

pub fn is_reserved_auth_target(name: &str) -> bool {
    name.eq_ignore_ascii_case("host")
        || name.eq_ignore_ascii_case("content-length")
        || name.eq_ignore_ascii_case(DATA_TOKEN_HEADER)
        || name.eq_ignore_ascii_case(CONTROL_TOKEN_HEADER)
        || HOP_BY_HOP
            .iter()
            .any(|reserved| name.eq_ignore_ascii_case(reserved))
}

pub fn prepare_request(headers: &mut HeaderMap, auth: &Auth, upstream_secret: Option<&[u8]>) {
    scrub(headers);
    headers.remove(HOST);
    headers.remove(DATA_TOKEN_HEADER);
    headers.remove(CONTROL_TOKEN_HEADER);
    match auth {
        Auth::Forward => {}
        Auth::Env { header, .. } => {
            headers.remove(AUTHORIZATION);
            headers.remove("x-api-key");
            if let (Ok(name), Some(secret)) =
                (HeaderName::from_bytes(header.as_bytes()), upstream_secret)
            {
                if let Ok(value) = HeaderValue::from_bytes(secret) {
                    headers.insert(name, value);
                }
            }
        }
        Auth::None => {
            headers.remove(AUTHORIZATION);
            headers.remove("x-api-key");
        }
    }
}

pub fn prepare_response(headers: &mut HeaderMap) {
    scrub(headers);
    headers.remove(DATA_TOKEN_HEADER);
    headers.remove(CONTROL_TOKEN_HEADER);
}

fn scrub(headers: &mut HeaderMap) {
    let nominated = nominated(headers);
    for name in nominated {
        headers.remove(name);
    }
    for name in HOP_BY_HOP {
        headers.remove(*name);
    }
}

fn nominated(headers: &HeaderMap) -> HashSet<String> {
    headers
        .get_all(CONNECTION)
        .iter()
        .filter_map(|value| value.to_str().ok())
        .flat_map(|value| value.split(','))
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_ascii_lowercase)
        .collect()
}
