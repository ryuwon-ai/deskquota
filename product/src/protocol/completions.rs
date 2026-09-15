use serde_json::Value;

use super::{ObservedUsage, nested_token, token};

pub(super) struct Observer {
    cache: Option<crate::cache::StreamCompletion>,
    final_usage: Option<ObservedUsage>,
    invalid: bool,
    output_delta: bool,
    terminal: bool,
}

impl Observer {
    pub(super) fn new(cache: bool) -> Self {
        Self {
            cache: cache.then(|| {
                crate::cache::StreamCompletion::new(crate::config::Endpoint::ChatCompletions)
            }),
            final_usage: None,
            invalid: false,
            output_delta: false,
            terminal: false,
        }
    }

    pub(super) fn observe(&mut self, data: &[u8]) {
        if data == b"[DONE]" {
            if let Some(cache) = &mut self.cache {
                cache.done();
            }
            self.terminal = true;
            return;
        }
        let Ok(value) = serde_json::from_slice::<Value>(data) else {
            if let Some(cache) = &mut self.cache {
                cache.invalidate();
            }
            self.invalid = true;
            return;
        };
        if let Some(cache) = &mut self.cache {
            cache.observe(None, &value);
        }
        if value.get("error").is_some_and(|error| !error.is_null()) {
            self.invalid = true;
            return;
        }
        if value
            .get("choices")
            .and_then(Value::as_array)
            .is_some_and(|choices| choices.iter().any(has_output_delta))
        {
            self.output_delta = true;
        }
        let Some(usage) = value.get("usage") else {
            return;
        };
        if usage.is_null() {
            return;
        }
        if self.terminal {
            self.invalid = true;
            return;
        }
        // Only the documented final empty-choices usage chunk qualifies for
        // accounting. Interim/unsupported shapes cannot supply final usage.
        if !value
            .get("choices")
            .and_then(Value::as_array)
            .is_some_and(Vec::is_empty)
        {
            return;
        }
        match (
            token(usage, "prompt_tokens"),
            token(usage, "completion_tokens"),
        ) {
            (Ok(Some(input_tokens)), Ok(Some(output_tokens))) => {
                let cache_read_input_tokens =
                    match nested_token(usage, "prompt_tokens_details", "cached_tokens") {
                        Ok(Some(value)) => value,
                        Ok(None) => 0,
                        Err(()) => {
                            self.invalid = true;
                            return;
                        }
                    };
                if cache_read_input_tokens > input_tokens {
                    self.invalid = true;
                    return;
                }
                let observed = ObservedUsage {
                    input_tokens,
                    output_tokens,
                    cache_creation_input_tokens: 0,
                    cache_read_input_tokens,
                };
                if self.final_usage.is_some_and(|previous| {
                    observed.input_tokens < previous.input_tokens
                        || observed.output_tokens < previous.output_tokens
                        || observed.cache_read_input_tokens < previous.cache_read_input_tokens
                }) {
                    self.invalid = true;
                    return;
                }
                self.final_usage = Some(observed);
            }
            _ => self.invalid = true,
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
        (!self.invalid && self.terminal)
            .then_some(self.final_usage)
            .flatten()
    }
}

fn has_output_delta(choice: &Value) -> bool {
    let Some(delta) = choice.get("delta") else {
        return false;
    };
    delta
        .get("content")
        .and_then(Value::as_str)
        .is_some_and(|value| !value.is_empty())
        || delta
            .get("tool_calls")
            .and_then(Value::as_array)
            .is_some_and(|calls| {
                calls.iter().any(|call| {
                    call.pointer("/function/arguments")
                        .and_then(Value::as_str)
                        .is_some_and(|value| !value.is_empty())
                })
            })
}
