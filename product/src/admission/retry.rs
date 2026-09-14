//! Retry timing is independent of the error body's replay qualification.
use axum::http::HeaderMap;
use std::time::{Duration, SystemTime};

/// Standard HTTP Retry-After and the Azure OpenAI integer-millisecond extension.
/// Multiple valid delays take their maximum, never shortening a server wait.
pub fn header_delay(headers: &HeaderMap, now: SystemTime) -> Option<Duration> {
    let seconds = headers.get_all("retry-after").iter().filter_map(|value| {
        let value = value.to_str().ok()?.trim();
        numeric_cooldown(value, Duration::from_secs).or_else(|| seconds_or_date(value, now))
    });
    let milliseconds = headers
        .get_all("retry-after-ms")
        .iter()
        .filter_map(|value| numeric_cooldown(value.to_str().ok()?.trim(), Duration::from_millis));
    seconds.chain(milliseconds).max()
}
fn integer(value: &str) -> Option<u64> {
    (!value.is_empty() && value.bytes().all(|b| b.is_ascii_digit()))
        .then(|| value.parse().ok())
        .flatten()
}

pub const MAX_ERROR_BODY: usize = 16 * 1024;

/// Only a complete, bounded, identity JSON error with an explicit known code qualifies.
/// Serde's named fields reject duplicate discriminators instead of last-value wins.
pub fn transient(headers: &HeaderMap, bytes: &[u8]) -> bool {
    if bytes.len() > MAX_ERROR_BODY
        || !headers.get_all("content-type").iter().all(|h| {
            h.to_str().ok().is_some_and(|s| {
                s.split(';')
                    .next()
                    .is_some_and(|s| s.trim().eq_ignore_ascii_case("application/json"))
            })
        })
        || !headers.contains_key("content-type")
        || !headers.get_all("content-encoding").iter().all(|h| {
            h.to_str()
                .ok()
                .is_some_and(|s| s.eq_ignore_ascii_case("identity"))
        })
    {
        return false;
    }
    #[derive(serde::Deserialize)]
    struct Envelope {
        error: Error,
        r#type: Option<String>,
        code: Option<serde::de::IgnoredAny>,
    }
    #[derive(serde::Deserialize)]
    struct Error {
        code: String,
        r#type: Option<String>,
        details: Option<serde::de::IgnoredAny>,
    }
    let Ok(text) = std::str::from_utf8(bytes) else {
        return false;
    };
    // IgnoredAny can skip invalid UTF-8/Unicode escapes in unrelated strings.
    // Validate the entire bounded document before the duplicate-rejecting projection.
    if serde_json::from_str::<serde_json::Value>(text).is_err() {
        return false;
    }
    let Ok(error) = serde_json::from_str::<Envelope>(text) else {
        return false;
    };
    error.r#type.as_deref().is_none_or(|t| t == "error")
        && error.code.is_none()
        && error.error.details.is_none()
        && match error.error.code.as_str() {
            "slow_down" => error
                .error
                .r#type
                .as_deref()
                .is_none_or(|t| t == "rate_limit_error"),
            "rate_limit_exceeded" => error
                .error
                .r#type
                .as_deref()
                .is_none_or(|t| matches!(t, "rate_limit_error" | "requests" | "tokens")),
            _ => false,
        }
}

pub fn fallback_delay() -> Duration {
    Duration::from_millis(1000 + rand::random_range(0..=250))
}

fn seconds_or_date(value: &str, now: SystemTime) -> Option<Duration> {
    integer(value).map(Duration::from_secs).or_else(|| {
        httpdate::parse_http_date(value)
            .ok()
            .map(|at| at.duration_since(now).unwrap_or_default())
    })
}
/// Malformed explicit timing cannot silently become the shorter missing-header fallback.
pub fn timing_allows_retry(headers: &HeaderMap) -> bool {
    headers.get_all("retry-after").iter().all(|h| {
        h.to_str()
            .ok()
            .and_then(|s| seconds_or_date(s.trim(), SystemTime::now()))
            .is_some()
    }) && headers
        .get_all("retry-after-ms")
        .iter()
        .all(|h| h.to_str().ok().and_then(|s| integer(s.trim())).is_some())
}
pub fn missing_timing(headers: &HeaderMap) -> bool {
    !headers.contains_key("retry-after") && !headers.contains_key("retry-after-ms")
}

// HTTP decimal syntax has no u64 digit limit. Preserve an oversized positive wait
// conservatively at the group, while timing_allows_retry still denies its replay.
fn numeric_cooldown(value: &str, scale: fn(u64) -> Duration) -> Option<Duration> {
    if value.is_empty() || !value.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    Some(value.parse::<u64>().map(scale).unwrap_or(Duration::MAX))
}
