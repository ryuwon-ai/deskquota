//! Native setup contract. All network and process checks use owned loopback fixtures.
use llmgw::config::{Accounting, Auth, CancelPolicy, Endpoint, Limit, LoadedConfig, StatePaths};
use llmgw::setup::{
    ApplyMode, ClientIntent, ConnectionAnswers, EnvironmentPreset, Flow, LimitAnswer, ModelAnswers,
    ModelListing, PromptIo, QuotaAnswers, RunAnswers, RuntimeImpact, SetupDraft, SetupOutcome,
    Step, ToolAnswers, apply, example_urls, list_models, run_wizard, runtime_impact,
};
use std::{
    collections::VecDeque,
    fs,
    net::TcpListener,
    path::PathBuf,
    process::Command,
    sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    },
};
#[path = "support/private_fs.rs"]
mod private_fs;
mod support;
use support::fixture::{response_body, send_raw, status};

static NEXT: AtomicUsize = AtomicUsize::new(0);

struct Temp {
    dir: PathBuf,
}

impl Temp {
    fn new(label: &str) -> Self {
        let dir = std::env::temp_dir().join(format!(
            "llmgw-setup-{}-{}-{label} 한글 space",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir_all(&dir).unwrap();
        Self { dir }
    }

    fn config(&self) -> PathBuf {
        self.dir.join("gateway 설정.toml")
    }
}

impl Drop for Temp {
    fn drop(&mut self) {
        let config = self.config();
        if config.exists() {
            let _ = Command::new(env!("CARGO_BIN_EXE_llmgw"))
                .args(["off", "--config"])
                .arg(&config)
                .output();
        }
        let _ = fs::remove_dir_all(&self.dir);
    }
}

fn draft(port: u16) -> SetupDraft {
    SetupDraft::new(EnvironmentPreset::ExternalApi)
        .connection(ConnectionAnswers {
            api_base: "http://127.0.0.1:9/prefix/v1".into(),
            endpoints: vec![Endpoint::Responses],
            auth: Auth::None,
            proxy: None,
            ca_bundle: None,
        })
        .models(ModelAnswers::manual("model-a", Some(64)))
        .quota(QuotaAnswers {
            rpm: LimitAnswer::Unlimited,
            tpm: LimitAnswer::Unknown,
            shared_with_other_pcs: false,
            separate_input_output: false,
            concurrency: 1,
            startup_hold_secs: 60,
            cache: None,
        })
        .run(RunAnswers {
            port,
            login_requested: false,
        })
        .tools(ToolAnswers {
            clients: vec![ClientIntent::Pi, ClientIntent::Manual],
        })
}

#[test]
fn client_route_is_added_without_rewriting_existing_routes_or_models() {
    let original = draft(4141);
    let original_root = original.config().roots[0].clone();
    let updated = original
        .ensure_client_route(
            "codex-work",
            "responses-model",
            llmgw::config::Endpoint::Responses,
        )
        .unwrap();
    assert_eq!(updated.config().roots[0], original_root);
    assert!(
        updated
            .config()
            .roots
            .iter()
            .any(|root| root.id == "codex-work"
                && root.endpoints == [llmgw::config::Endpoint::Responses]
                && root.models == ["responses-model"])
    );
    assert!(
        updated
            .config()
            .models
            .iter()
            .any(|model| model.id == "responses-model" && model.max_output_tokens.is_none())
    );
}

fn gateway_status(config: &std::path::Path) -> serde_json::Value {
    let output = Command::new(env!("CARGO_BIN_EXE_llmgw"))
        .args(["status", "--json", "--config"])
        .arg(config)
        .output()
        .unwrap();
    assert!(output.status.success(), "{output:?}");
    serde_json::from_slice(&output.stdout).unwrap()
}

#[test]
fn presets_only_supply_defaults_and_never_claim_protocol_or_local_start() {
    let local = SetupDraft::new(EnvironmentPreset::LocalLlm);
    let corporate = SetupDraft::new(EnvironmentPreset::Corporate);
    let external = SetupDraft::new(EnvironmentPreset::ExternalApi);

    assert_eq!(local.concurrency(), 1);
    assert!(!local.login_requested());
    assert!(local.endpoints().is_empty());
    assert!(!local.local_model_started_or_downloaded());
    assert!(corporate.endpoints().is_empty());
    assert!(external.endpoints().is_empty());
    assert_eq!(local.accounting(), Accounting::Actual);
    assert!(!local.retry_transient_429());
}

#[test]
fn quota_known_unknown_and_unlimited_are_typed_and_zero_is_rejected() {
    assert_eq!(
        LimitAnswer::known(12).unwrap().to_limit(),
        Limit::Known(12.try_into().unwrap())
    );
    assert_eq!(LimitAnswer::Unknown.to_limit(), Limit::Unknown);
    assert_eq!(LimitAnswer::Unlimited.to_limit(), Limit::Unlimited);
    assert!(LimitAnswer::known(0).is_err());
    assert!(
        QuotaAnswers {
            rpm: LimitAnswer::Unlimited,
            tpm: LimitAnswer::Unknown,
            shared_with_other_pcs: true,
            separate_input_output: true,
            concurrency: 17,
            startup_hold_secs: 60,
            cache: None,
        }
        .validate()
        .is_err()
    );
}

#[test]
fn invalid_base_is_rejected_before_model_listing_network_call() {
    let calls = Arc::new(AtomicUsize::new(0));
    let observed = Arc::clone(&calls);
    let result = list_models(
        "vendor.example/not-absolute",
        &Auth::None,
        move |_request| {
            observed.fetch_add(1, Ordering::SeqCst);
            unreachable!("invalid URL must not reach transport")
        },
    );
    assert!(result.unwrap_err().to_string().contains("absolute URL"));
    assert_eq!(calls.load(Ordering::SeqCst), 0);
}

#[test]
fn typed_draft_preserves_invalid_api_base_error_until_validation() {
    let invalid = draft(4141).connection(ConnectionAnswers {
        api_base: "vendor.example/not-absolute".into(),
        endpoints: vec![Endpoint::Responses],
        auth: Auth::None,
        proxy: None,
        ca_bundle: None,
    });
    let error = invalid.validate().unwrap_err().to_string();
    assert!(error.contains("absolute URL"), "{error}");
}

#[test]
fn typed_draft_preserves_invalid_proxy_error_instead_of_dropping_proxy() {
    let invalid = draft(4141).connection(ConnectionAnswers {
        api_base: "https://example.invalid/v1".into(),
        endpoints: vec![Endpoint::Responses],
        auth: Auth::None,
        proxy: Some("proxy.example:8080".into()),
        ca_bundle: None,
    });
    let error = invalid.validate().unwrap_err().to_string();
    assert!(error.contains("proxy must"), "{error}");
}

#[test]
fn missing_env_is_distinct_from_valid_static_configuration_and_redacts_value() {
    let name = format!("LLMGW_SETUP_MISSING_{}", std::process::id());
    let auth = Auth::Env {
        header: "authorization".into(),
        name: name.clone(),
    };
    let error = llmgw::setup::validate::available_auth(&auth).unwrap_err();
    assert!(error.to_string().contains(&name));
    assert!(error.to_string().contains("complete header value"));
    assert!(!error.to_string().contains("Bearer"));
}

#[test]
fn listing_404_is_explained_and_manual_model_can_still_be_saved() {
    let result = list_models("https://fixture.invalid/v1", &Auth::None, |_request| {
        Ok((404, br#"{"error":"missing"}"#.to_vec()))
    })
    .unwrap();
    assert!(matches!(
        result,
        ModelListing::Unavailable {
            status: Some(404),
            ..
        }
    ));

    let temp = Temp::new("listing404");
    let outcome = apply(&temp.config(), &draft(4141), ApplyMode::SaveOnly).unwrap();
    assert_eq!(outcome.mode, ApplyMode::SaveOnly);
    let loaded = LoadedConfig::load(temp.config()).unwrap();
    assert_eq!(loaded.config.models[0].id, "model-a");
}

#[derive(Default)]
struct Script {
    responses: VecDeque<(Step, Flow)>,
    visited: Vec<Step>,
}

impl Script {
    fn with(responses: impl IntoIterator<Item = (Step, Flow)>) -> Self {
        Self {
            responses: responses.into_iter().collect(),
            visited: Vec::new(),
        }
    }
}

impl PromptIo for Script {
    fn prompt(&mut self, step: Step, _draft: &SetupDraft) -> Result<Flow, llmgw::setup::Error> {
        self.visited.push(step);
        let (expected, answer) = self.responses.pop_front().expect("script answer");
        assert_eq!(step, expected);
        Ok(answer)
    }
}

#[test]
fn scripted_wizard_supports_back_and_cancel_without_writes() {
    let temp = Temp::new("cancel");
    let config = fs::canonicalize(&temp.dir)
        .unwrap()
        .join("gateway 설정.toml");
    let state = StatePaths::from_config_path(&config).unwrap();
    let sentinel = temp.dir.join("existing.txt");
    fs::write(&sentinel, "unchanged").unwrap();
    let before: Vec<_> = fs::read_dir(&temp.dir)
        .unwrap()
        .map(|e| e.unwrap().file_name())
        .collect();
    let mut io = Script::with([
        (
            Step::Environment,
            Flow::SetEnvironment(EnvironmentPreset::Corporate),
        ),
        (Step::Connection, Flow::Back),
        (
            Step::Environment,
            Flow::SetEnvironment(EnvironmentPreset::LocalLlm),
        ),
        (Step::Connection, Flow::Cancel),
    ]);

    let outcome = run_wizard(&mut io, None).unwrap();
    assert_eq!(outcome, SetupOutcome::Cancelled);
    assert_eq!(fs::read_to_string(sentinel).unwrap(), "unchanged");
    let after: Vec<_> = fs::read_dir(&temp.dir)
        .unwrap()
        .map(|e| e.unwrap().file_name())
        .collect();
    assert_eq!(before, after);
    assert!(!config.exists());
    assert!(!state.directory.exists());
    assert!(!state.directory.join("data-token").exists());
    assert!(!state.control_token.exists());
}

#[test]
fn scripted_wizard_accepts_local_corporate_and_external_presets() {
    for preset in [
        EnvironmentPreset::LocalLlm,
        EnvironmentPreset::Corporate,
        EnvironmentPreset::ExternalApi,
    ] {
        let mut io = Script::with([
            (Step::Environment, Flow::SetEnvironment(preset)),
            (
                Step::Connection,
                Flow::SetConnection(ConnectionAnswers {
                    api_base: "https://example.invalid/prefix/v1".into(),
                    endpoints: vec![Endpoint::Responses],
                    auth: Auth::None,
                    proxy: None,
                    ca_bundle: None,
                }),
            ),
            (
                Step::Models,
                Flow::SetModels(ModelAnswers::manual("model-a", Some(64))),
            ),
            (
                Step::Quota,
                Flow::SetQuota(QuotaAnswers {
                    rpm: LimitAnswer::Known(3.try_into().unwrap()),
                    tpm: LimitAnswer::Unlimited,
                    shared_with_other_pcs: false,
                    separate_input_output: false,
                    concurrency: 1,
                    startup_hold_secs: 60,
                    cache: None,
                }),
            ),
            (
                Step::Run,
                Flow::SetRun(RunAnswers {
                    port: 4141,
                    login_requested: false,
                }),
            ),
            (Step::Tools, Flow::SetTools(ToolAnswers { clients: vec![] })),
            (Step::Apply, Flow::Apply(ApplyMode::SaveOnly)),
        ]);
        assert!(matches!(
            run_wizard(&mut io, None).unwrap(),
            SetupOutcome::Ready {
                mode: ApplyMode::SaveOnly,
                ..
            }
        ));
        assert_eq!(io.visited.len(), 7);
    }
}

#[test]
fn rerun_preserves_multiroot_models_retry_and_accounting_until_explicitly_changed() {
    let temp = Temp::new("preserve");
    let raw = r#"listen = "127.0.0.1:4200"
concurrency = 3
cancel_policy = "close"
accounting = "actual"
retry_transient_429 = true
[upstream]
api_base = "https://example.invalid/company/v1"
[upstream.auth]
mode = "forward"
[quota.rpm]
kind = "known"
value = 7
[quota.tpm]
kind = "known"
value = 800
[[models]]
id = "a"
max_output_tokens = 20
[[models]]
id = "b"
max_output_tokens = 30
[[roots]]
id = "pi"
endpoints = ["responses"]
models = ["a"]
[[roots]]
id = "claude"
endpoints = ["messages"]
models = ["b"]
"#;
    fs::write(temp.config(), raw).unwrap();
    let existing = LoadedConfig::load(temp.config()).unwrap();
    let rerun = SetupDraft::from_existing(&existing.config);
    let rendered = rerun.render_config().unwrap();
    let parsed = llmgw::config::parse(rendered.as_bytes()).unwrap();
    assert_eq!(parsed, existing.config);
    assert_eq!(parsed.accounting, Accounting::Actual);
    assert!(parsed.retry_transient_429);
    assert_eq!(parsed.cancel_policy, CancelPolicy::Close);
    assert_eq!(parsed.models.len(), 2);
    assert_eq!(parsed.roots.len(), 2);
}

#[test]
fn rerun_noop_preserves_exact_bytes_and_scalar_edit_preserves_inline_comment() {
    let temp = Temp::new("document-preserve");
    let raw = r#"# user heading
listen = "127.0.0.1:4200" # keep port comment
concurrency = 1
cancel_policy = "drain"
accounting = "reserved"
retry_transient_429 = false
[upstream]
api_base = "https://example.invalid/v1" # keep upstream comment
[upstream.auth]
mode = "none"
[quota.rpm]
kind = "unlimited"
[quota.tpm]
kind = "unknown"
[[models]]
id = "a"
max_output_tokens = 20
[[roots]]
id = "pi"
endpoints = ["responses"]
models = ["a"]
"#;
    fs::write(temp.config(), raw).unwrap();
    let loaded = LoadedConfig::load(temp.config()).unwrap();
    let noop = SetupDraft::from_loaded(&loaded).unwrap();
    assert_eq!(noop.render_config().unwrap().as_bytes(), raw.as_bytes());
    let edited = SetupDraft::from_loaded(&loaded).unwrap().run(RunAnswers {
        port: 4201,
        login_requested: false,
    });
    let rendered = edited.render_config().unwrap();
    assert!(rendered.contains("# user heading"));
    assert!(rendered.contains("# keep port comment"));
    assert!(rendered.contains("# keep upstream comment"));
    assert_eq!(
        llmgw::config::parse(rendered.as_bytes())
            .unwrap()
            .listen
            .port(),
        4201
    );
}

#[test]
fn endpoint_and_model_edits_preserve_unrelated_document_comments_and_routes() {
    let temp = Temp::new("document-structured-edits");
    let raw = r#"# preserved user heading
listen = "127.0.0.1:4200" # keep port comment
concurrency = 3
cancel_policy = "close"
accounting = "actual"
retry_transient_429 = true
[upstream]
api_base = "https://example.invalid/company/v1"
auth = { mode = "none" }
[quota]
rpm = { kind = "unlimited" }
tpm = { kind = "unknown" }
[[models]]
id = "model-a" # keep model comment
max_output_tokens = 20
[[models]]
id = "model-b"
max_output_tokens = 30
[[roots]]
id = "pi"
endpoints = ["responses"] # keep endpoint comment
models = ["model-a"]
[[roots]]
id = "claude" # keep second route comment
endpoints = [
  # keep second endpoint array note
  "messages",
]
models = [
  # keep second models array note
  "model-b",
]
"#;
    fs::write(temp.config(), raw).unwrap();
    let loaded = LoadedConfig::load(temp.config()).unwrap();

    let endpoint_edit = SetupDraft::from_loaded(&loaded)
        .unwrap()
        .connection(ConnectionAnswers {
            api_base: "https://example.invalid/company/v1".into(),
            endpoints: vec![Endpoint::Responses, Endpoint::ChatCompletions],
            auth: Auth::None,
            proxy: None,
            ca_bundle: None,
        });
    let endpoint_rendered = endpoint_edit.render_config().unwrap();
    assert!(endpoint_rendered.contains("# preserved user heading"));
    assert!(endpoint_rendered.contains("# keep port comment"));
    assert!(endpoint_rendered.contains("# keep endpoint comment"));
    assert!(endpoint_rendered.contains("# keep second route comment"));
    assert!(endpoint_rendered.contains("# keep second endpoint array note"));
    assert!(endpoint_rendered.contains("# keep second models array note"));
    let endpoint_config = llmgw::config::parse(endpoint_rendered.as_bytes()).unwrap();
    assert_eq!(
        endpoint_config.roots[0].endpoints,
        vec![Endpoint::Responses, Endpoint::ChatCompletions]
    );
    assert_eq!(endpoint_config.roots[1], loaded.config.roots[1]);

    let model_temp = Temp::new("document-model-edit");
    let model_raw = r#"# preserved model file heading
listen = "127.0.0.1:4200" # keep model-file port comment
[upstream]
api_base = "https://example.invalid/company/v1"
auth = { mode = "none" }
[quota]
rpm = { kind = "unlimited" }
tpm = { kind = "unknown" }
[[models]]
id = "model-a" # keep model comment
max_output_tokens = 20
[[roots]]
id = "pi"
endpoints = ["responses"] # keep model route comment
models = ["model-a"]
"#;
    fs::write(model_temp.config(), model_raw).unwrap();
    let model_loaded = LoadedConfig::load(model_temp.config()).unwrap();
    let model_edit = SetupDraft::from_loaded(&model_loaded)
        .unwrap()
        .models(ModelAnswers::manual("model-next", Some(48)));
    let model_rendered = model_edit.render_config().unwrap();
    assert!(model_rendered.contains("# preserved model file heading"));
    assert!(model_rendered.contains("# keep model-file port comment"));
    assert!(model_rendered.contains("# keep model comment"));
    assert!(model_rendered.contains("# keep model route comment"));
    let model_config = llmgw::config::parse(model_rendered.as_bytes()).unwrap();
    assert_eq!(model_config.models.len(), 1);
    assert_eq!(model_config.models[0].id, "model-next");
    assert_eq!(model_config.models[0].max_output_tokens.unwrap().get(), 48);
    assert_eq!(model_config.roots[0].models, ["model-next"]);
}

#[test]
fn structured_edits_preserve_inline_model_and_root_table_forms() {
    let temp = Temp::new("document-inline-collections");
    let raw = r#"# keep inline collection heading
listen = "127.0.0.1:4200"
models = [{ id = "model-a", max_output_tokens = 20 }]
roots = [{ id = "pi", endpoints = ["responses"], models = ["model-a"] }]
[upstream]
api_base = "https://example.invalid/v1"
auth = { mode = "none" }
[quota]
rpm = { kind = "unlimited" }
tpm = { kind = "unknown" }
"#;
    fs::write(temp.config(), raw).unwrap();
    let loaded = LoadedConfig::load(temp.config()).unwrap();
    let edited = SetupDraft::from_loaded(&loaded)
        .unwrap()
        .connection(ConnectionAnswers {
            api_base: "https://example.invalid/v1".into(),
            endpoints: vec![Endpoint::Responses, Endpoint::Models],
            auth: Auth::None,
            proxy: None,
            ca_bundle: None,
        })
        .models(ModelAnswers::manual("model-next", Some(48)));
    let rendered = edited.render_config().unwrap();
    assert!(rendered.contains("# keep inline collection heading"));
    assert!(rendered.contains("models = [{ id = \"model-next\", max_output_tokens = 48 }]"));
    assert!(rendered.contains("endpoints = [\"responses\", \"models\"]"));
    let parsed = llmgw::config::parse(rendered.as_bytes()).unwrap();
    assert_eq!(parsed.models[0].id, "model-next");
    assert_eq!(parsed.roots[0].models, ["model-next"]);
    assert_eq!(
        parsed.roots[0].endpoints,
        [Endpoint::Responses, Endpoint::Models]
    );
}

#[test]
fn summary_shows_exact_quota_limits_and_concurrency_before_apply() {
    let temp = Temp::new("quota-summary");
    let configured = draft(4141).quota(QuotaAnswers {
        rpm: LimitAnswer::known(18).unwrap(),
        tpm: LimitAnswer::known(450_000).unwrap(),
        concurrency: 3,
        shared_with_other_pcs: false,
        separate_input_output: false,
        startup_hold_secs: 60,
        cache: None,
    });
    let summary = configured.summary(&temp.config()).unwrap();
    assert!(summary.contains("RPM: 18 (local rolling 60-second cap); TPM: 450000 (local rolling 60-second cap); concurrency: 3"));
    let summary = draft(4141).summary(&temp.config()).unwrap();
    assert!(summary.contains("RPM: unlimited (explicitly no local quota cap); TPM: unknown (defer to upstream; no local cap; upstream limit unverified); concurrency: 1"));
    assert!(summary.contains("concurrency and shared upstream 429 cooldown still apply"));
    assert!(!temp.config().exists());
}

#[test]
fn valid_request_bounded_model_needs_no_invented_fallback() {
    let temp = Temp::new("request-bounded");
    let configured = draft(4141).models(ModelAnswers::manual("model-a", None));
    let summary = configured.summary(&temp.config()).unwrap();
    assert!(summary.contains("reservation output bound: none"));
    assert!(summary.contains("known TPM generation admission requires one"));
    apply(&temp.config(), &configured, ApplyMode::SaveOnly).unwrap();
    assert_eq!(
        LoadedConfig::load(temp.config()).unwrap().config.models[0].max_output_tokens,
        None
    );
}

#[test]
fn apply_rejects_a_stale_preview_and_preserves_the_concurrent_edit() {
    let temp = Temp::new("stale-preview");
    apply(&temp.config(), &draft(4141), ApplyMode::SaveOnly).unwrap();
    let loaded = LoadedConfig::load(temp.config()).unwrap();
    let edited = SetupDraft::from_loaded(&loaded).unwrap().run(RunAnswers {
        port: 4142,
        login_requested: false,
    });
    let concurrent = String::from_utf8(loaded.source_bytes)
        .unwrap()
        .replace("concurrency = 1", "concurrency = 2");
    fs::write(temp.config(), &concurrent).unwrap();
    let error = apply(&temp.config(), &edited, ApplyMode::SaveOnly)
        .unwrap_err()
        .to_string();
    assert!(error.contains("changed since setup loaded"), "{error}");
    assert_eq!(fs::read_to_string(temp.config()).unwrap(), concurrent);
}

#[test]
fn setup_writer_lock_rejects_a_second_cooperating_apply() {
    use fs4::FileExt;
    let temp = Temp::new("writer-lock");
    apply(&temp.config(), &draft(4141), ApplyMode::SaveOnly).unwrap();
    let lock_path = temp.dir.join(".gateway 설정.toml.setup.lock");
    let lock = fs::OpenOptions::new()
        .read(true)
        .write(true)
        .open(lock_path)
        .unwrap();
    FileExt::lock(&lock).unwrap();
    let loaded = LoadedConfig::load(temp.config()).unwrap();
    let error = apply(
        &temp.config(),
        &SetupDraft::from_loaded(&loaded).unwrap(),
        ApplyMode::SaveOnly,
    )
    .unwrap_err()
    .to_string();
    assert!(
        error.contains("another setup apply is in progress"),
        "{error}"
    );
}

#[test]
fn unchanged_save_and_start_keeps_the_authenticated_worker_identity() {
    let temp = Temp::new("unchanged-running");
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    drop(listener);
    apply(&temp.config(), &draft(port), ApplyMode::SaveOnly).unwrap();
    let started = Command::new(env!("CARGO_BIN_EXE_llmgw"))
        .args(["on", "--config"])
        .arg(temp.config())
        .output()
        .unwrap();
    assert!(started.status.success(), "{started:?}");
    let before = gateway_status(&temp.config());
    let loaded = LoadedConfig::load(temp.config()).unwrap();
    let result = apply(
        &temp.config(),
        &SetupDraft::from_loaded(&loaded).unwrap(),
        ApplyMode::SaveAndStart,
    )
    .unwrap();
    let after = gateway_status(&temp.config());
    assert_eq!(result.worker_processes_started, 0);
    assert_eq!(before["identity"], after["identity"]);
}

#[test]
fn saved_pending_config_is_detected_from_the_authenticated_worker_fingerprint() {
    let temp = Temp::new("saved-pending-restart");
    let first_listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let first_port = first_listener.local_addr().unwrap().port();
    drop(first_listener);
    apply(&temp.config(), &draft(first_port), ApplyMode::SaveOnly).unwrap();
    let started = Command::new(env!("CARGO_BIN_EXE_llmgw"))
        .args(["on", "--config"])
        .arg(temp.config())
        .output()
        .unwrap();
    assert!(started.status.success(), "{started:?}");
    let before = gateway_status(&temp.config());

    let next_listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let next_port = next_listener.local_addr().unwrap().port();
    drop(next_listener);
    let loaded = LoadedConfig::load(temp.config()).unwrap();
    let changed = SetupDraft::from_loaded(&loaded).unwrap().run(RunAnswers {
        port: next_port,
        login_requested: false,
    });
    apply(&temp.config(), &changed, ApplyMode::SaveOnly).unwrap();
    let saved = gateway_status(&temp.config());
    assert_eq!(saved["identity"], before["identity"]);
    assert_eq!(saved["pending_restart"], true);

    let loaded = LoadedConfig::load(temp.config()).unwrap();
    let desired = loaded.fingerprint.as_str().to_owned();
    let rerun = SetupDraft::from_loaded(&loaded).unwrap();
    let impact = runtime_impact(&temp.config(), &rerun).unwrap();
    assert!(matches!(
        &impact,
        RuntimeImpact::RestartRequired {
            desired_fingerprint,
            worker_fingerprint,
        } if desired_fingerprint == &desired
            && worker_fingerprint == before["identity"]["fingerprint"].as_str().unwrap()
    ));
    let summary = rerun.summary_with_runtime(&temp.config(), &impact).unwrap();
    assert!(summary.contains(&format!("desired {desired}")));
    assert!(summary.contains("authenticated running worker"));
    assert!(summary.contains("explicitly restarts"));
    assert!(summary.contains("cancels queued requests"));
    assert!(summary.contains("drains active requests for up to 10 seconds"));
}

#[test]
fn desired_config_matching_worker_avoids_restart_even_when_disk_was_pending() {
    let temp = Temp::new("desired-matches-worker");
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    drop(listener);
    apply(&temp.config(), &draft(port), ApplyMode::SaveOnly).unwrap();
    let started = Command::new(env!("CARGO_BIN_EXE_llmgw"))
        .args(["on", "--config"])
        .arg(temp.config())
        .output()
        .unwrap();
    assert!(started.status.success(), "{started:?}");
    let before = gateway_status(&temp.config());

    let loaded = LoadedConfig::load(temp.config()).unwrap();
    let pending = SetupDraft::from_loaded(&loaded).unwrap().run(RunAnswers {
        port: port.checked_add(1).unwrap(),
        login_requested: false,
    });
    apply(&temp.config(), &pending, ApplyMode::SaveOnly).unwrap();
    assert_eq!(gateway_status(&temp.config())["pending_restart"], true);

    let loaded = LoadedConfig::load(temp.config()).unwrap();
    let desired = SetupDraft::from_loaded(&loaded).unwrap().run(RunAnswers {
        port,
        login_requested: false,
    });
    let impact = runtime_impact(&temp.config(), &desired).unwrap();
    assert!(matches!(impact, RuntimeImpact::Matching { .. }));
    let result = apply(&temp.config(), &desired, ApplyMode::SaveAndStart).unwrap();
    let after = gateway_status(&temp.config());
    assert_eq!(result.worker_processes_started, 0);
    assert_eq!(after["identity"], before["identity"]);
    assert_eq!(after["pending_restart"], false);
}

#[test]
fn pending_restart_without_preview_consent_is_refused_before_apply() {
    let temp = Temp::new("pending-without-consent");
    let first_listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let first_port = first_listener.local_addr().unwrap().port();
    drop(first_listener);
    apply(&temp.config(), &draft(first_port), ApplyMode::SaveOnly).unwrap();
    let started = Command::new(env!("CARGO_BIN_EXE_llmgw"))
        .args(["on", "--config"])
        .arg(temp.config())
        .output()
        .unwrap();
    assert!(started.status.success(), "{started:?}");
    let before = gateway_status(&temp.config());

    let loaded = LoadedConfig::load(temp.config()).unwrap();
    let changed = SetupDraft::from_loaded(&loaded).unwrap().run(RunAnswers {
        port: first_port.checked_add(1).unwrap(),
        login_requested: false,
    });
    apply(&temp.config(), &changed, ApplyMode::SaveOnly).unwrap();
    let saved_bytes = fs::read(temp.config()).unwrap();
    let saved = LoadedConfig::load(temp.config()).unwrap();
    let error = apply(
        &temp.config(),
        &SetupDraft::from_loaded(&saved).unwrap(),
        ApplyMode::SaveAndStart,
    )
    .unwrap_err()
    .to_string();
    assert!(error.contains("restart impact"), "{error}");
    assert_eq!(fs::read(temp.config()).unwrap(), saved_bytes);
    assert_eq!(
        gateway_status(&temp.config())["identity"],
        before["identity"]
    );
}

#[test]
fn unverified_running_worker_refuses_start_or_restart_before_writing() {
    let temp = Temp::new("unverified-worker");
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    drop(listener);
    apply(&temp.config(), &draft(port), ApplyMode::SaveOnly).unwrap();
    let started = Command::new(env!("CARGO_BIN_EXE_llmgw"))
        .args(["on", "--config"])
        .arg(temp.config())
        .output()
        .unwrap();
    assert!(started.status.success(), "{started:?}");
    let before = gateway_status(&temp.config());
    let loaded = LoadedConfig::load(temp.config()).unwrap();
    let reviewed = SetupDraft::from_loaded(&loaded).unwrap();
    let config_before = fs::read(temp.config()).unwrap();
    let token = loaded.state_paths.control_token;
    let held_token = token.with_extension("held-for-test");
    fs::rename(&token, &held_token).unwrap();
    let result = apply(&temp.config(), &reviewed, ApplyMode::SaveAndRestart);
    fs::rename(&held_token, &token).unwrap();

    let error = result.unwrap_err().to_string();
    assert!(error.contains("could not be authenticated"), "{error}");
    assert_eq!(fs::read(temp.config()).unwrap(), config_before);
    assert_eq!(
        gateway_status(&temp.config())["identity"],
        before["identity"]
    );
}

#[cfg(target_os = "macos")]
#[test]
fn edited_existing_config_preserves_its_extended_acl() {
    let temp = Temp::new("config-acl");
    apply(&temp.config(), &draft(4141), ApplyMode::SaveOnly).unwrap();
    assert!(
        Command::new("/bin/chmod")
            .args(["+a", "everyone allow read"])
            .arg(temp.config())
            .status()
            .unwrap()
            .success()
    );
    let before = exacl::getfacl(temp.config(), None).unwrap();
    let loaded = LoadedConfig::load(temp.config()).unwrap();
    let edited = SetupDraft::from_loaded(&loaded).unwrap().run(RunAnswers {
        port: 4142,
        login_requested: false,
    });
    apply(&temp.config(), &edited, ApplyMode::SaveOnly).unwrap();
    assert_eq!(exacl::getfacl(temp.config(), None).unwrap(), before);
}

#[test]
fn invalid_config_and_missing_inference_arguments_exit_two() {
    let temp = Temp::new("invalid-cli");
    fs::write(temp.config(), "not valid toml").unwrap();
    for command in ["doctor", "on", "run"] {
        let output = Command::new(env!("CARGO_BIN_EXE_llmgw"))
            .arg(command)
            .args(["--config"])
            .arg(temp.config())
            .output()
            .unwrap();
        assert_eq!(output.status.code(), Some(2), "{command}: {output:?}");
    }
    fs::write(temp.config(), draft(4141).render_config().unwrap()).unwrap();
    let output = Command::new(env!("CARGO_BIN_EXE_llmgw"))
        .args(["doctor", "--inference", "--config"])
        .arg(temp.config())
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(2), "{output:?}");
}

#[test]
fn scalar_edit_preserves_valid_inline_auth_and_quota_tables_without_panicking() {
    let temp = Temp::new("inline-tables");
    let raw = r#"listen = "127.0.0.1:4200"
concurrency = 1
cancel_policy = "drain"
accounting = "reserved"
retry_transient_429 = false
upstream = { api_base = "https://example.invalid/v1", auth = { mode = "none" } }
quota = { rpm = { kind = "unlimited" }, tpm = { kind = "unknown" } }
[[models]]
id = "a"
max_output_tokens = 20
[[roots]]
id = "pi"
endpoints = ["responses"]
models = ["a"]
"#;
    fs::write(temp.config(), raw).unwrap();
    let loaded = LoadedConfig::load(temp.config()).unwrap();
    let edited = SetupDraft::from_loaded(&loaded).unwrap().run(RunAnswers {
        port: 4201,
        login_requested: false,
    });
    let rendered = edited.render_config().unwrap();
    let parsed = llmgw::config::parse(rendered.as_bytes()).unwrap();
    assert_eq!(parsed.listen.port(), 4201);
    assert!(rendered.contains("auth = { mode = \"none\" }"));
    assert!(rendered.contains("rpm = { kind = \"unlimited\" }"));
}

#[test]
fn save_and_rerun_preserve_minimal_pending_intents_without_credentials() {
    let temp = Temp::new("pending-intents");
    let configured = draft(4141)
        .quota(QuotaAnswers {
            rpm: LimitAnswer::Unlimited,
            tpm: LimitAnswer::Unknown,
            shared_with_other_pcs: true,
            separate_input_output: true,
            concurrency: 1,
            startup_hold_secs: 60,
            cache: None,
        })
        .run(RunAnswers {
            port: 4141,
            login_requested: true,
        })
        .tools(ToolAnswers {
            clients: vec![ClientIntent::ClaudeCode, ClientIntent::Codex],
        });
    let saved = apply(&temp.config(), &configured, ApplyMode::SaveOnly).unwrap();
    let pending = fs::read_to_string(&saved.pending_path).unwrap();
    assert!(pending.contains("shared_with_other_pcs"));
    assert!(pending.contains("claude_code"));
    assert!(!pending.contains("api_base"));
    assert!(!pending.contains("authorization"));
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        assert_eq!(
            fs::metadata(&saved.pending_path)
                .unwrap()
                .permissions()
                .mode()
                & 0o777,
            0o600
        );
    }

    let loaded = LoadedConfig::load(temp.config()).unwrap();
    let restored = SetupDraft::from_loaded(&loaded).unwrap();
    assert!(restored.shared_with_other_pcs());
    assert!(restored.separate_input_output());
    assert!(restored.login_requested());
    assert_eq!(
        restored.clients(),
        &[ClientIntent::ClaudeCode, ClientIntent::Codex]
    );
}

#[cfg(unix)]
#[test]
fn edited_existing_config_preserves_its_permissions() {
    use std::os::unix::fs::PermissionsExt;
    let temp = Temp::new("config-permissions");
    apply(&temp.config(), &draft(4141), ApplyMode::SaveOnly).unwrap();
    fs::set_permissions(temp.config(), fs::Permissions::from_mode(0o640)).unwrap();
    let loaded = LoadedConfig::load(temp.config()).unwrap();
    let edited = SetupDraft::from_loaded(&loaded).unwrap().run(RunAnswers {
        port: 4142,
        login_requested: false,
    });
    apply(&temp.config(), &edited, ApplyMode::SaveOnly).unwrap();
    assert_eq!(
        fs::metadata(temp.config()).unwrap().permissions().mode() & 0o777,
        0o640
    );
}

#[cfg(unix)]
#[test]
fn setup_refuses_a_config_symlink_even_when_target_bytes_are_unchanged() {
    use std::os::unix::fs::symlink;
    let temp = Temp::new("config-symlink");
    let target = temp.dir.join("target.toml");
    let configured = draft(4141);
    let bytes = configured.render_config().unwrap();
    fs::write(&target, &bytes).unwrap();
    symlink(&target, temp.config()).unwrap();
    let error = apply(&temp.config(), &configured, ApplyMode::SaveOnly)
        .unwrap_err()
        .to_string();
    assert!(error.contains("regular file, not a link"), "{error}");
    assert_eq!(fs::read_to_string(target).unwrap(), bytes);
}

#[test]
fn summary_resolves_paths_and_joins_prefix_and_v1_exactly_once() {
    let temp = Temp::new("summary");
    let config = temp.config();
    let summary = draft(4141).summary(&config).unwrap();
    assert!(summary.contains(config.to_string_lossy().as_ref()));
    assert!(summary.contains("pending, registration not applied"));
    assert!(summary.contains("pending, client files not changed"));
    assert!(summary.contains("quota: estimated"));
    assert!(summary.contains("login requested: no"));
    assert!(
        summary.contains("after config save, exact OS target preview and separate confirmation")
    );
    assert!(summary.contains("capabilities: unverified"));
    assert!(summary.contains("user-supplied accounting fallback"));
    assert!(summary.contains("not a verified provider limit"));
    assert!(summary.contains("fingerprint impact:"));
    assert!(!summary.contains("Bearer"));
    let urls = example_urls("http://127.0.0.1:4141", "pi-work").unwrap();
    assert_eq!(urls.openai, "http://127.0.0.1:4141/r/pi-work/v1");
    assert_eq!(urls.messages, "http://127.0.0.1:4141/r/pi-work");
    assert_eq!(
        urls.messages_request,
        "http://127.0.0.1:4141/r/pi-work/v1/messages"
    );
    assert!(example_urls("http://127.0.0.1:4141", "pi work").is_err());
}

#[test]
fn non_tty_first_run_exits_two_without_waiting_or_creating_files() {
    let temp = Temp::new("non-tty");
    let output = Command::new(env!("CARGO_BIN_EXE_llmgw"))
        .arg("--config")
        .arg(temp.config())
        .stdin(std::process::Stdio::null())
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(2));
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("run `llmgw setup` in a terminal"));
    assert!(stderr.contains(temp.config().to_string_lossy().as_ref()));
    assert!(!stderr.contains("examples/fixture.toml"));
    assert!(!stderr.contains("docs/runtime-contract.md"));
    assert!(!temp.config().exists());
}

#[test]
fn explicit_command_with_missing_config_is_unconfigured_exit_two() {
    let temp = Temp::new("explicit-unconfigured");
    let output = Command::new(env!("CARGO_BIN_EXE_llmgw"))
        .args(["status", "--config"])
        .arg(temp.config())
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(2));
    assert!(String::from_utf8_lossy(&output.stderr).contains("llmgw setup"));
    assert!(!temp.config().exists());
}

#[test]
fn relative_config_is_fixed_from_invocation_directory() {
    let temp = Temp::new("relative-config");
    apply(&temp.config(), &draft(4141), ApplyMode::SaveOnly).unwrap();
    let output = Command::new(env!("CARGO_BIN_EXE_llmgw"))
        .current_dir(&temp.dir)
        .args(["status", "--json", "--config", "gateway 설정.toml"])
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(
        serde_json::from_slice::<serde_json::Value>(&output.stdout).unwrap()["state"],
        "stopped"
    );
}

#[test]
fn bare_configured_command_means_on_and_reaches_authenticated_readiness() {
    let temp = Temp::new("bare-on");
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    drop(listener);
    apply(&temp.config(), &draft(port), ApplyMode::SaveOnly).unwrap();
    let output = Command::new(env!("CARGO_BIN_EXE_llmgw"))
        .args(["--config"])
        .arg(temp.config())
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let status = Command::new(env!("CARGO_BIN_EXE_llmgw"))
        .args(["status", "--json", "--config"])
        .arg(temp.config())
        .output()
        .unwrap();
    let status: serde_json::Value = serde_json::from_slice(&status.stdout).unwrap();
    assert_eq!(status["state"], "running");
    assert!(status["identity"]["fingerprint"].as_str().is_some());
}

#[test]
fn save_only_starts_no_process_and_real_on_reaches_authenticated_readiness() {
    let temp = Temp::new("apply");
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    drop(listener);
    let save = apply(&temp.config(), &draft(port), ApplyMode::SaveOnly).unwrap();
    assert_eq!(save.runtime_state, "not_started");
    assert_eq!(save.worker_processes_started, 0);
    let stopped = Command::new(env!("CARGO_BIN_EXE_llmgw"))
        .args(["status", "--json", "--config"])
        .arg(temp.config())
        .output()
        .unwrap();
    assert!(stopped.status.success());
    assert_eq!(
        serde_json::from_slice::<serde_json::Value>(&stopped.stdout).unwrap()["state"],
        "stopped"
    );

    let started = Command::new(env!("CARGO_BIN_EXE_llmgw"))
        .args(["on", "--config"])
        .arg(temp.config())
        .output()
        .unwrap();
    assert!(
        started.status.success(),
        "{}",
        String::from_utf8_lossy(&started.stderr)
    );
    let running = Command::new(env!("CARGO_BIN_EXE_llmgw"))
        .args(["status", "--json", "--config"])
        .arg(temp.config())
        .output()
        .unwrap();
    let running: serde_json::Value = serde_json::from_slice(&running.stdout).unwrap();
    assert_eq!(running["state"], "running");
    assert!(running["identity"]["fingerprint"].as_str().is_some());
}

#[test]
fn fresh_save_only_initializes_private_tokens_without_starting_and_preserves_identity() {
    let temp = Temp::new("save-only-state");
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    drop(listener);

    let save = apply(&temp.config(), &draft(port), ApplyMode::SaveOnly).unwrap();
    let loaded = LoadedConfig::load(temp.config()).unwrap();
    let control_before = fs::read(&loaded.state_paths.control_token).unwrap();
    assert_eq!(control_before.len(), 64);
    assert!(!loaded.state_paths.directory.join("data-token").exists());
    assert!(!loaded.state_paths.runtime_state.exists());
    assert_eq!(save.runtime_state, "not_started");
    assert_eq!(save.worker_processes_started, 0);
    assert!(!save.authenticated_readiness);

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        assert_eq!(
            fs::metadata(&loaded.state_paths.directory)
                .unwrap()
                .permissions()
                .mode()
                & 0o777,
            0o700
        );
        assert_eq!(
            fs::metadata(&loaded.state_paths.control_token)
                .unwrap()
                .permissions()
                .mode()
                & 0o777,
            0o600
        );
    }

    let rerun = SetupDraft::from_loaded(&loaded).unwrap();
    apply(&temp.config(), &rerun, ApplyMode::SaveOnly).unwrap();
    assert_eq!(
        fs::read(&loaded.state_paths.control_token).unwrap(),
        control_before
    );
    let stopped = gateway_status(&temp.config());
    assert_eq!(stopped["state"], "stopped");
}

#[cfg(unix)]
#[test]
fn state_initialization_failure_reports_partial_apply_and_never_starts_worker() {
    use std::os::unix::fs::PermissionsExt;

    let temp = Temp::new("state-init-failure");
    let config = fs::canonicalize(&temp.dir)
        .unwrap()
        .join("gateway 설정.toml");
    let state = StatePaths::from_config_path(&config).unwrap();
    fs::create_dir(&state.directory).unwrap();
    fs::set_permissions(&state.directory, fs::Permissions::from_mode(0o755)).unwrap();
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    drop(listener);

    let error = apply(&config, &draft(port), ApplyMode::SaveAndStart)
        .unwrap_err()
        .to_string();
    assert!(
        error.contains("configuration and setup pending metadata were applied")
            && error.contains("protected state initialization failed"),
        "{error}"
    );
    assert!(config.exists());
    assert!(
        temp.dir
            .join(".gateway 설정.toml.setup-pending.json")
            .exists()
    );
    assert!(!state.directory.join("data-token").exists());
    assert!(!state.control_token.exists());
    assert!(!state.runtime_state.exists());

    fs::set_permissions(&state.directory, fs::Permissions::from_mode(0o700)).unwrap();
}

#[cfg(unix)]
#[test]
fn existing_invalid_token_is_preserved_and_reports_partial_state_initialization() {
    use std::os::unix::fs::PermissionsExt;

    let temp = Temp::new("invalid-existing-token");
    let config = fs::canonicalize(&temp.dir)
        .unwrap()
        .join("gateway 설정.toml");
    let state = StatePaths::from_config_path(&config).unwrap();
    fs::create_dir(&state.directory).unwrap();
    fs::set_permissions(&state.directory, fs::Permissions::from_mode(0o700)).unwrap();
    fs::write(&state.control_token, b"").unwrap();
    fs::set_permissions(&state.control_token, fs::Permissions::from_mode(0o600)).unwrap();
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    drop(listener);

    let error = apply(&config, &draft(port), ApplyMode::SaveOnly)
        .unwrap_err()
        .to_string();
    assert!(
        error.contains("protected state initialization failed"),
        "{error}"
    );
    assert!(config.exists());
    assert_eq!(fs::read(&state.control_token).unwrap(), b"");
    assert!(!state.runtime_state.exists());
}

#[test]
fn save_only_never_repairs_a_missing_token_under_a_running_worker() {
    let temp = Temp::new("running-missing-token");
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    drop(listener);
    apply(&temp.config(), &draft(port), ApplyMode::SaveOnly).unwrap();
    let started = Command::new(env!("CARGO_BIN_EXE_llmgw"))
        .args(["on", "--config"])
        .arg(temp.config())
        .output()
        .unwrap();
    assert!(
        started.status.success(),
        "{}",
        String::from_utf8_lossy(&started.stderr)
    );
    let before = gateway_status(&temp.config());
    let loaded = LoadedConfig::load(temp.config()).unwrap();
    let control = fs::read(&loaded.state_paths.control_token).unwrap();
    fs::remove_file(&loaded.state_paths.control_token).unwrap();

    let rerun = SetupDraft::from_loaded(&loaded).unwrap();
    let error = apply(&temp.config(), &rerun, ApplyMode::SaveOnly)
        .unwrap_err()
        .to_string();
    assert!(
        error.contains("protected state initialization failed"),
        "{error}"
    );
    assert!(!loaded.state_paths.control_token.exists());
    private_fs::write_private(&loaded.state_paths.control_token, &control);
    let after = gateway_status(&temp.config());
    assert_eq!(after["state"], "running");
    assert_eq!(after["identity"]["pid"], before["identity"]["pid"]);
    assert_eq!(after["identity"]["nonce"], before["identity"]["nonce"]);
}

#[test]
fn doctor_default_is_offline_and_reports_static_auth_availability() {
    let temp = Temp::new("doctor-offline");
    apply(&temp.config(), &draft(4141), ApplyMode::SaveOnly).unwrap();
    let output = Command::new(env!("CARGO_BIN_EXE_llmgw"))
        .args(["doctor", "--config"])
        .arg(temp.config())
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let text = String::from_utf8_lossy(&output.stdout);
    assert!(text.contains("network: not requested"));
    assert!(text.contains("auth availability: not required"));
    assert!(text.contains("port availability:"));
    assert!(text.contains("worker readiness: not checked"));
    assert!(text.contains("autostart registration:"));
    assert!(text.contains("login auth availability: available"));
}

#[test]
fn doctor_missing_terminal_env_keeps_unobserved_login_auth_unknown() {
    let temp = Temp::new("doctor-missing-env");
    let name = format!("LLMGW_SETUP_DOCTOR_MISSING_{}", std::process::id());
    let configured = draft(4141).connection(ConnectionAnswers {
        api_base: "https://example.invalid/v1".into(),
        endpoints: vec![Endpoint::Responses],
        auth: Auth::Env {
            header: "authorization".into(),
            name: name.clone(),
        },
        proxy: None,
        ca_bundle: None,
    });
    apply(&temp.config(), &configured, ApplyMode::SaveOnly).unwrap();
    let output = Command::new(env!("CARGO_BIN_EXE_llmgw"))
        .args(["doctor", "--config"])
        .arg(temp.config())
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(1));
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains(&format!("environment reference {name} unavailable")));
    assert!(stdout.contains("network: not requested"));
    assert!(stdout.contains("login auth availability: unknown"));
    assert!(!stdout.contains("Bearer"));
}

fn one_http_response(
    response: &'static [u8],
) -> (std::net::SocketAddr, std::thread::JoinHandle<Vec<u8>>) {
    use std::io::{Read, Write};
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap();
    let task = std::thread::spawn(move || {
        let (mut socket, _) = listener.accept().unwrap();
        socket
            .set_read_timeout(Some(std::time::Duration::from_secs(3)))
            .unwrap();
        let mut bytes = Vec::new();
        let mut part = [0u8; 1024];
        loop {
            let count = socket.read(&mut part).unwrap();
            assert_ne!(count, 0);
            bytes.extend_from_slice(&part[..count]);
            if let Some(end) = bytes
                .windows(4)
                .position(|w| w == b"\r\n\r\n")
                .map(|p| p + 4)
            {
                let head = String::from_utf8_lossy(&bytes[..end]);
                let length = head
                    .lines()
                    .find_map(|line| {
                        line.strip_prefix("content-length: ")
                            .or_else(|| line.strip_prefix("Content-Length: "))
                    })
                    .and_then(|v| v.trim().parse::<usize>().ok())
                    .unwrap_or(0);
                if bytes.len() >= end + length {
                    break;
                }
            }
        }
        socket.write_all(response).unwrap();
        bytes
    });
    (address, task)
}

#[test]
fn doctor_network_is_one_bounded_models_get_without_inference() {
    let response = b"HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: 27\r\nConnection: close\r\n\r\n{\"data\":[{\"id\":\"model-a\"}]}";
    let (address, request) = one_http_response(response);
    let temp = Temp::new("doctor-network");
    let configured = draft(4141).connection(ConnectionAnswers {
        api_base: format!("http://{address}/prefix/v1"),
        endpoints: vec![Endpoint::Responses],
        auth: Auth::None,
        proxy: None,
        ca_bundle: None,
    });
    apply(&temp.config(), &configured, ApplyMode::SaveOnly).unwrap();
    let output = Command::new(env!("CARGO_BIN_EXE_llmgw"))
        .args(["doctor", "--network", "--config"])
        .arg(temp.config())
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let request = String::from_utf8(request.join().unwrap()).unwrap();
    assert!(request.starts_with("GET /prefix/v1/models HTTP/1.1\r\n"));
    assert!(!request.contains("model-a\""));
    assert!(String::from_utf8_lossy(&output.stdout).contains("model-a"));
}

#[test]
fn doctor_inference_is_one_explicit_bounded_call_and_no_listing() {
    let body = br#"{"id":"response-fixture","status":"completed","error":null,"incomplete_details":null,"output":[{"type":"message","content":[{"type":"output_text","text":"OK"}]}]}"#;
    let response = format!(
        "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
        body.len(),
        String::from_utf8_lossy(body)
    );
    let leaked: &'static [u8] = Box::leak(response.into_bytes().into_boxed_slice());
    let (address, request) = one_http_response(leaked);
    let temp = Temp::new("doctor-inference");
    let configured = draft(4141).connection(ConnectionAnswers {
        api_base: format!("http://{address}/prefix/v1"),
        endpoints: vec![Endpoint::Responses],
        auth: Auth::None,
        proxy: None,
        ca_bundle: None,
    });
    apply(&temp.config(), &configured, ApplyMode::SaveOnly).unwrap();
    let output = Command::new(env!("CARGO_BIN_EXE_llmgw"))
        .args([
            "doctor",
            "--inference",
            "--model",
            "model-a",
            "--max-output-tokens",
            "8",
            "--config",
        ])
        .arg(temp.config())
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let request = String::from_utf8(request.join().unwrap()).unwrap();
    assert!(request.starts_with("POST /prefix/v1/responses HTTP/1.1\r\n"));
    assert!(request.contains("\"model\":\"model-a\""));
    assert!(request.contains("\"max_output_tokens\":8"));
    assert!(!request.contains("GET "));
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("inference calls: 1"));
    assert!(stdout.contains("output bound: 8"));
}

fn run_inference_doctor_with_response(response_body: &'static [u8]) -> std::process::Output {
    let response = format!(
        "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
        response_body.len(),
        String::from_utf8_lossy(response_body)
    );
    let leaked: &'static [u8] = Box::leak(response.into_bytes().into_boxed_slice());
    let (address, request) = one_http_response(leaked);
    let temp = Temp::new("doctor-inference-invalid");
    let configured = draft(4141).connection(ConnectionAnswers {
        api_base: format!("http://{address}/prefix/v1"),
        endpoints: vec![Endpoint::Responses],
        auth: Auth::None,
        proxy: None,
        ca_bundle: None,
    });
    apply(&temp.config(), &configured, ApplyMode::SaveOnly).unwrap();
    let output = Command::new(env!("CARGO_BIN_EXE_llmgw"))
        .args([
            "doctor",
            "--inference",
            "--model",
            "model-a",
            "--max-output-tokens",
            "8",
            "--config",
        ])
        .arg(temp.config())
        .output()
        .unwrap();
    request.join().unwrap();
    output
}

#[test]
fn doctor_inference_rejects_http_200_without_generation_evidence() {
    let output = run_inference_doctor_with_response(br#"{}"#);
    assert_eq!(output.status.code(), Some(1));
    assert!(String::from_utf8_lossy(&output.stderr).contains("generation evidence"));
}

#[test]
fn doctor_inference_rejects_provider_failed_response() {
    let output = run_inference_doctor_with_response(
        br#"{"id":"response-fixture","status":"failed","error":{"message":"synthetic failure"}}"#,
    );
    assert_eq!(output.status.code(), Some(1));
    assert!(String::from_utf8_lossy(&output.stderr).contains("provider reported failure"));
}

#[tokio::test]
async fn explicit_proxy_is_used_for_non_loopback_upstream_without_environment_proxy() {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let proxy_address = listener.local_addr().unwrap();
    let proxy = tokio::spawn(async move {
        let (mut socket, _) = listener.accept().await.unwrap();
        let mut bytes = Vec::new();
        let mut part = [0u8; 1024];
        loop {
            let count = socket.read(&mut part).await.unwrap();
            assert_ne!(count, 0);
            bytes.extend_from_slice(&part[..count]);
            if bytes.windows(4).any(|window| window == b"\r\n\r\n") {
                break;
            }
        }
        let head = String::from_utf8(bytes).unwrap();
        assert!(
            head.starts_with("GET http://upstream.invalid/prefix/v1/models HTTP/1.1\r\n"),
            "{head}"
        );
        socket.write_all(b"HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: 12\r\nConnection: close\r\n\r\n{\"data\":[]}").await.unwrap();
    });
    let mut config = draft(4141).config().clone();
    config.listen.set_port(0);
    config.upstream.api_base = "http://upstream.invalid/prefix/v1".parse().unwrap();
    config.upstream.proxy = Some(format!("http://{proxy_address}").parse().unwrap());
    config.roots[0].endpoints.push(Endpoint::Models);
    let gateway = llmgw::server::spawn(
        config,
        llmgw::server::RuntimeCredentials::new(b"control", None).unwrap(),
    )
    .await
    .unwrap();
    let request = b"GET /r/default/v1/models HTTP/1.1\r\nHost: localhost\r\nx-llmgw-token: data\r\nConnection: close\r\n\r\n";
    let response = send_raw(gateway.address(), request).await;
    assert_eq!(
        status(&response),
        200,
        "{}",
        String::from_utf8_lossy(response_body(&response))
    );
    proxy.await.unwrap();
    gateway.shutdown().await.unwrap();
}

#[tokio::test]
async fn explicit_proxy_bypasses_owned_loopback_upstream() {
    let upstream = support::fixture::UpstreamFixture::start(
        b"HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: 12\r\nConnection: close\r\n\r\n{\"data\":[]}".to_vec(),
    ).await;
    let proxy = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let mut config = draft(4141).config().clone();
    config.listen.set_port(0);
    config.upstream.api_base = format!("http://{}/prefix/v1", upstream.address())
        .parse()
        .unwrap();
    config.upstream.proxy = Some(
        format!("http://{}", proxy.local_addr().unwrap())
            .parse()
            .unwrap(),
    );
    config.roots[0].endpoints.push(Endpoint::Models);
    let gateway = llmgw::server::spawn(
        config,
        llmgw::server::RuntimeCredentials::new(b"control", None).unwrap(),
    )
    .await
    .unwrap();
    let response = send_raw(gateway.address(), b"GET /r/default/v1/models HTTP/1.1\r\nHost: localhost\r\nx-llmgw-token: data\r\nConnection: close\r\n\r\n").await;
    assert_eq!(status(&response), 200);
    assert_eq!(upstream.attempts(), 1);
    assert!(
        tokio::time::timeout(std::time::Duration::from_millis(100), proxy.accept())
            .await
            .is_err()
    );
    gateway.shutdown().await.unwrap();
}

#[tokio::test]
async fn explicit_ca_bundle_merges_with_system_trust_for_owned_tls_upstream() {
    use rustls::pki_types::{CertificateDer, PrivateKeyDer, pem::PemObject};
    use std::sync::Arc;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    let certificate =
        CertificateDer::from_pem_slice(include_bytes!("fixtures/setup-localhost-server.pem"))
            .unwrap();
    let key =
        PrivateKeyDer::from_pem_slice(include_bytes!("fixtures/setup-localhost-key.pem")).unwrap();
    let tls = rustls::ServerConfig::builder()
        .with_no_client_auth()
        .with_single_cert(vec![certificate], key)
        .unwrap();
    let acceptor = tokio_rustls::TlsAcceptor::from(Arc::new(tls));
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();
    let server = tokio::spawn(async move {
        let (socket, _) = listener.accept().await.unwrap();
        let mut socket = acceptor.accept(socket).await.unwrap();
        let mut received = Vec::new();
        let mut part = [0u8; 1024];
        loop {
            let count = socket.read(&mut part).await.unwrap();
            assert_ne!(count, 0);
            received.extend_from_slice(&part[..count]);
            if received.windows(4).any(|w| w == b"\r\n\r\n") {
                break;
            }
        }
        assert!(
            String::from_utf8_lossy(&received).starts_with("GET /prefix/v1/models HTTP/1.1\r\n")
        );
        socket.write_all(b"HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: 12\r\nConnection: close\r\n\r\n{\"data\":[]}").await.unwrap();
    });
    let mut config = draft(4141).config().clone();
    config.listen.set_port(0);
    config.upstream.api_base = format!("https://localhost:{port}/prefix/v1")
        .parse()
        .unwrap();
    config.upstream.ca_bundle = Some(
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/setup-localhost-cert.pem"),
    );
    config.roots[0].endpoints.push(Endpoint::Models);
    let gateway = llmgw::server::spawn(
        config,
        llmgw::server::RuntimeCredentials::new(b"control", None).unwrap(),
    )
    .await
    .unwrap();
    let response = send_raw(gateway.address(), b"GET /r/default/v1/models HTTP/1.1\r\nHost: localhost\r\nx-llmgw-token: data\r\nConnection: close\r\n\r\n").await;
    let server_result = server.await;
    assert!(
        server_result.is_ok(),
        "TLS fixture failed: {server_result:?}"
    );
    assert_eq!(
        status(&response),
        200,
        "{}",
        String::from_utf8_lossy(response_body(&response))
    );
    gateway.shutdown().await.unwrap();
}

#[test]
fn cache_setup_enables_preserves_edits_and_removes_only_the_requested_table() {
    let temp = Temp::new("exact-cache-setup");
    let answers = |cache| QuotaAnswers {
        rpm: LimitAnswer::Unlimited,
        tpm: LimitAnswer::Unknown,
        shared_with_other_pcs: false,
        separate_input_output: false,
        concurrency: 1,
        startup_hold_secs: 60,
        cache,
    };
    let cache = llmgw::config::CacheConfig {
        ttl_secs: 180,
        max_history: 2,
    };
    let raw = draft(4141)
        .quota(answers(Some(cache)))
        .render_config()
        .unwrap();
    assert_eq!(
        llmgw::config::parse(raw.as_bytes()).unwrap().cache,
        Some(cache)
    );
    let raw = raw.replace("ttl_secs = 180", "ttl_secs = 180 # preserve cache comment");
    fs::write(temp.config(), &raw).unwrap();
    let loaded = LoadedConfig::load(temp.config()).unwrap();
    assert_eq!(
        SetupDraft::from_loaded(&loaded)
            .unwrap()
            .render_config()
            .unwrap(),
        raw
    );
    let updated = llmgw::config::CacheConfig {
        ttl_secs: 90,
        max_history: 3,
    };
    let edited = SetupDraft::from_loaded(&loaded)
        .unwrap()
        .quota(answers(Some(updated)))
        .render_config()
        .unwrap();
    assert!(edited.contains("# preserve cache comment"));
    assert_eq!(
        llmgw::config::parse(edited.as_bytes()).unwrap().cache,
        Some(updated)
    );
    let removed = SetupDraft::from_loaded(&loaded)
        .unwrap()
        .quota(answers(None))
        .render_config()
        .unwrap();
    assert!(!removed.contains("[cache]"));
    assert!(
        llmgw::config::parse(removed.as_bytes())
            .unwrap()
            .cache
            .is_none()
    );
    assert!(
        answers(Some(llmgw::config::CacheConfig {
            ttl_secs: 0,
            max_history: 3
        }))
        .validate()
        .is_err()
    );
}

#[cfg(feature = "bpe")]
#[test]
fn editing_first_model_retains_estimator_other_models_routes_and_comments() {
    use llmgw::input_estimate::InputEstimator;
    let temp = Temp::new("model-estimator-preserve");
    let raw = r#"# user configuration
listen = "127.0.0.1:4141"
[upstream]
api_base = "http://127.0.0.1:9/v1"
auth = { mode = "none" }
[quota]
rpm = { kind = "unlimited" }
tpm = { kind = "unknown" }
[[models]]
id = "a" # first model
max_output_tokens = 20
input_estimator = "cl100k_base" # chosen vocabulary
input_token_overhead = 48
[[models]]
id = "b" # retained model
max_output_tokens = 30
input_estimator = "o200k_base"
input_token_overhead = 64
[[roots]]
id = "first"
endpoints = ["responses"]
models = ["a", "b"]
[[roots]]
id = "second" # retained route
endpoints = ["messages"]
models = ["b"]
"#;
    fs::write(temp.config(), raw).unwrap();
    let loaded = LoadedConfig::load(temp.config()).unwrap();
    // Renaming the first model to an existing second ID must fail validation,
    // rather than silently editing a different model or discarding either route.
    assert!(
        SetupDraft::from_loaded(&loaded)
            .unwrap()
            .models(ModelAnswers::manual("b", Some(50)))
            .render_config()
            .is_err()
    );
    let same = SetupDraft::from_loaded(&loaded)
        .unwrap()
        .models(ModelAnswers::manual("a", Some(50)));
    let rendered = same.render_config().unwrap();
    let parsed = llmgw::config::parse(rendered.as_bytes()).unwrap();
    assert_eq!(parsed.models[0].max_output_tokens.unwrap().get(), 50);
    assert_eq!(parsed.models[0].input_estimator, InputEstimator::Cl100kBase);
    assert_eq!(parsed.models[0].input_token_overhead, 48);
    assert_eq!(parsed.models[1], loaded.config.models[1]);
    assert_eq!(parsed.roots, loaded.config.roots);
    for comment in [
        "# user configuration",
        "# first model",
        "# chosen vocabulary",
        "# retained model",
        "# retained route",
    ] {
        assert!(rendered.contains(comment));
    }
    let mut changed = ModelAnswers::manual("a", Some(50));
    changed.input_estimator = Some(InputEstimator::O200kBase);
    changed.input_token_overhead = Some(80);
    let edited = SetupDraft::from_loaded(&loaded)
        .unwrap()
        .models(changed)
        .render_config()
        .unwrap();
    let parsed = llmgw::config::parse(edited.as_bytes()).unwrap();
    assert_eq!(parsed.models[0].input_estimator, InputEstimator::O200kBase);
    assert_eq!(parsed.models[0].input_token_overhead, 80);
    assert!(edited.contains("# chosen vocabulary"));
    assert_eq!(parsed.models[1], loaded.config.models[1]);
    assert_eq!(parsed.roots, loaded.config.roots);
    let renamed = SetupDraft::from_loaded(&loaded)
        .unwrap()
        .models(ModelAnswers::manual("new", Some(60)))
        .render_config()
        .unwrap();
    let parsed = llmgw::config::parse(renamed.as_bytes()).unwrap();
    assert_eq!(parsed.models[0].id, "new");
    assert_eq!(parsed.models[0].input_estimator, InputEstimator::Cl100kBase);
    assert_eq!(parsed.models[0].input_token_overhead, 32);
    assert_eq!(parsed.models[1], loaded.config.models[1]);
    assert_eq!(parsed.roots[0].models, ["new", "b"]);
    assert_eq!(parsed.roots[1], loaded.config.roots[1]);
}

#[cfg(not(feature = "bpe"))]
#[test]
fn byte_only_build_rejects_bpe_draft_before_replacing_existing_config() {
    use llmgw::input_estimate::InputEstimator::{Cl100kBase, O200kBase};
    let temp = Temp::new("missing-bpe-capability");
    let original = format!(
        "# preserve user comment\n{}",
        draft(4141).render_config().unwrap()
    );
    fs::write(temp.config(), &original).unwrap();
    let loaded = LoadedConfig::load(temp.config()).unwrap();
    for estimator in [Cl100kBase, O200kBase] {
        let mut answers = ModelAnswers::manual("model-a", Some(64));
        answers.input_estimator = Some(estimator);
        let edited = SetupDraft::from_loaded(&loaded).unwrap().models(answers);
        let error = apply(&temp.config(), &edited, ApplyMode::SaveOnly)
            .unwrap_err()
            .to_string();
        assert!(error.contains(estimator.name()), "{error}");
        assert!(error.contains("--features bpe"), "{error}");
        assert_eq!(fs::read(temp.config()).unwrap(), original.as_bytes());
        assert_eq!(fs::read_dir(&temp.dir).unwrap().count(), 1);
    }
}
