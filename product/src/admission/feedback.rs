//! Conservative exhaustion hints for the configured shared quota group.
use axum::http::HeaderMap;
use std::time::{Duration, SystemTime};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct Exhausted {
    pub delay: Duration,
    pub generation_only: bool,
}

/// Only exact zero with a valid reset creates a gate; positive values are not credits.
pub(super) fn exhausted(headers: &HeaderMap, now: SystemTime) -> Option<Exhausted> {
    let mut result: Option<Exhausted> = None;
    for (remaining, reset, generation_only, timestamp) in [
        (
            "x-ratelimit-remaining-requests",
            "x-ratelimit-reset-requests",
            false,
            false,
        ),
        (
            "x-ratelimit-remaining-tokens",
            "x-ratelimit-reset-tokens",
            true,
            false,
        ),
        (
            "x-ratelimit-remaining-project-tokens",
            "x-ratelimit-reset-project-tokens",
            true,
            false,
        ),
        (
            "anthropic-ratelimit-requests-remaining",
            "anthropic-ratelimit-requests-reset",
            false,
            true,
        ),
    ] {
        let Some(value) = unique(headers, remaining) else {
            continue;
        };
        if value.is_empty()
            || !value.bytes().all(|b| b.is_ascii_digit())
            || value.parse::<u64>() != Ok(0)
        {
            continue;
        }
        let Some(reset) = unique(headers, reset) else {
            continue;
        };
        let delay = if timestamp {
            humantime::parse_rfc3339(reset)
                .ok()
                .map(|at| at.duration_since(now).unwrap_or_default())
        } else {
            humantime::parse_duration(reset).ok()
        };
        if let Some(delay) = delay {
            result = Some(match result {
                Some(previous) => Exhausted {
                    delay: previous.delay.max(delay),
                    generation_only: previous.generation_only || generation_only,
                },
                None => Exhausted {
                    delay,
                    generation_only,
                },
            });
        }
    }
    result
}

fn unique<'a>(headers: &'a HeaderMap, name: &str) -> Option<&'a str> {
    let mut values = headers.get_all(name).iter();
    let value = values.next()?.to_str().ok()?.trim();
    values
        .all(|next| next.to_str().ok().is_some_and(|next| next.trim() == value))
        .then_some(value)
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::http::HeaderValue;

    #[test]
    fn exact_feedback_accepts_supported_resets_and_rejects_ambiguous_values() {
        let now = humantime::parse_rfc3339("2026-09-20T00:00:00Z").unwrap();
        for (remaining, reset, expected) in [
            ("0", "1m2s400ms", Some(Duration::from_millis(62_400))),
            ("00", "0s", Some(Duration::ZERO)),
            ("1", "2s", None),
            ("-0", "2s", None),
            ("0.0", "2s", None),
            ("18446744073709551616", "2s", None),
            ("0", "-2s", None),
            ("0", "999999999999999999999999999999999999999999s", None),
            ("0", "tomorrow", None),
            ("0", "", None),
        ] {
            let mut headers = HeaderMap::new();
            headers.insert(
                "x-ratelimit-remaining-requests",
                HeaderValue::from_str(remaining).unwrap(),
            );
            headers.insert(
                "x-ratelimit-reset-requests",
                HeaderValue::from_str(reset).unwrap(),
            );
            assert_eq!(exhausted(&headers, now).map(|hint| hint.delay), expected);
        }
        let mut headers = HeaderMap::new();
        headers.insert(
            "anthropic-ratelimit-requests-remaining",
            HeaderValue::from_static("0"),
        );
        headers.insert(
            "anthropic-ratelimit-requests-reset",
            HeaderValue::from_static("2026-09-20T00:00:02+00:00"),
        );
        assert_eq!(
            exhausted(&headers, now),
            Some(Exhausted {
                delay: Duration::from_secs(2),
                generation_only: false
            })
        );
        headers.append(
            "anthropic-ratelimit-requests-remaining",
            HeaderValue::from_static("1"),
        );
        assert_eq!(exhausted(&headers, now), None);
        headers.clear();
        headers.insert(
            "anthropic-ratelimit-tokens-remaining",
            HeaderValue::from_static("0"),
        );
        headers.insert(
            "anthropic-ratelimit-tokens-reset",
            HeaderValue::from_static("2026-09-20T00:00:02Z"),
        );
        assert_eq!(
            exhausted(&headers, now),
            None,
            "Anthropic token counts are rounded"
        );
        for suffix in ["tokens", "project-tokens"] {
            let mut headers = HeaderMap::new();
            headers.insert(
                axum::http::HeaderName::from_bytes(
                    format!("x-ratelimit-remaining-{suffix}").as_bytes(),
                )
                .unwrap(),
                HeaderValue::from_static("0"),
            );
            let reset = axum::http::HeaderName::from_bytes(
                format!("x-ratelimit-reset-{suffix}").as_bytes(),
            )
            .unwrap();
            headers.append(&reset, HeaderValue::from_static("2s"));
            headers.append(&reset, HeaderValue::from_static("2s"));
            assert!(exhausted(&headers, now).unwrap().generation_only);
            headers.append(reset, HeaderValue::from_static("1s"));
            assert_eq!(exhausted(&headers, now), None);
        }
    }
}
