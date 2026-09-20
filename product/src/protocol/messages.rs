use serde_json::Value;

use super::{ObservedUsage, token};

pub(super) struct Observer {
    cache: Option<crate::cache::StreamCompletion>,
    usage: ObservedUsageParts,
    invalid: bool,
    output_delta: bool,
    terminal: bool,
    final_output_usage_seen: bool,
}

#[derive(Default)]
struct ObservedUsageParts {
    input_tokens: Option<u64>,
    output_tokens: Option<u64>,
    cache_creation_input_tokens: Option<u64>,
    cache_read_input_tokens: Option<u64>,
}

impl Observer {
    pub(super) fn new(cache: bool) -> Self {
        Self {
            cache: cache
                .then(|| crate::cache::StreamCompletion::new(crate::config::Endpoint::Messages)),
            usage: ObservedUsageParts::default(),
            invalid: false,
            output_delta: false,
            terminal: false,
            final_output_usage_seen: false,
        }
    }

    pub(super) fn observe(&mut self, event: Option<&str>, value: &Value) {
        if let Some(cache) = &mut self.cache {
            cache.observe(event, value);
        }
        let kind = value.get("type").and_then(Value::as_str).or(event);
        match kind {
            Some("message_start") => {
                if let Some(usage) = value.pointer("/message/usage") {
                    if self.terminal {
                        self.invalid = true;
                        return;
                    }
                    self.assign_usage(usage);
                }
            }
            Some("message_delta") => {
                if let Some(usage) = value.get("usage") {
                    if self.terminal {
                        self.invalid = true;
                        return;
                    }
                    let previous = self.usage.output_tokens;
                    let output_present = usage.get("output_tokens").is_some();
                    self.assign_usage(usage);
                    if output_present && let Some(current) = self.usage.output_tokens {
                        if previous.is_some_and(|previous| current < previous) {
                            self.invalid = true;
                        }
                        self.final_output_usage_seen = true;
                    }
                }
            }
            Some("content_block_delta") => {
                self.output_delta |= value
                    .pointer("/delta/text")
                    .or_else(|| value.pointer("/delta/partial_json"))
                    .and_then(Value::as_str)
                    .is_some_and(|delta| !delta.is_empty());
            }
            Some("message_stop") => self.terminal = true,
            Some("error") => self.invalid = true,
            _ => {}
        }
    }

    fn assign_usage(&mut self, usage: &Value) {
        for (key, target) in [
            ("input_tokens", &mut self.usage.input_tokens),
            ("output_tokens", &mut self.usage.output_tokens),
            (
                "cache_creation_input_tokens",
                &mut self.usage.cache_creation_input_tokens,
            ),
            (
                "cache_read_input_tokens",
                &mut self.usage.cache_read_input_tokens,
            ),
        ] {
            match token(usage, key) {
                Ok(Some(value)) => {
                    if target.is_some_and(|previous| value < previous) {
                        self.invalid = true;
                    }
                    *target = Some(value);
                }
                Ok(None) => {}
                Err(()) => self.invalid = true,
            }
        }
    }

    pub(super) fn cache_complete(&self) -> bool {
        self.cache
            .as_ref()
            .is_some_and(crate::cache::StreamCompletion::complete)
    }

    pub(super) fn mark_invalid(&mut self) {
        if let Some(cache) = &mut self.cache {
            cache.invalidate();
        }
        self.invalid = true;
    }

    pub(super) fn first_output_delta(&self) -> bool {
        self.output_delta
    }

    pub(super) fn terminal_marker(&self) -> bool {
        self.terminal
    }

    pub(super) fn finish(&self) -> Option<ObservedUsage> {
        if self.invalid || !self.terminal || !self.final_output_usage_seen {
            return None;
        }
        Some(ObservedUsage {
            input_tokens: self.usage.input_tokens?,
            output_tokens: self.usage.output_tokens?,
            cache_creation_input_tokens: self.usage.cache_creation_input_tokens.unwrap_or(0),
            cache_read_input_tokens: self.usage.cache_read_input_tokens.unwrap_or(0),
        })
    }
}
