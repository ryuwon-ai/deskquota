//! Dialoguer adapter. It is constructed only after the CLI confirms a real terminal.
use super::*;
use dialoguer::{Confirm, Input, MultiSelect, Select, console::Term, theme::ColorfulTheme};

fn validate_input_token_overhead(value: &str) -> Result<(), Error> {
    match value.parse::<u64>() {
        Ok(value) if value <= i64::MAX as u64 => Ok(()),
        _ => Err(Error::message(
            "input token overhead must be 0..=9223372036854775807 (TOML integer range)",
        )),
    }
}

pub struct DialoguerIo {
    term: Term,
    theme: ColorfulTheme,
    config_path: PathBuf,
}

impl DialoguerIo {
    pub fn new(config_path: PathBuf) -> Self {
        Self {
            term: Term::stderr(),
            theme: ColorfulTheme::default(),
            config_path,
        }
    }

    fn action(&self, step: Step) -> Result<bool, Error> {
        let mut items = vec!["Continue"];
        if step != Step::Environment {
            items.push("Back");
        }
        items.push("Cancel");
        let selected = Select::with_theme(&self.theme)
            .with_prompt(format!("Setup: {step:?}"))
            .items(&items)
            .default(0)
            .interact_on_opt(&self.term)
            .map_err(|error| Error::message(error.to_string()))?;
        match selected {
            None => Err(Error::message("cancel")),
            Some(0) => Ok(true),
            Some(index) if items[index] == "Back" => Ok(false),
            Some(_) => Err(Error::message("cancel")),
        }
    }

    fn text(&self, prompt: &str, default: String) -> Result<String, Error> {
        Input::<String>::with_theme(&self.theme)
            .with_prompt(prompt)
            .default(default)
            .interact_text_on(&self.term)
            .map_err(|error| Error::message(error.to_string()))
    }

    fn validated_text(
        &self,
        prompt: &str,
        default: String,
        validate: impl Fn(&str) -> Result<(), Error>,
    ) -> Result<String, Error> {
        loop {
            let value = self.text(prompt, default.clone())?;
            match validate(&value) {
                Ok(()) => return Ok(value),
                Err(error) => self
                    .term
                    .write_line(&format!("Invalid value: {error}"))
                    .map_err(|write| Error::message(write.to_string()))?,
            }
        }
    }

    fn limit(&self, name: &str, current: &Limit) -> Result<LimitAnswer, Error> {
        let default = match current {
            Limit::Known(_) => 0,
            Limit::Unknown => 1,
            Limit::Unlimited => 2,
        };
        let choice = Select::with_theme(&self.theme)
            .with_prompt(name)
            .items(["Known nonzero number", "Unknown", "Unlimited"])
            .default(default)
            .interact_on_opt(&self.term)
            .map_err(|error| Error::message(error.to_string()))?
            .ok_or_else(|| Error::message("cancel"))?;
        match choice {
            0 => {
                let default = match current {
                    Limit::Known(value) => value.get(),
                    _ => 1,
                };
                loop {
                    let value = Input::<u64>::with_theme(&self.theme)
                        .with_prompt(format!("{name} value"))
                        .default(default)
                        .interact_text_on(&self.term)
                        .map_err(|error| Error::message(error.to_string()))?;
                    match LimitAnswer::known(value) {
                        Ok(answer) => break Ok(answer),
                        Err(error) => self
                            .term
                            .write_line(&format!("Invalid value: {error}"))
                            .map_err(|write| Error::message(write.to_string()))?,
                    }
                }
            }
            1 => Ok(LimitAnswer::Unknown),
            _ => Ok(LimitAnswer::Unlimited),
        }
    }
}

impl PromptIo for DialoguerIo {
    fn prompt(&mut self, step: Step, draft: &SetupDraft) -> Result<Flow, Error> {
        match self.action(step) {
            Ok(false) => return Ok(Flow::Back),
            Err(error) if error.to_string() == "cancel" => return Ok(Flow::Cancel),
            Err(error) => return Err(error),
            Ok(true) => {}
        }
        match step {
            Step::Environment => {
                let choice = Select::with_theme(&self.theme)
                    .with_prompt("Environment preset (defaults only; protocol remains your choice)")
                    .items(["Local LLM", "Corporate LLM", "External API"])
                    .default(match draft.environment {
                        EnvironmentPreset::LocalLlm => 0,
                        EnvironmentPreset::Corporate => 1,
                        EnvironmentPreset::ExternalApi => 2,
                    })
                    .interact_on_opt(&self.term)
                    .map_err(|e| Error::message(e.to_string()))?;
                Ok(choice.map_or(Flow::Cancel, |v| {
                    Flow::SetEnvironment(
                        [
                            EnvironmentPreset::LocalLlm,
                            EnvironmentPreset::Corporate,
                            EnvironmentPreset::ExternalApi,
                        ][v],
                    )
                }))
            }
            Step::Connection => {
                let base = self.validated_text(
                    "Upstream API base (prefix preserved; /v1 is not duplicated)",
                    draft.config.upstream.api_base.to_string(),
                    |value| validate::api_base(value).map(|_| ()),
                )?;
                let generation_endpoints = [
                    Endpoint::ChatCompletions,
                    Endpoint::Responses,
                    Endpoint::Messages,
                ];
                let choices = ["Completions", "Responses", "Messages"];
                let defaults = generation_endpoints.map(|e| draft.endpoints().contains(&e));
                let selected = MultiSelect::with_theme(&self.theme)
                    .with_prompt("Actually supported endpoint formats; unknown is never enabled")
                    .items(choices)
                    .defaults(&defaults)
                    .interact_on_opt(&self.term)
                    .map_err(|e| Error::message(e.to_string()))?
                    .ok_or_else(|| Error::message("cancel"))?;
                let selected_generation = selected
                    .into_iter()
                    .map(|i| generation_endpoints[i])
                    .collect::<Vec<_>>();
                let mut endpoints = draft
                    .endpoints()
                    .iter()
                    .copied()
                    .filter(|endpoint| {
                        !generation_endpoints.contains(endpoint)
                            || selected_generation.contains(endpoint)
                    })
                    .collect::<Vec<_>>();
                for endpoint in selected_generation {
                    if !endpoints.contains(&endpoint) {
                        endpoints.push(endpoint);
                    }
                }
                if endpoints.is_empty() {
                    return Err(Error::message(
                        "select at least one actually supported endpoint",
                    ));
                }
                let auth_choice = Select::with_theme(&self.theme)
                    .with_prompt("Upstream auth")
                    .items(["Forward incoming header", "Environment reference", "None"])
                    .default(match draft.config.upstream.auth {
                        Auth::Forward => 0,
                        Auth::Env { .. } => 1,
                        Auth::None => 2,
                    })
                    .interact_on_opt(&self.term)
                    .map_err(|e| Error::message(e.to_string()))?
                    .ok_or_else(|| Error::message("cancel"))?;
                let (header_default, name_default) = match &draft.config.upstream.auth {
                    Auth::Env { header, name } => (header.clone(), name.clone()),
                    _ => ("authorization".into(), "LLMGW_UPSTREAM_AUTH".into()),
                };
                let auth = match auth_choice {
                    0 => Auth::Forward,
                    1 => Auth::Env {
                        header: self.text("Header name", header_default)?,
                        name: self.text(
                            "Environment variable (complete header value; secret is never copied)",
                            name_default,
                        )?,
                    },
                    _ => Auth::None,
                };
                let mut proxy = draft
                    .config
                    .upstream
                    .proxy
                    .as_ref()
                    .map(ToString::to_string);
                let mut ca_bundle = draft.config.upstream.ca_bundle.clone();
                if draft.environment == EnvironmentPreset::Corporate {
                    if Confirm::with_theme(&self.theme)
                        .with_prompt("Use an explicit upstream proxy? Loopback always bypasses it")
                        .default(proxy.is_some())
                        .interact_on_opt(&self.term)
                        .map_err(|e| Error::message(e.to_string()))?
                        .ok_or_else(|| Error::message("cancel"))?
                    {
                        proxy = Some(self.validated_text(
                            "Proxy URL (credentials are not accepted)",
                            proxy.unwrap_or_else(|| "http://proxy.example:8080".into()),
                            |value| validate::proxy(value).map(|_| ()),
                        )?);
                    } else {
                        proxy = None;
                    }
                    if Confirm::with_theme(&self.theme)
                        .with_prompt("Merge an explicit PEM CA bundle with system trust?")
                        .default(ca_bundle.is_some())
                        .interact_on_opt(&self.term)
                        .map_err(|e| Error::message(e.to_string()))?
                        .ok_or_else(|| Error::message("cancel"))?
                    {
                        ca_bundle = Some(PathBuf::from(
                            self.text(
                                "PEM CA bundle path",
                                ca_bundle
                                    .map_or_else(String::new, |p| p.to_string_lossy().into_owned()),
                            )?,
                        ));
                    } else {
                        ca_bundle = None;
                    }
                }
                Ok(Flow::SetConnection(ConnectionAnswers {
                    api_base: base,
                    endpoints,
                    auth,
                    proxy,
                    ca_bundle,
                }))
            }
            Step::Models => {
                if draft.original.is_some()
                    && Confirm::with_theme(&self.theme)
                        .with_prompt("Keep all existing models and routes unchanged?")
                        .default(true)
                        .interact_on_opt(&self.term)
                        .map_err(|e| Error::message(e.to_string()))?
                        .ok_or_else(|| Error::message("cancel"))?
                {
                    return Ok(Flow::Keep);
                }
                self.term.write_line("Model listing is an optional bounded GET only; it does not verify inference, tools, vision, or reasoning.").map_err(|e| Error::message(e.to_string()))?;
                let listed = Confirm::with_theme(&self.theme)
                    .with_prompt("Try one bounded GET /models against this upstream now?")
                    .default(false)
                    .interact_on_opt(&self.term)
                    .map_err(|e| Error::message(e.to_string()))?
                    .ok_or_else(|| Error::message("cancel"))?;
                let mut listing_verified = false;
                let id = if listed {
                    let runtime = tokio::runtime::Builder::new_current_thread()
                        .enable_all()
                        .build()
                        .map_err(|e| Error::message(e.to_string()))?;
                    match runtime.block_on(fetch_models(&draft.config.upstream)) {
                        Ok(ModelListing::Available(models)) if !models.is_empty() => {
                            let selected = Select::with_theme(&self.theme)
                                .with_prompt(
                                    "Model ID from listing (listing does not verify capabilities)",
                                )
                                .items(&models)
                                .default(0)
                                .interact_on_opt(&self.term)
                                .map_err(|e| Error::message(e.to_string()))?;
                            let Some(selected) = selected else {
                                return Ok(Flow::Cancel);
                            };
                            listing_verified = true;
                            models[selected].clone()
                        }
                        Ok(ModelListing::Available(_)) => {
                            self.term
                                .write_line(
                                    "Model listing returned no model IDs; enter one manually.",
                                )
                                .map_err(|e| Error::message(e.to_string()))?;
                            self.text(
                                "Manual model ID",
                                draft
                                    .config
                                    .models
                                    .first()
                                    .map_or_else(|| "model-id".into(), |m| m.id.clone()),
                            )?
                        }
                        Ok(ModelListing::Unavailable { reason, .. }) | Err(Error(reason)) => {
                            self.term
                                .write_line(&reason)
                                .map_err(|e| Error::message(e.to_string()))?;
                            self.text(
                                "Manual model ID",
                                draft
                                    .config
                                    .models
                                    .first()
                                    .map_or_else(|| "model-id".into(), |m| m.id.clone()),
                            )?
                        }
                    }
                } else {
                    self.text(
                        "Manual model ID",
                        draft
                            .config
                            .models
                            .first()
                            .map_or_else(|| "model-id".into(), |m| m.id.clone()),
                    )?
                };
                let existing_model = draft.config.models.iter().find(|model| model.id == id);
                let current_cap = existing_model.and_then(|model| model.max_output_tokens);
                let set_fallback = Confirm::with_theme(&self.theme)
                    .with_prompt("Set a user-chosen reservation fallback for requests that omit an output cap? This is not a verified provider limit")
                    .default(current_cap.is_some())
                    .interact_on_opt(&self.term)
                    .map_err(|e| Error::message(e.to_string()))?
                    .ok_or_else(|| Error::message("cancel"))?;
                let cap = if set_fallback {
                    Some(
                        self.validated_text(
                            "Nonzero reservation fallback (the gateway does not inject or enforce it upstream)",
                            current_cap.map_or(1024, NonZeroU64::get).to_string(),
                            |value| match value.parse::<u64>() {
                                Ok(value) if value > 0 => Ok(()),
                                _ => Err(Error::message(
                                    "reservation output fallback must be a nonzero number",
                                )),
                            },
                        )?
                        .parse::<u64>()
                        .expect("validated number"),
                    )
                } else {
                    None
                };
                use crate::input_estimate::InputEstimator;
                let modes = [
                    InputEstimator::Utf8Bytes,
                    InputEstimator::Cl100kBase,
                    InputEstimator::O200kBase,
                ];
                let current_mode = existing_model
                    .map_or_else(InputEstimator::default, |model| model.input_estimator);
                self.term.write_line("BPE counts serialized JSON, not exact provider input. Match your upstream encoding and framing; OpenAI-compatible APIs may use different tokenizers. Selected BPE adds vocabulary memory (roughly 32/67 MiB in a standalone probe).").map_err(|e| Error::message(e.to_string()))?;
                let selected = Select::with_theme(&self.theme)
                    .with_prompt("Input estimate for known TPM")
                    .items([
                        "UTF-8 bytes (default, no vocabulary)",
                        "cl100k_base (explicit BPE estimate)",
                        "o200k_base (explicit BPE estimate)",
                    ])
                    .default(
                        modes
                            .iter()
                            .position(|mode| *mode == current_mode)
                            .unwrap_or(0),
                    )
                    .interact_on_opt(&self.term)
                    .map_err(|e| Error::message(e.to_string()))?
                    .ok_or_else(|| Error::message("cancel"))?;
                let input_estimator = modes[selected];
                let input_token_overhead = if input_estimator == InputEstimator::Utf8Bytes {
                    0
                } else {
                    let current = existing_model
                        .filter(|model| model.input_estimator == input_estimator)
                        .map_or(input_estimator.default_overhead(), |model| {
                            model.input_token_overhead
                        });
                    self.validated_text(
                        "Extra input tokens for server framing (estimate, not an upper bound)",
                        current.to_string(),
                        validate_input_token_overhead,
                    )?
                    .parse()
                    .expect("validated overhead")
                };
                Ok(Flow::SetModels(ModelAnswers {
                    id,
                    max_output_tokens: cap,
                    input_estimator: Some(input_estimator),
                    input_token_overhead: Some(input_token_overhead),
                    listing_verified,
                    capabilities_verified: false,
                }))
            }
            Step::Quota => {
                let rpm = self.limit("RPM", &draft.config.quota.rpm)?;
                let tpm = self.limit("TPM", &draft.config.quota.tpm)?;
                let shared_with_other_pcs = Confirm::with_theme(&self.theme)
                    .with_prompt("Is this same quota shared by other PCs?")
                    .default(draft.shared_with_other_pcs)
                    .interact_on_opt(&self.term)
                    .map_err(|e| Error::message(e.to_string()))?
                    .ok_or_else(|| Error::message("cancel"))?;
                let separate_input_output = Confirm::with_theme(&self.theme).with_prompt("Does the provider specify separate input/output limits? (v0.1 reports this as estimated)").default(draft.separate_input_output).interact_on_opt(&self.term).map_err(|e| Error::message(e.to_string()))?.ok_or_else(|| Error::message("cancel"))?;
                let concurrency = loop {
                    let value = Input::<u8>::with_theme(&self.theme)
                        .with_prompt("Concurrency 1..16")
                        .default(draft.config.concurrency)
                        .interact_text_on(&self.term)
                        .map_err(|e| Error::message(e.to_string()))?;
                    if (crate::config::MIN_CONCURRENCY..=crate::config::MAX_CONCURRENCY)
                        .contains(&value)
                    {
                        break value;
                    }
                    self.term
                        .write_line("Invalid value: concurrency must be between 1 and 16")
                        .map_err(|e| Error::message(e.to_string()))?;
                };
                let startup_hold_secs = self
                    .validated_text(
                        "Startup quota hold in seconds (0 disables, maximum 3600)",
                        draft.config.startup_hold_secs.to_string(),
                        |value| match value.parse::<u64>() {
                            Ok(value) if value <= 3600 => Ok(()),
                            _ => Err(Error::message(
                                "startup hold must be between 0 and 3600 seconds",
                            )),
                        },
                    )?
                    .parse()
                    .expect("validated startup hold");
                let cache_enabled = Confirm::with_theme(&self.theme)
                    .with_prompt("Enable exact response cache for short text requests?")
                    .default(draft.config.cache.is_some())
                    .interact_on_opt(&self.term)
                    .map_err(|e| Error::message(e.to_string()))?
                    .ok_or_else(|| Error::message("cancel"))?;
                let cache = if cache_enabled {
                    let previous = draft.config.cache.unwrap_or_default();
                    let ttl_secs = self
                        .validated_text(
                            "Cache TTL in seconds (1..3600)",
                            previous.ttl_secs.to_string(),
                            |value| match value.parse::<u64>() {
                                Ok(value) if (1..=3600).contains(&value) => Ok(()),
                                _ => Err(Error::message("cache TTL must be 1..3600")),
                            },
                        )?
                        .parse()
                        .expect("validated cache TTL");
                    let max_history = self
                        .validated_text(
                            "Cache maximum message history (1..64)",
                            previous.max_history.to_string(),
                            |value| match value.parse::<usize>() {
                                Ok(value) if (1..=64).contains(&value) => Ok(()),
                                _ => Err(Error::message("cache history must be 1..64")),
                            },
                        )?
                        .parse()
                        .expect("validated cache history");
                    Some(crate::config::CacheConfig {
                        ttl_secs,
                        max_history,
                    })
                } else {
                    None
                };
                Ok(Flow::SetQuota(QuotaAnswers {
                    rpm,
                    tpm,
                    shared_with_other_pcs,
                    separate_input_output,
                    concurrency,
                    startup_hold_secs,
                    cache,
                }))
            }
            Step::Run => {
                let port = loop {
                    let value = Input::<u16>::with_theme(&self.theme)
                        .with_prompt("Local loopback port")
                        .default(draft.config.listen.port())
                        .interact_text_on(&self.term)
                        .map_err(|e| Error::message(e.to_string()))?;
                    if value > 0 {
                        break value;
                    }
                    self.term
                        .write_line("Invalid value: listen port must be between 1 and 65535")
                        .map_err(|e| Error::message(e.to_string()))?;
                };
                let login_requested = Confirm::with_theme(&self.theme).with_prompt("Start at next user login? After the config is saved, the exact OS registration target and argv will be previewed for separate confirmation").default(draft.login_requested).interact_on_opt(&self.term).map_err(|e| Error::message(e.to_string()))?.ok_or_else(|| Error::message("cancel"))?;
                Ok(Flow::SetRun(RunAnswers {
                    port,
                    login_requested,
                }))
            }
            Step::Tools => {
                let defaults = [
                    ClientIntent::Pi,
                    ClientIntent::ClaudeCode,
                    ClientIntent::Codex,
                    ClientIntent::Manual,
                ]
                .map(|client| draft.clients.contains(&client));
                let selected = MultiSelect::with_theme(&self.theme)
                    .with_prompt("Client intents (pending only; no client file is changed)")
                    .items(["Pi", "Claude Code", "Codex", "Manual"])
                    .defaults(&defaults)
                    .interact_on_opt(&self.term)
                    .map_err(|e| Error::message(e.to_string()))?
                    .ok_or_else(|| Error::message("cancel"))?;
                let clients = selected
                    .into_iter()
                    .map(|i| {
                        [
                            ClientIntent::Pi,
                            ClientIntent::ClaudeCode,
                            ClientIntent::Codex,
                            ClientIntent::Manual,
                        ][i]
                    })
                    .collect();
                Ok(Flow::SetTools(ToolAnswers { clients }))
            }
            Step::Apply => {
                let runtime_impact = runtime_impact(&self.config_path, draft)?;
                self.term
                    .write_line(&draft.summary_with_runtime(&self.config_path, &runtime_impact)?)
                    .map_err(|e| Error::message(e.to_string()))?;
                let selected = Select::with_theme(&self.theme)
                    .with_prompt("Apply")
                    .items(["Save and start", "Save only", "Back", "Cancel"])
                    .default(1)
                    .interact_on_opt(&self.term)
                    .map_err(|e| Error::message(e.to_string()))?;
                Ok(match selected {
                    Some(0) => Flow::Apply(runtime_impact.save_and_start_mode()),
                    Some(1) => Flow::Apply(ApplyMode::SaveOnly),
                    Some(2) => Flow::Back,
                    _ => Flow::Cancel,
                })
            }
        }
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn overhead_prompt_accepts_only_storable_nonnegative_toml_integers() {
        for value in ["0", "+0", "32", "9223372036854775807"] {
            assert!(super::validate_input_token_overhead(value).is_ok());
            assert!(value.parse::<u64>().expect("accepted prompt must parse") <= i64::MAX as u64);
        }
        for value in [
            "-0",
            "-1",
            "9223372036854775808",
            "18446744073709551615",
            "0.5",
            "",
        ] {
            assert!(
                super::validate_input_token_overhead(value).is_err(),
                "{value}"
            );
        }
    }
}
