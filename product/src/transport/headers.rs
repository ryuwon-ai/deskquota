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

/// Native loopback clients are trusted; reject browser-origin and rebinding requests.
pub(crate) fn validate_local_request(
    headers: &HeaderMap,
    method: &axum::http::Method,
    port: u16,
) -> Result<(), (axum::http::StatusCode, &'static str)> {
    use axum::http::{Method, StatusCode, uri::Authority};
    if headers.contains_key("origin") {
        return Err((StatusCode::FORBIDDEN, "browser_origin_not_allowed"));
    }
    let mut hosts = headers.get_all(HOST).iter();
    let authority = hosts
        .next()
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.parse::<Authority>().ok());
    let valid = authority.is_some_and(|authority| {
        let host = authority.host();
        let numeric = host
            .trim_start_matches('[')
            .trim_end_matches(']')
            .parse::<std::net::IpAddr>()
            .is_ok_and(|ip| ip.is_loopback());
        !authority.as_str().contains('@')
            && (host.eq_ignore_ascii_case("localhost") || numeric)
            && match authority.port_u16() {
                Some(value) => value == port,
                None => authority.as_str() == host && port == 80,
            }
    });
    if !valid || hosts.next().is_some() {
        return Err((StatusCode::FORBIDDEN, "invalid_local_host"));
    }
    if method == Method::POST {
        let mut types = headers.get_all("content-type").iter();
        let valid = types
            .next()
            .and_then(|value| value.to_str().ok())
            .is_some_and(|value| {
                value
                    .split(';')
                    .next()
                    .is_some_and(|media| media.trim().eq_ignore_ascii_case("application/json"))
            });
        if !valid || types.next().is_some() {
            return Err((
                StatusCode::UNSUPPORTED_MEDIA_TYPE,
                "json_content_type_required",
            ));
        }
    }
    Ok(())
}

#[cfg(test)]
mod local_request_tests {
    use super::*;
    use axum::http::Method;

    #[test]
    fn authority_and_json_boundary_reject_browser_capable_requests() {
        for host in ["localhost:4141", "127.0.0.1:4141", "[::1]:4141"] {
            let mut headers = HeaderMap::new();
            headers.insert(HOST, host.parse().unwrap());
            headers.insert(
                "content-type",
                "application/json; charset=utf-8".parse().unwrap(),
            );
            assert!(validate_local_request(&headers, &Method::POST, 4141).is_ok());
        }
        for host in [
            "attacker.example:4141",
            "127.0.0.1:80",
            "localhost",
            "user@localhost:4141",
            "localhost:65536",
        ] {
            let mut headers = HeaderMap::new();
            headers.insert(HOST, host.parse().unwrap());
            assert!(validate_local_request(&headers, &Method::GET, 4141).is_err());
        }
        for host in ["localhost:bad", "localhost:65536"] {
            let mut headers = HeaderMap::new();
            headers.insert(HOST, host.parse().unwrap());
            assert!(validate_local_request(&headers, &Method::GET, 80).is_err());
        }
        let mut headers = HeaderMap::new();
        headers.insert(HOST, "localhost:4141".parse().unwrap());
        assert!(validate_local_request(&headers, &Method::POST, 4141).is_err());
        headers.insert("content-type", "text/plain".parse().unwrap());
        assert!(validate_local_request(&headers, &Method::POST, 4141).is_err());
        headers.insert("content-type", "application/json".parse().unwrap());
        headers.insert("origin", "null".parse().unwrap());
        assert!(validate_local_request(&headers, &Method::POST, 4141).is_err());
    }
}
