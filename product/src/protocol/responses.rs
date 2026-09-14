use serde_json::Value;

use super::{ObservedUsage, nested_token, token};

pub(super) struct Observer {
    usage: Option<ObservedUsage>,
    invalid: bool,
    output_delta: bool,
    terminal: bool,
    completed: bool,
}

impl Observer {
    pub(super) fn new() -> Self {
        Self {
            usage: None,
            invalid: false,
            output_delta: false,
            terminal: false,
            completed: false,
        }
    }

    pub(super) fn observe(&mut self, data: &[u8]) {
        let Ok(value) = serde_json::from_slice::<Value>(data) else {
            self.invalid = true;
            return;
        };
        let Some(kind) = value.get("type").and_then(Value::as_str) else {
            return;
        };
        if matches!(
            kind,
            "response.output_text.delta" | "response.function_call_arguments.delta"
        ) && value
            .get("delta")
            .and_then(Value::as_str)
            .is_some_and(|delta| !delta.is_empty())
        {
            self.output_delta = true;
        }
        match kind {
            "response.completed" => {
                self.terminal = true;
                self.completed = true;
                if value
                    .pointer("/response/status")
                    .is_some_and(|status| status.as_str() != Some("completed"))
                    || value
                        .pointer("/response/error")
                        .is_some_and(|error| !error.is_null())
                {
                    self.invalid = true;
                    return;
                }
                let Some(usage) = value.pointer("/response/usage") else {
                    self.invalid = true;
                    return;
                };
                match (token(usage, "input_tokens"), token(usage, "output_tokens")) {
                    (Ok(Some(input_tokens)), Ok(Some(output_tokens))) => {
                        let cached =
                            match nested_token(usage, "input_tokens_details", "cached_tokens") {
                                Ok(Some(value)) => value,
                                Ok(None) => 0,
                                Err(()) => {
                                    self.invalid = true;
                                    return;
                                }
                            };
                        if cached > input_tokens {
                            self.invalid = true;
                            return;
                        }
                        let observed = ObservedUsage {
                            input_tokens,
                            output_tokens,
                            cache_creation_input_tokens: 0,
                            cache_read_input_tokens: cached,
                        };
                        if self.usage.is_some_and(|previous| {
                            observed.input_tokens < previous.input_tokens
                                || observed.output_tokens < previous.output_tokens
                                || observed.cache_read_input_tokens
                                    < previous.cache_read_input_tokens
                        }) {
                            self.invalid = true;
                            return;
                        }
                        self.usage = Some(observed);
                    }
                    _ => self.invalid = true,
                }
            }
            "response.incomplete" | "response.failed" | "error" => {
                self.terminal = true;
                // Sticky across either ordering of unsuccessful/successful
                // terminal events: stale completed usage must never settle.
                self.invalid = true;
            }
            _ => {}
        }
    }

    pub(super) fn mark_invalid(&mut self) {
        self.invalid = true;
    }

    pub(super) fn first_output_delta(&self) -> bool {
        self.output_delta
    }

    pub(super) fn terminal_marker(&self) -> bool {
        self.terminal
    }

    pub(super) fn finish(&self) -> Option<ObservedUsage> {
        (!self.invalid && self.terminal && self.completed)
            .then_some(self.usage)
            .flatten()
    }
}
