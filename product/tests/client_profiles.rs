use llmgw::clients::{
    ClientKind, ModelMetadata, ProfileRequest, ProjectLocalApproval, Protocol, Verification,
    apply_reviewed, disconnect, prepare, validate_current_snapshot,
};
use std::{collections::BTreeMap, fs, net::TcpListener, path::PathBuf, process::Command};

struct Fixture {
    root: PathBuf,
    preserve_on_drop: bool,
}

impl Fixture {
    fn new(name: &str) -> Self {
        let root = std::env::temp_dir().join(format!(
            "llmgw-client-profiles-{name}-{}-{:016x}",
            std::process::id(),
            rand::random::<u64>()
        ));
        fs::create_dir_all(&root).unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(&root, fs::Permissions::from_mode(0o700)).unwrap();
        }
        Self {
            root,
            preserve_on_drop: false,
        }
    }

    fn path(&self, relative: &str) -> PathBuf {
        self.root.join(relative)
    }

    fn private_file(&self, relative: &str, bytes: &[u8]) -> PathBuf {
        let path = self.path(relative);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(&path, bytes).unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(&path, fs::Permissions::from_mode(0o600)).unwrap();
        }
        path
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        if self.preserve_on_drop {
            return;
        }
        let _ = fs::remove_dir_all(&self.root);
    }
}

fn fresh_setup_draft(port: u16) -> llmgw::setup::SetupDraft {
    llmgw::setup::SetupDraft::new(llmgw::setup::EnvironmentPreset::ExternalApi)
        .connection(llmgw::setup::ConnectionAnswers {
            api_base: "http://127.0.0.1:9/v1".into(),
            endpoints: vec![llmgw::config::Endpoint::Responses],
            auth: llmgw::config::Auth::None,
            proxy: None,
            ca_bundle: None,
        })
        .models(llmgw::setup::ModelAnswers::manual(
            "example-model",
            Some(64),
        ))
        .quota(llmgw::setup::QuotaAnswers {
            rpm: llmgw::setup::LimitAnswer::Unlimited,
            tpm: llmgw::setup::LimitAnswer::Unknown,
            shared_with_other_pcs: false,
            separate_input_output: false,
            concurrency: 1,
        })
        .run(llmgw::setup::RunAnswers {
            port,
            login_requested: false,
        })
        .tools(llmgw::setup::ToolAnswers { clients: vec![] })
}

fn isolated_cli(binary: &str, home: &std::path::Path) -> Command {
    let mut command = Command::new(binary);
    command
        .env_clear()
        .env("PATH", "/usr/bin:/bin")
        .env("HOME", home)
        .env("USERPROFILE", home)
        .env("XDG_CONFIG_HOME", home.join(".config"));
    command
}

fn minimal_git_repository(path: &std::path::Path, tracked_settings: bool) {
    let git = path.join(".git");
    for directory in ["objects", "refs/heads", "refs/tags"] {
        fs::create_dir_all(git.join(directory)).unwrap();
    }
    fs::write(
        git.join("config"),
        b"[core]\nrepositoryformatversion = 0\nfilemode = true\nbare = false\nlogallrefupdates = true\n",
    )
    .unwrap();
    fs::write(git.join("HEAD"), b"ref: refs/heads/main\n").unwrap();
    fs::write(path.join(".gitignore"), b".claude/\n").unwrap();
    if tracked_settings {
        fs::write(
            git.join("index"),
            include_bytes!("fixtures/git-index-tracked-settings-local"),
        )
        .unwrap();
    }
}

fn git_fixture(path: &std::path::Path, args: &[&str]) {
    let output = Command::new("git")
        .args(args)
        .current_dir(path)
        .env_clear()
        .env("PATH", "/opt/homebrew/bin:/usr/bin:/bin")
        .env("HOME", path)
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "git {:?}: {}",
        args,
        String::from_utf8_lossy(&output.stderr)
    );
}

fn request(
    fixture: &Fixture,
    client: ClientKind,
    version: &str,
    protocol: Protocol,
) -> ProfileRequest {
    let config_dir = match client {
        ClientKind::Pi => fixture.path("home/.pi/agent"),
        ClientKind::Claude => fixture.path("home/.claude"),
        ClientKind::Codex => fixture.path("home/.codex"),
    };
    ProfileRequest {
        client,
        installed_version: version.into(),
        config_dir,
        config_dir_source: "synthetic explicit client home".into(),
        project_local: None,
        gateway_origin: "http://127.0.0.1:4141".into(),
        gateway_fingerprint: "a".repeat(64),
        root: "shared-work".into(),
        model: "example-model".into(),
        protocol,
        local_data_token: "synthetic-local-data-token".into(),
        token_source: fixture.path("gateway-state/data-token"),
        journal_path: fixture.path(&format!("gateway-state/clients/{}.json", client.as_str())),
        set_default: true,
        discover_models: false,
        metadata: ModelMetadata::unknown(),
        effective_environment: BTreeMap::new(),
        managed_settings: Vec::new(),
    }
}

#[test]
fn pi_profile_previews_absolute_files_and_applies_explicit_model_without_inventing_capabilities() {
    let fixture = Fixture::new("pi");
    let models = fixture.private_file(
        "home/.pi/agent/models.json",
        br#"{
  // keep this provider
  "providers": {"other": {"baseUrl":"https://example.invalid","models":[]}}
}
"#,
    );
    let settings = fixture.private_file(
        "home/.pi/agent/settings.json",
        br#"{"quietStartup":true,"defaultProvider":"other","defaultModel":"old"}"#,
    );
    let mut input = request(
        &fixture,
        ClientKind::Pi,
        "0.84.2",
        Protocol::OpenAiCompletions,
    );
    input.metadata = ModelMetadata {
        context_window: Some(4096),
        max_output_tokens: Some(32),
        reasoning: Verification::Unverified,
        tools: Verification::Unverified,
    };

    let plan = prepare(&input).unwrap();
    assert_eq!(plan.target_paths(), &[models.clone(), settings.clone()]);
    assert!(plan.target_paths().iter().all(|path| path.is_absolute()));
    assert!(
        plan.preview()
            .text
            .contains("http://127.0.0.1:4141/r/shared-work/v1")
    );
    assert!(plan.preview().text.contains("example-model"));
    assert!(plan.preview().text.contains("openai_completions"));
    assert!(
        plan.preview()
            .text
            .contains("credential: local data token from")
    );
    assert!(!plan.preview().text.contains("synthetic-local-data-token"));
    assert!(!format!("{plan:?}").contains("synthetic-local-data-token"));
    assert_eq!(plan.capabilities().tools, Verification::Unverified);
    assert_eq!(plan.capabilities().inference, Verification::Unverified);
    assert!(!fs::read_to_string(&models).unwrap().contains("llmgw"));

    apply_reviewed(&plan, &plan.preview().hash).unwrap();
    let written = fs::read_to_string(&models).unwrap();
    assert!(written.contains("// keep this provider"));
    let semantic: serde_json::Value = serde_json::from_str(
        &written
            .lines()
            .filter(|line| !line.trim_start().starts_with("//"))
            .collect::<Vec<_>>()
            .join("\n"),
    )
    .unwrap();
    let provider = &semantic["providers"]["llmgw"];
    assert_eq!(
        provider["baseUrl"],
        "http://127.0.0.1:4141/r/shared-work/v1"
    );
    assert_eq!(provider["api"], "openai-completions");
    assert_eq!(
        provider["headers"]["X-LLMGW-Token"],
        "synthetic-local-data-token"
    );
    assert_eq!(provider["models"][0]["contextWindow"], 4096);
    assert_eq!(provider["models"][0]["maxTokens"], 32);
    let defaults: serde_json::Value =
        serde_json::from_slice(&fs::read(&settings).unwrap()).unwrap();
    assert_eq!(defaults["defaultProvider"], "llmgw");
    assert_eq!(defaults["defaultModel"], "example-model");

    let restored = disconnect(&input.journal_path).unwrap();
    assert!(restored.conflicts.is_empty());
    let restored_models = fs::read_to_string(models).unwrap();
    let restored_models: serde_json::Value = serde_json::from_str(
        &restored_models
            .lines()
            .filter(|line| !line.trim_start().starts_with("//"))
            .collect::<Vec<_>>()
            .join("\n"),
    )
    .unwrap();
    assert!(restored_models["providers"].get("llmgw").is_none());
    assert!(restored_models["providers"].get("other").is_some());
    let defaults: serde_json::Value = serde_json::from_slice(&fs::read(settings).unwrap()).unwrap();
    assert_eq!(defaults["defaultProvider"], "other");
    assert_eq!(defaults["defaultModel"], "old");
    assert_eq!(defaults["quietStartup"], true);
}

#[test]
fn pi_override_home_and_optional_defaults_are_independent_resources() {
    let fixture = Fixture::new("pi-no-default");
    let mut input = request(
        &fixture,
        ClientKind::Pi,
        "0.84.2",
        Protocol::OpenAiCompletions,
    );
    input.set_default = false;
    let plan = prepare(&input).unwrap();
    assert_eq!(
        plan.target_paths(),
        &[fixture.path("home/.pi/agent/models.json")]
    );
    assert!(!plan.preview().text.contains("settings.json"));
}

#[test]
fn pi_refuses_an_unowned_existing_llmgw_provider_collision() {
    let fixture = Fixture::new("pi-collision");
    fixture.private_file(
        "home/.pi/agent/models.json",
        br#"{"providers":{"llmgw":{"baseUrl":"https://user.invalid","models":[]}}}"#,
    );
    let input = request(
        &fixture,
        ClientKind::Pi,
        "0.84.2",
        Protocol::OpenAiCompletions,
    );
    let error = prepare(&input).unwrap_err().to_string();
    assert!(error.contains("existing llmgw provider"));
    assert!(error.contains("disconnect") || error.contains("choose another provider"));
}

#[test]
fn claude_preserves_existing_env_and_headers_in_private_user_settings() {
    let fixture = Fixture::new("claude");
    let settings = fixture.private_file(
        "home/.claude/settings.json",
        br#"{"env":{"KEEP":"yes","ANTHROPIC_CUSTOM_HEADERS":"X-Existing: keep","ANTHROPIC_API_KEY":"opaque-existing-credential-sentinel"},"permissions":{"allow":[]}}"#,
    );
    let input = request(
        &fixture,
        ClientKind::Claude,
        "2.1.63",
        Protocol::AnthropicMessages,
    );
    let plan = prepare(&input).unwrap();
    assert!(
        !plan
            .preview()
            .text
            .contains("opaque-existing-credential-sentinel")
    );
    assert!(
        plan.preview()
            .text
            .contains("ANTHROPIC_API_KEY=llmgw-local-only")
    );
    assert_eq!(plan.target_paths(), &[settings.clone()]);
    apply_reviewed(&plan, &plan.preview().hash).unwrap();
    let value: serde_json::Value = serde_json::from_slice(&fs::read(&settings).unwrap()).unwrap();
    assert_eq!(value["env"]["KEEP"], "yes");
    assert_eq!(value["permissions"]["allow"], serde_json::json!([]));
    assert_eq!(
        value["env"]["ANTHROPIC_BASE_URL"],
        "http://127.0.0.1:4141/r/shared-work"
    );
    assert_eq!(value["env"]["ANTHROPIC_MODEL"], "example-model");
    assert_eq!(value["env"]["ANTHROPIC_API_KEY"], "llmgw-local-only");
    assert_eq!(
        value["env"]["ANTHROPIC_CUSTOM_HEADERS"],
        "X-Existing: keep\nX-LLMGW-Token: synthetic-local-data-token"
    );
    disconnect(&input.journal_path).unwrap();
    let restored: serde_json::Value = serde_json::from_slice(&fs::read(settings).unwrap()).unwrap();
    assert_eq!(
        restored["env"]["ANTHROPIC_API_KEY"],
        "opaque-existing-credential-sentinel"
    );
}

#[test]
fn claude_native_directory_refuses_a_git_tracked_settings_file_before_writes() {
    let fixture = Fixture::new("claude-native-tracked");
    let target = fixture.private_file("project/.claude/settings.json", br#"{"keep":true}"#);
    let project = fixture.path("project");
    git_fixture(&project, &["init", "-q"]);
    git_fixture(&project, &["add", "--", ".claude/settings.json"]);
    fs::write(project.join(".gitignore"), b".claude/\n").unwrap();
    let mut input = request(
        &fixture,
        ClientKind::Claude,
        "2.1.63",
        Protocol::AnthropicMessages,
    );
    input.config_dir = target.parent().unwrap().to_owned();
    input.config_dir_source = "CLAUDE_CONFIG_DIR environment".into();

    let error = prepare(&input).unwrap_err().to_string();
    assert!(error.contains("Git-tracked"), "{error}");
    assert_eq!(fs::read(&target).unwrap(), br#"{"keep":true}"#);
    assert!(!input.journal_path.exists());
}

#[cfg(unix)]
#[test]
fn claude_native_directory_accepts_a_safe_canonical_parent_alias() {
    use std::os::unix::fs::symlink;

    let fixture = Fixture::new("claude-native-alias");
    let target = fixture.private_file("private-native/settings.json", br#"{"keep":true}"#);
    let alias = fixture.path("native-alias");
    symlink(target.parent().unwrap(), &alias).unwrap();
    let mut input = request(
        &fixture,
        ClientKind::Claude,
        "2.1.63",
        Protocol::AnthropicMessages,
    );
    input.config_dir = alias;

    let plan = prepare(&input).unwrap();
    apply_reviewed(&plan, &plan.preview().hash).unwrap();
    assert!(
        fs::read_to_string(target)
            .unwrap()
            .contains("X-LLMGW-Token")
    );
}

#[test]
fn claude_native_scope_is_rechecked_when_git_state_changes_after_preview() {
    let fixture = Fixture::new("claude-native-git-drift");
    let target = fixture.private_file("project/private-claude/settings.json", br#"{"keep":true}"#);
    let project = fixture.path("project");
    git_fixture(&project, &["init", "-q"]);
    fs::write(
        project.join(".gitignore"),
        b"private-claude/settings.json\n",
    )
    .unwrap();
    let mut input = request(
        &fixture,
        ClientKind::Claude,
        "2.1.63",
        Protocol::AnthropicMessages,
    );
    input.config_dir = target.parent().unwrap().to_owned();
    let plan = prepare(&input).unwrap();

    git_fixture(
        &project,
        &["add", "-f", "--", "private-claude/settings.json"],
    );
    assert!(
        validate_current_snapshot(&plan)
            .unwrap_err()
            .to_string()
            .contains("Git-tracked")
    );
    assert!(
        apply_reviewed(&plan, &plan.preview().hash)
            .unwrap_err()
            .to_string()
            .contains("Git-tracked")
    );
    assert_eq!(fs::read(&target).unwrap(), br#"{"keep":true}"#);
    assert!(!input.journal_path.exists());
}

#[cfg(unix)]
#[test]
fn cli_tracked_native_claude_target_refuses_before_route_activation_or_client_write() {
    use std::os::unix::fs::PermissionsExt;

    let fixture = Fixture::new("claude-native-cli-refusal");
    let target = fixture.private_file("project/.claude/settings.json", br#"{"keep":true}"#);
    let project = fixture.path("project");
    git_fixture(&project, &["init", "-q"]);
    git_fixture(&project, &["add", "--", ".claude/settings.json"]);
    fs::write(project.join(".gitignore"), b".claude/\n").unwrap();
    let config = fixture.private_file(
        "gateway/config.toml",
        br#"listen = "127.0.0.1:4141"
concurrency = 1
cancel_policy = "drain"
accounting = "reserved"
[upstream]
api_base = "http://127.0.0.1:9/v1"
[upstream.auth]
mode = "none"
[quota.rpm]
kind = "unlimited"
[quota.tpm]
kind = "unknown"
[[models]]
id = "example-model"
[[roots]]
id = "existing-work"
endpoints = ["responses"]
models = ["example-model"]
"#,
    );
    let config_before = fs::read(&config).unwrap();
    let loaded = llmgw::config::LoadedConfig::load(&config).unwrap();
    fs::create_dir_all(&loaded.state_paths.directory).unwrap();
    fs::set_permissions(
        &loaded.state_paths.directory,
        fs::Permissions::from_mode(0o700),
    )
    .unwrap();
    fs::write(&loaded.state_paths.data_token, b"synthetic-data-token").unwrap();
    fs::set_permissions(
        &loaded.state_paths.data_token,
        fs::Permissions::from_mode(0o600),
    )
    .unwrap();
    let claude = fixture.private_file(
        "bin/claude",
        b"#!/bin/sh\nprintf '%s\\n' '2.1.63 (Claude Code)'\n",
    );
    fs::set_permissions(&claude, fs::Permissions::from_mode(0o700)).unwrap();
    let wrong_hash = "0".repeat(64);

    let output = Command::new(env!("CARGO_BIN_EXE_llmgw"))
        .args([
            "--config",
            config.to_str().unwrap(),
            "connect",
            "claude",
            "--client-executable",
            claude.to_str().unwrap(),
            "--client-config-dir",
            target.parent().unwrap().to_str().unwrap(),
            "--root",
            "claude-work",
            "--model",
            "example-model",
            "--apply-hash",
            &wrong_hash,
            "--restart",
        ])
        .env_clear()
        .env("PATH", "/opt/homebrew/bin:/usr/bin:/bin")
        .env("HOME", fixture.path("isolated-home"))
        .env("USERPROFILE", fixture.path("isolated-home"))
        .env("GIT_INDEX_FILE", fixture.path("redirected-empty-index"))
        .output()
        .unwrap();
    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("Git-tracked"));
    assert_eq!(fs::read(&config).unwrap(), config_before);
    assert_eq!(fs::read(&target).unwrap(), br#"{"keep":true}"#);
    assert!(!loaded.state_paths.runtime_state.exists());
    assert!(
        !loaded
            .state_paths
            .directory
            .join("clients/claude.journal.json")
            .exists()
    );
}

#[test]
fn claude_project_local_requires_private_and_untracked_confirmation() {
    let fixture = Fixture::new("claude-project");
    let mut input = request(
        &fixture,
        ClientKind::Claude,
        "2.1.63",
        Protocol::AnthropicMessages,
    );
    input.project_local = Some(ProjectLocalApproval {
        directory: fixture.path("work"),
        user_private_confirmed: true,
        untracked_confirmed: false,
    });
    let error = prepare(&input).unwrap_err().to_string();
    assert!(error.contains("untracked confirmations"));
    assert!(!fixture.path("work/.claude/settings.local.json").exists());
}

#[test]
fn claude_project_local_uses_actual_git_ignore_tracking_and_private_path_checks() {
    let fixture = Fixture::new("claude-project-git");
    let project = fixture.path("ignored-project");
    fs::create_dir_all(project.join(".claude")).unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&project, fs::Permissions::from_mode(0o700)).unwrap();
        fs::set_permissions(project.join(".claude"), fs::Permissions::from_mode(0o700)).unwrap();
    }
    minimal_git_repository(&project, false);
    let mut input = request(
        &fixture,
        ClientKind::Claude,
        "2.1.63",
        Protocol::AnthropicMessages,
    );
    input.project_local = Some(ProjectLocalApproval {
        directory: project.clone(),
        user_private_confirmed: true,
        untracked_confirmed: true,
    });
    let plan = prepare(&input).unwrap();
    assert_eq!(
        plan.target_paths(),
        &[project.join(".claude/settings.local.json")]
    );

    let tracked = fixture.path("tracked-project");
    fs::create_dir_all(tracked.join(".claude")).unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&tracked, fs::Permissions::from_mode(0o700)).unwrap();
        fs::set_permissions(tracked.join(".claude"), fs::Permissions::from_mode(0o700)).unwrap();
    }
    minimal_git_repository(&tracked, true);
    input.project_local = Some(ProjectLocalApproval {
        directory: tracked,
        user_private_confirmed: true,
        untracked_confirmed: true,
    });
    assert!(
        prepare(&input)
            .unwrap_err()
            .to_string()
            .contains("Git-tracked")
    );

    let shared = fixture.path("shared-project");
    fs::create_dir_all(shared.join(".claude")).unwrap();
    minimal_git_repository(&shared, false);
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(shared.join(".claude"), fs::Permissions::from_mode(0o755)).unwrap();
        input.project_local = Some(ProjectLocalApproval {
            directory: shared,
            user_private_confirmed: true,
            untracked_confirmed: true,
        });
        assert!(
            prepare(&input)
                .unwrap_err()
                .to_string()
                .contains("accessible to other users")
        );
    }
}

#[test]
fn claude_refuses_higher_priority_environment_and_conflicting_managed_policy() {
    let fixture = Fixture::new("claude-precedence");
    let mut input = request(
        &fixture,
        ClientKind::Claude,
        "2.1.63",
        Protocol::AnthropicMessages,
    );
    input.effective_environment.insert(
        "ANTHROPIC_BASE_URL".into(),
        "https://managed.invalid".into(),
    );
    assert!(
        prepare(&input)
            .unwrap_err()
            .to_string()
            .contains("higher-priority environment")
    );

    input.effective_environment.clear();
    input
        .effective_environment
        .insert("ANTHROPIC_API_KEY".into(), "opaque-process-value".into());
    assert!(
        prepare(&input)
            .unwrap_err()
            .to_string()
            .contains("ANTHROPIC_API_KEY")
    );

    input.effective_environment.clear();
    let managed = fixture.private_file(
        "managed/settings.json",
        br#"{"env":{"ANTHROPIC_API_KEY":"opaque-managed-value"}}"#,
    );
    input.managed_settings.push(managed);
    assert!(
        prepare(&input)
            .unwrap_err()
            .to_string()
            .contains("managed policy")
    );
}

#[test]
fn claude_discovery_is_version_gated_and_never_implied() {
    let fixture = Fixture::new("claude-discovery");
    let mut old = request(
        &fixture,
        ClientKind::Claude,
        "2.1.63",
        Protocol::AnthropicMessages,
    );
    old.discover_models = true;
    assert!(
        prepare(&old)
            .unwrap_err()
            .to_string()
            .contains("model discovery")
    );

    let ordinary = request(
        &fixture,
        ClientKind::Claude,
        "2.1.63",
        Protocol::AnthropicMessages,
    );
    let plan = prepare(&ordinary).unwrap();
    assert_eq!(plan.capabilities().listing, Verification::NotRequested);
    assert_eq!(plan.capabilities().selection, Verification::Configured);
}

#[test]
fn codex_uses_a_separate_responses_profile_and_static_native_header() {
    let fixture = Fixture::new("codex");
    let existing = fixture.private_file("home/.codex/llmgw.config.toml", b"notify = [\"keep\"]\n");
    let input = request(
        &fixture,
        ClientKind::Codex,
        "0.154.0",
        Protocol::OpenAiResponses,
    );
    let plan = prepare(&input).unwrap();
    assert_eq!(plan.target_paths(), &[existing.clone()]);
    apply_reviewed(&plan, &plan.preview().hash).unwrap();
    let value: toml_edit::DocumentMut = fs::read_to_string(existing).unwrap().parse().unwrap();
    assert_eq!(value["model"].as_str(), Some("example-model"));
    assert_eq!(value["model_provider"].as_str(), Some("llmgw"));
    assert_eq!(
        value["model_providers"]["llmgw"]["wire_api"].as_str(),
        Some("responses")
    );
    assert_eq!(
        value["model_providers"]["llmgw"]["supports_websockets"].as_bool(),
        Some(false)
    );
    assert_eq!(
        value["model_providers"]["llmgw"]["http_headers"]["X-LLMGW-Token"].as_str(),
        Some("synthetic-local-data-token")
    );
    assert_eq!(value["notify"][0].as_str(), Some("keep"));

    let restored = disconnect(&input.journal_path).unwrap();
    assert!(restored.conflicts.is_empty());
    let restored: toml_edit::DocumentMut =
        fs::read_to_string(fixture.path("home/.codex/llmgw.config.toml"))
            .unwrap()
            .parse()
            .unwrap();
    assert_eq!(restored["notify"][0].as_str(), Some("keep"));
    assert!(restored.get("model_providers").is_some_and(|providers| {
        providers
            .as_table_like()
            .is_some_and(|table| table.is_empty())
    }));
    assert!(restored.get("model").is_none());
    assert!(restored.get("model_provider").is_none());
}

#[test]
fn codex_refuses_an_unowned_existing_llmgw_provider_collision() {
    let fixture = Fixture::new("codex-collision");
    fixture.private_file(
        "home/.codex/llmgw.config.toml",
        br#"[model_providers.llmgw]
name = "user-owned"
base_url = "https://user.invalid/v1"
"#,
    );
    let input = request(
        &fixture,
        ClientKind::Codex,
        "0.154.0",
        Protocol::OpenAiResponses,
    );
    let error = prepare(&input).unwrap_err().to_string();
    assert!(error.contains("existing llmgw provider"));
    assert!(error.contains("disconnect") || error.contains("choose another provider"));
}

#[test]
fn unsupported_version_or_protocol_is_refused_before_any_write() {
    let fixture = Fixture::new("unsupported");
    let old = request(
        &fixture,
        ClientKind::Pi,
        "0.83.0",
        Protocol::OpenAiCompletions,
    );
    assert!(
        prepare(&old)
            .unwrap_err()
            .to_string()
            .contains("unsupported Pi version")
    );
    let wrong = request(
        &fixture,
        ClientKind::Codex,
        "0.154.0",
        Protocol::OpenAiCompletions,
    );
    assert!(
        prepare(&wrong)
            .unwrap_err()
            .to_string()
            .contains("requires openai_responses")
    );
    assert!(!fixture.path("home").exists());
}

#[test]
fn reviewed_hash_is_required_and_reconnect_disconnect_preserves_user_conflicts() {
    let fixture = Fixture::new("hash-conflict");
    let settings = fixture.private_file("home/.claude/settings.json", br#"{"env":{},"keep":1}"#);
    let input = request(
        &fixture,
        ClientKind::Claude,
        "2.1.63",
        Protocol::AnthropicMessages,
    );
    let first = prepare(&input).unwrap();
    assert!(apply_reviewed(&first, "wrong-hash").is_err());
    assert_eq!(fs::read(&settings).unwrap(), br#"{"env":{},"keep":1}"#);
    apply_reviewed(&first, &first.preview().hash).unwrap();

    let mut value: serde_json::Value =
        serde_json::from_slice(&fs::read(&settings).unwrap()).unwrap();
    value["env"]["ANTHROPIC_MODEL"] = serde_json::json!("user-model");
    value["keep"] = serde_json::json!(2);
    fs::write(&settings, serde_json::to_vec(&value).unwrap()).unwrap();
    let restored = disconnect(&input.journal_path).unwrap();
    assert_eq!(restored.conflicts.len(), 1);
    let value: serde_json::Value = serde_json::from_slice(&fs::read(settings).unwrap()).unwrap();
    assert_eq!(value["env"]["ANTHROPIC_MODEL"], "user-model");
    assert_eq!(value["keep"], 2);
    assert!(value["env"].get("ANTHROPIC_BASE_URL").is_none());
}

#[test]
fn reconnect_never_adopts_user_changed_pi_or_codex_provider_objects() {
    for (client, version, protocol, relative, pointer) in [
        (
            ClientKind::Pi,
            "0.84.2",
            Protocol::OpenAiCompletions,
            "home/.pi/agent/models.json",
            "/providers/llmgw/baseUrl",
        ),
        (
            ClientKind::Codex,
            "0.154.0",
            Protocol::OpenAiResponses,
            "home/.codex/llmgw.config.toml",
            "/model_providers/llmgw/base_url",
        ),
    ] {
        let fixture = Fixture::new(&format!("reconnect-object-{}", client.as_str()));
        match client {
            ClientKind::Pi => {
                fixture.private_file(relative, br#"{"userBefore":true}"#);
            }
            ClientKind::Codex => {
                fixture.private_file(relative, b"notify = [\"keep\"]\n");
            }
            ClientKind::Claude => unreachable!(),
        }
        let mut input = request(&fixture, client, version, protocol);
        input.set_default = false;
        let first = prepare(&input).unwrap();
        apply_reviewed(&first, &first.preview().hash).unwrap();
        let path = fixture.path(relative);
        let changed = match client {
            ClientKind::Pi => {
                let mut value: serde_json::Value =
                    serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
                *value.pointer_mut(pointer).unwrap() = serde_json::json!("https://user.invalid/v1");
                value["userAdded"] = serde_json::json!(true);
                serde_json::to_vec_pretty(&value).unwrap()
            }
            ClientKind::Codex => {
                let mut value: toml_edit::DocumentMut =
                    fs::read_to_string(&path).unwrap().parse().unwrap();
                value["model_providers"]["llmgw"]["base_url"] =
                    toml_edit::value("https://user.invalid/v1");
                value["user_added"] = toml_edit::value(true);
                value.to_string().into_bytes()
            }
            ClientKind::Claude => unreachable!(),
        };
        fs::write(&path, &changed).unwrap();
        let reconnect = prepare(&input).unwrap_err().to_string();
        assert!(reconnect.contains("changed after the reviewed connection"));
        assert_eq!(fs::read(&path).unwrap(), changed);
        let restored = disconnect(&input.journal_path).unwrap();
        assert_eq!(restored.conflicts.len(), 1);
        match client {
            ClientKind::Pi => {
                let value: serde_json::Value =
                    serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
                assert_eq!(value.pointer(pointer).unwrap(), "https://user.invalid/v1");
                assert_eq!(value["userAdded"], true);
            }
            ClientKind::Codex => {
                let value: toml_edit::DocumentMut =
                    fs::read_to_string(&path).unwrap().parse().unwrap();
                assert_eq!(
                    value["model_providers"]["llmgw"]["base_url"].as_str(),
                    Some("https://user.invalid/v1")
                );
                assert_eq!(value["user_added"].as_bool(), Some(true));
                assert!(value.get("model").is_none());
                assert!(value.get("model_provider").is_none());
            }
            ClientKind::Claude => unreachable!(),
        }
    }
}

fn retired_prepared_provider_plan_is_refused(
    client: ClientKind,
    version: &str,
    protocol: Protocol,
    relative: &str,
) {
    let fixture = Fixture::new(&format!("retired-prepared-{}", client.as_str()));
    match client {
        ClientKind::Pi => {
            fixture.private_file(relative, br#"{"providers":{},"keep":true}"#);
        }
        ClientKind::Codex => {
            fixture.private_file(relative, b"keep = true\n");
        }
        ClientKind::Claude => unreachable!(),
    }
    let mut input = request(&fixture, client, version, protocol);
    input.set_default = false;
    let first = prepare(&input).unwrap();
    apply_reviewed(&first, &first.preview().hash).unwrap();
    let path = fixture.path(relative);
    let managed_bytes = fs::read(&path).unwrap();

    input.model = "second-model".into();
    let retained = prepare(&input).unwrap();
    disconnect(&input.journal_path).unwrap();
    fs::write(&path, &managed_bytes).unwrap();
    let retired_journal = fs::read(&input.journal_path).unwrap();
    assert!(
        prepare(&input).is_err(),
        "a fresh plan must refuse retired ownership"
    );

    let stale_snapshot_accepted = validate_current_snapshot(&retained).is_ok();
    let stale_apply_accepted = apply_reviewed(&retained, &retained.preview().hash).is_ok();
    assert!(
        !stale_snapshot_accepted && !stale_apply_accepted,
        "retired ownership must invalidate a retained {client} plan before activation and apply; snapshot_accepted={stale_snapshot_accepted}, apply_accepted={stale_apply_accepted}"
    );
    assert_eq!(
        fs::read(path).unwrap(),
        managed_bytes,
        "retired retained plan must not change client bytes"
    );
    assert_eq!(
        fs::read(&input.journal_path).unwrap(),
        retired_journal,
        "under-lock ownership refusal must precede journal reset or publication"
    );
}

#[test]
fn pi_retired_journal_after_preview_refuses_recreated_value() {
    retired_prepared_provider_plan_is_refused(
        ClientKind::Pi,
        "0.84.2",
        Protocol::OpenAiCompletions,
        "home/.pi/agent/models.json",
    );
}

#[test]
fn codex_retired_journal_after_preview_refuses_recreated_value() {
    retired_prepared_provider_plan_is_refused(
        ClientKind::Codex,
        "0.154.0",
        Protocol::OpenAiResponses,
        "home/.codex/llmgw.config.toml",
    );
}

#[test]
fn cooperating_disconnect_and_retained_reconnect_never_recreate_a_retired_provider() {
    use std::sync::{Arc, Barrier};

    for (client, version, protocol, relative) in [
        (
            ClientKind::Pi,
            "0.84.2",
            Protocol::OpenAiCompletions,
            "home/.pi/agent/models.json",
        ),
        (
            ClientKind::Codex,
            "0.154.0",
            Protocol::OpenAiResponses,
            "home/.codex/llmgw.config.toml",
        ),
    ] {
        for iteration in 0..8 {
            let fixture = Fixture::new(&format!(
                "provider-disconnect-race-{}-{iteration}",
                client.as_str()
            ));
            match client {
                ClientKind::Pi => {
                    fixture.private_file(relative, br#"{"providers":{},"keep":true}"#);
                }
                ClientKind::Codex => {
                    fixture.private_file(relative, b"keep = true\n");
                }
                ClientKind::Claude => unreachable!(),
            }
            let mut input = request(&fixture, client, version, protocol);
            input.set_default = false;
            let first = prepare(&input).unwrap();
            apply_reviewed(&first, &first.preview().hash).unwrap();
            input.model = "second-model".into();
            let retained = prepare(&input).unwrap();
            let retained_hash = retained.preview().hash.clone();
            let journal = input.journal_path.clone();
            let barrier = Arc::new(Barrier::new(3));
            let apply_barrier = Arc::clone(&barrier);
            let apply = std::thread::spawn(move || {
                apply_barrier.wait();
                apply_reviewed(&retained, &retained_hash)
            });
            let disconnect_barrier = Arc::clone(&barrier);
            let disconnect_journal = journal.clone();
            let restore = std::thread::spawn(move || {
                disconnect_barrier.wait();
                disconnect(&disconnect_journal)
            });
            barrier.wait();
            let apply_result = apply.join().unwrap();
            let restore_result = restore.join().unwrap();
            let path = fixture.path(relative);
            let contains_second = fs::read_to_string(&path).unwrap().contains("second-model");

            if restore_result.is_ok() {
                assert!(
                    !contains_second,
                    "a reconnect must not publish after the cooperating disconnect retired its journal"
                );
            } else if apply_result.is_ok() {
                assert!(contains_second);
                disconnect(&journal).unwrap();
            } else {
                assert!(
                    !contains_second,
                    "failed competing operations must not leave an unowned reconnect published"
                );
            }
        }
    }
}

#[test]
fn restored_journal_never_reauthorizes_a_recreated_provider_object() {
    for (client, version, protocol, relative) in [
        (
            ClientKind::Pi,
            "0.84.2",
            Protocol::OpenAiCompletions,
            "home/.pi/agent/models.json",
        ),
        (
            ClientKind::Codex,
            "0.154.0",
            Protocol::OpenAiResponses,
            "home/.codex/llmgw.config.toml",
        ),
    ] {
        let fixture = Fixture::new(&format!("restored-recreated-{}", client.as_str()));
        let mut input = request(&fixture, client, version, protocol);
        input.set_default = false;
        let plan = prepare(&input).unwrap();
        apply_reviewed(&plan, &plan.preview().hash).unwrap();
        let path = fixture.path(relative);
        let managed_bytes = fs::read(&path).unwrap();
        disconnect(&input.journal_path).unwrap();
        assert!(!path.exists());
        fixture.private_file(relative, &managed_bytes);
        let error = prepare(&input).unwrap_err().to_string();
        assert!(error.contains("not owned by this connection journal"));
    }
}

#[cfg(unix)]
#[test]
fn cli_resolves_native_config_directory_and_explicit_home_precedence() {
    use std::os::unix::fs::PermissionsExt;

    let fixture = Fixture::new("native-config-dir");
    let config = fixture.private_file(
        "gateway/config.toml",
        br#"listen = "127.0.0.1:4141"
concurrency = 1
cancel_policy = "drain"
accounting = "reserved"
[upstream]
api_base = "http://127.0.0.1:9/v1"
[upstream.auth]
mode = "none"
[quota.rpm]
kind = "unlimited"
[quota.tpm]
kind = "unknown"
[[models]]
id = "example-model"
[[roots]]
id = "shared-work"
endpoints = ["chat/completions"]
models = ["example-model"]
"#,
    );
    let loaded = llmgw::config::LoadedConfig::load(&config).unwrap();
    fs::create_dir_all(&loaded.state_paths.directory).unwrap();
    fs::set_permissions(
        &loaded.state_paths.directory,
        fs::Permissions::from_mode(0o700),
    )
    .unwrap();
    fs::write(
        &loaded.state_paths.data_token,
        b"synthetic-local-data-token",
    )
    .unwrap();
    fs::set_permissions(
        &loaded.state_paths.data_token,
        fs::Permissions::from_mode(0o600),
    )
    .unwrap();
    let pi = fixture.private_file("bin/pi", b"#!/bin/sh\nprintf '%s\\n' '0.84.2'\n");
    fs::set_permissions(&pi, fs::Permissions::from_mode(0o700)).unwrap();
    let native = fixture.path("native-pi-agent");
    let default_home = fixture.path("default-home");
    let explicit_home = fixture.path("explicit-home");
    let args = [
        "--config",
        config.to_str().unwrap(),
        "connect",
        "pi",
        "--client-executable",
        pi.to_str().unwrap(),
        "--root",
        "shared-work",
        "--model",
        "example-model",
    ];
    let native_preview = Command::new(env!("CARGO_BIN_EXE_llmgw"))
        .args(args)
        .env("HOME", &default_home)
        .env("PI_CODING_AGENT_DIR", &native)
        .output()
        .unwrap();
    let native_stdout = String::from_utf8(native_preview.stdout).unwrap();
    assert!(native_preview.status.success());
    assert!(native_stdout.contains(native.join("models.json").to_str().unwrap()));
    assert!(native_stdout.contains("PI_CODING_AGENT_DIR environment"));

    let explicit_preview = Command::new(env!("CARGO_BIN_EXE_llmgw"))
        .args(args)
        .args(["--client-home", explicit_home.to_str().unwrap()])
        .env("HOME", &default_home)
        .env("PI_CODING_AGENT_DIR", &native)
        .output()
        .unwrap();
    let explicit_stdout = String::from_utf8(explicit_preview.stdout).unwrap();
    assert!(explicit_preview.status.success());
    assert!(
        explicit_stdout.contains(
            explicit_home
                .join(".pi/agent/models.json")
                .to_str()
                .unwrap()
        )
    );
    assert!(
        explicit_stdout
            .contains("--client-home; later client launches must set PI_CODING_AGENT_DIR=")
    );
    assert!(explicit_stdout.contains(explicit_home.join(".pi/agent").to_str().unwrap()));
    assert!(!explicit_stdout.contains(native.to_str().unwrap()));

    let exact = fixture.path("exact-agent-dir");
    let exact_preview = Command::new(env!("CARGO_BIN_EXE_llmgw"))
        .args(args)
        .args(["--client-config-dir", exact.to_str().unwrap()])
        .env("HOME", &default_home)
        .env("PI_CODING_AGENT_DIR", &native)
        .output()
        .unwrap();
    let exact_stdout = String::from_utf8(exact_preview.stdout).unwrap();
    assert!(exact_preview.status.success());
    assert!(exact_stdout.contains(exact.join("models.json").to_str().unwrap()));
    assert!(
        exact_stdout
            .contains("--client-config-dir; later client launches must set PI_CODING_AGENT_DIR=")
    );

    let userprofile = fixture.path("userprofile-home");
    let userprofile_preview = Command::new(env!("CARGO_BIN_EXE_llmgw"))
        .args(args)
        .env_remove("HOME")
        .env_remove("PI_CODING_AGENT_DIR")
        .env("USERPROFILE", &userprofile)
        .output()
        .unwrap();
    let userprofile_stdout = String::from_utf8(userprofile_preview.stdout).unwrap();
    assert!(userprofile_preview.status.success());
    assert!(
        userprofile_stdout.contains(userprofile.join(".pi/agent/models.json").to_str().unwrap())
    );
    assert!(userprofile_stdout.contains("(USERPROFILE default)"));
}

#[cfg(unix)]
#[test]
fn cli_failed_activation_keeps_reviewed_client_bytes_unchanged() {
    use std::os::unix::fs::PermissionsExt;

    let fixture = Fixture::new("cli-failed-activation");
    let occupied = TcpListener::bind("127.0.0.1:0").unwrap();
    let port = occupied.local_addr().unwrap().port();
    let config = fixture.private_file(
        "gateway/config.toml",
        format!(
            r#"listen = "127.0.0.1:{port}"
concurrency = 1
cancel_policy = "drain"
accounting = "reserved"

[upstream]
api_base = "http://127.0.0.1:9/v1"

[upstream.auth]
mode = "none"

[quota.rpm]
kind = "unlimited"

[quota.tpm]
kind = "unknown"

[[models]]
id = "example-model"
max_output_tokens = 32

[[roots]]
id = "existing-work"
endpoints = ["chat/completions"]
models = ["example-model"]
"#
        )
        .as_bytes(),
    );
    let loaded = llmgw::config::LoadedConfig::load(&config).unwrap();
    fs::create_dir_all(&loaded.state_paths.directory).unwrap();
    fs::set_permissions(
        &loaded.state_paths.directory,
        fs::Permissions::from_mode(0o700),
    )
    .unwrap();
    fs::write(
        &loaded.state_paths.data_token,
        b"synthetic-local-data-token",
    )
    .unwrap();
    fs::set_permissions(
        &loaded.state_paths.data_token,
        fs::Permissions::from_mode(0o600),
    )
    .unwrap();
    let pi = fixture.private_file("bin/pi", b"#!/bin/sh\nprintf '%s\\n' '0.84.2'\n");
    fs::set_permissions(&pi, fs::Permissions::from_mode(0o700)).unwrap();
    let home = fixture.path("client-home");
    let binary = env!("CARGO_BIN_EXE_llmgw");
    let base_args = [
        "--config",
        config.to_str().unwrap(),
        "connect",
        "pi",
        "--client-executable",
        pi.to_str().unwrap(),
        "--client-home",
        home.to_str().unwrap(),
        "--root",
        "shared-work",
        "--model",
        "example-model",
    ];
    let preview = Command::new(binary).args(base_args).output().unwrap();
    assert!(
        preview.status.success(),
        "{}",
        String::from_utf8_lossy(&preview.stderr)
    );
    let stdout = String::from_utf8(preview.stdout).unwrap();
    assert!(stdout.contains("runtime impact: stopped; activation required before client patch"));
    let hash = stdout
        .lines()
        .find_map(|line| line.strip_prefix("preview hash: "))
        .unwrap();
    let models = home.join(".pi/agent/models.json");
    assert!(!models.exists());

    let mut changed_config = fs::read_to_string(&config).unwrap();
    changed_config.push_str("\n# user edit changes the reviewed gateway fingerprint\n");
    fs::write(&config, &changed_config).unwrap();
    let stale = Command::new(binary)
        .args(base_args)
        .args(["--apply-hash", hash, "--restart"])
        .output()
        .unwrap();
    assert!(!stale.status.success());
    assert!(String::from_utf8_lossy(&stale.stderr).contains("preview hash does not match"));
    assert!(!loaded.state_paths.runtime_state.exists());
    assert_eq!(fs::read_to_string(&config).unwrap(), changed_config);
    assert!(!models.exists());

    let reviewed_again = Command::new(binary).args(base_args).output().unwrap();
    assert!(reviewed_again.status.success());
    let reviewed_again_stdout = String::from_utf8(reviewed_again.stdout).unwrap();
    let reviewed_again_hash = reviewed_again_stdout
        .lines()
        .find_map(|line| line.strip_prefix("preview hash: "))
        .unwrap();

    let failed = Command::new(binary)
        .args(base_args)
        .args(["--apply-hash", reviewed_again_hash, "--restart"])
        .output()
        .unwrap();
    assert!(!failed.status.success());
    assert!(
        !models.exists(),
        "client patch must follow authenticated readiness"
    );
    let applied_gateway = fs::read_to_string(config).unwrap();
    assert!(applied_gateway.contains("shared-work"));
    assert!(applied_gateway.contains("chat/completions"));
}

#[cfg(unix)]
#[test]
fn cli_wrong_hash_does_not_restart_a_running_worker_with_pending_route() {
    use std::os::unix::fs::PermissionsExt;

    let fixture = Fixture::new("cli-wrong-hash-running");
    let reserved = TcpListener::bind("127.0.0.1:0").unwrap();
    let port = reserved.local_addr().unwrap().port();
    drop(reserved);
    let config = fixture.private_file(
        "gateway/config.toml",
        format!(
            r#"listen = "127.0.0.1:{port}"
concurrency = 1
cancel_policy = "drain"
accounting = "reserved"

[upstream]
api_base = "http://127.0.0.1:9/v1"

[upstream.auth]
mode = "none"

[quota.rpm]
kind = "unlimited"

[quota.tpm]
kind = "unknown"

[[models]]
id = "example-model"

[[roots]]
id = "existing-work"
endpoints = ["responses"]
models = ["example-model"]
"#
        )
        .as_bytes(),
    );
    let loaded = llmgw::config::LoadedConfig::load(&config).unwrap();
    fs::create_dir_all(&loaded.state_paths.directory).unwrap();
    fs::set_permissions(
        &loaded.state_paths.directory,
        fs::Permissions::from_mode(0o700),
    )
    .unwrap();
    for (path, value) in [
        (&loaded.state_paths.data_token, b"synthetic-data".as_slice()),
        (
            &loaded.state_paths.control_token,
            b"synthetic-control".as_slice(),
        ),
    ] {
        fs::write(path, value).unwrap();
        fs::set_permissions(path, fs::Permissions::from_mode(0o600)).unwrap();
    }
    let pi = fixture.private_file("bin/pi", b"#!/bin/sh\nprintf '%s\\n' '0.84.2'\n");
    fs::set_permissions(&pi, fs::Permissions::from_mode(0o700)).unwrap();
    let home = fixture.path("client-home");
    let binary = env!("CARGO_BIN_EXE_llmgw");
    let on = Command::new(binary)
        .args(["--config", config.to_str().unwrap(), "on"])
        .output()
        .unwrap();
    assert!(
        on.status.success(),
        "{}",
        String::from_utf8_lossy(&on.stderr)
    );
    let status_before = Command::new(binary)
        .args(["--config", config.to_str().unwrap(), "status", "--json"])
        .output()
        .unwrap();
    let before: serde_json::Value = serde_json::from_slice(&status_before.stdout).unwrap();
    assert_eq!(before["state"], "running");

    let connect_args = [
        "--config",
        config.to_str().unwrap(),
        "connect",
        "pi",
        "--client-executable",
        pi.to_str().unwrap(),
        "--client-home",
        home.to_str().unwrap(),
        "--root",
        "pi-work",
        "--model",
        "example-model",
    ];
    let preview = Command::new(binary).args(connect_args).output().unwrap();
    assert!(preview.status.success());
    assert!(String::from_utf8_lossy(&preview.stdout).contains("explicit restart required"));
    let refused = Command::new(binary)
        .args(connect_args)
        .args(["--apply-hash", &"0".repeat(64), "--restart"])
        .output()
        .unwrap();
    assert!(!refused.status.success());
    assert!(String::from_utf8_lossy(&refused.stderr).contains("preview hash does not match"));
    let status_after = Command::new(binary)
        .args(["--config", config.to_str().unwrap(), "status", "--json"])
        .output()
        .unwrap();
    let after: serde_json::Value = serde_json::from_slice(&status_after.stdout).unwrap();
    assert_eq!(after["state"], "running");
    assert_eq!(after["identity"]["pid"], before["identity"]["pid"]);
    assert_eq!(
        after["identity"]["fingerprint"],
        before["identity"]["fingerprint"]
    );
    assert!(!home.join(".pi/agent/models.json").exists());
    let off = Command::new(binary)
        .args(["--config", config.to_str().unwrap(), "off"])
        .output()
        .unwrap();
    assert!(off.status.success());
}

#[cfg(unix)]
#[test]
fn fresh_setup_save_only_allows_all_client_previews_without_manual_token_seeding() {
    use std::os::unix::fs::PermissionsExt;

    let cases = [
        (
            "pi",
            "0.84.2",
            ".pi/agent/models.json",
            "pi-work",
            br#"{"providers":{"other":{"baseUrl":"https://example.invalid","models":[]}}}"#
                .as_slice(),
        ),
        (
            "claude",
            "2.1.63 (Claude Code)",
            ".claude/settings.json",
            "claude-work",
            br#"{"keep":true}"#.as_slice(),
        ),
        (
            "codex",
            "codex-cli 0.154.0",
            ".codex/llmgw.config.toml",
            "codex-work",
            b"notify = [\"keep\"]\n".as_slice(),
        ),
    ];
    let binary = env!("CARGO_BIN_EXE_llmgw");
    let mut failures = Vec::new();

    for (client, version, target, root, existing) in cases {
        let mut fixture = Fixture::new(&format!("fresh-save-only-{client}"));
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        drop(listener);
        let config = fixture.path("gateway/config.toml");
        llmgw::setup::apply(
            &config,
            &fresh_setup_draft(port),
            llmgw::setup::ApplyMode::SaveOnly,
        )
        .unwrap();
        let executable = fixture.private_file(
            &format!("bin/{client}"),
            format!("#!/bin/sh\nprintf '%s\\n' '{version}'\n").as_bytes(),
        );
        fs::set_permissions(&executable, fs::Permissions::from_mode(0o700)).unwrap();
        let home = fixture.path("client-home");
        let target_path = fixture.private_file(&format!("client-home/{target}"), existing);
        let target_before = fs::read(&target_path).unwrap();
        let connect_args = [
            "--config",
            config.to_str().unwrap(),
            "connect",
            client,
            "--client-executable",
            executable.to_str().unwrap(),
            "--client-home",
            home.to_str().unwrap(),
            "--root",
            root,
            "--model",
            "example-model",
        ];
        let output = isolated_cli(binary, &home)
            .args(connect_args)
            .output()
            .unwrap();
        let stdout = String::from_utf8_lossy(&output.stdout);
        let stderr = String::from_utf8_lossy(&output.stderr);
        if !output.status.success() || !stdout.contains("preview hash: ") {
            failures.push(format!(
                "{client}: status={:?}, stdout={stdout:?}, stderr={stderr:?}",
                output.status.code()
            ));
            continue;
        }
        assert_eq!(fs::read(&target_path).unwrap(), target_before);
        let preview_hash = stdout
            .lines()
            .find_map(|line| line.strip_prefix("preview hash: "))
            .unwrap();
        // From activation onward, retain config and protected state unless an
        // authenticated off and parsed stopped receipt both complete.
        fixture.preserve_on_drop = true;
        let applied = isolated_cli(binary, &home)
            .args(connect_args)
            .args(["--apply-hash", preview_hash, "--restart"])
            .output()
            .unwrap();
        let running = isolated_cli(binary, &home)
            .args(["--config", config.to_str().unwrap(), "status", "--json"])
            .output()
            .unwrap();
        let target_exists = target_path.exists();
        let disconnected = isolated_cli(binary, &home)
            .args(["--config", config.to_str().unwrap(), "disconnect", client])
            .output()
            .unwrap();
        let off = isolated_cli(binary, &home)
            .args(["--config", config.to_str().unwrap(), "off"])
            .output()
            .unwrap();
        let stopped = isolated_cli(binary, &home)
            .args(["--config", config.to_str().unwrap(), "status", "--json"])
            .output()
            .unwrap();

        // Assertions follow authenticated cleanup so a failed expectation
        // cannot orphan an owned worker before Fixture removes its state.
        if !off.status.success() {
            panic!(
                "{client}: authenticated cleanup failed; fixture preserved at {}: {}",
                fixture.root.display(),
                String::from_utf8_lossy(&off.stderr)
            );
        }
        let stopped: serde_json::Value =
            serde_json::from_slice(&stopped.stdout).unwrap_or_else(|error| {
                panic!(
                    "{client}: stopped receipt was invalid; fixture preserved at {}: {error}",
                    fixture.root.display()
                )
            });
        assert_eq!(
            stopped["state"],
            "stopped",
            "{client}: stopped receipt did not confirm cleanup; fixture preserved at {}",
            fixture.root.display()
        );
        fixture.preserve_on_drop = false;
        assert!(
            applied.status.success(),
            "{client}: {}",
            String::from_utf8_lossy(&applied.stderr)
        );
        let applied_stdout = String::from_utf8_lossy(&applied.stdout);
        let ready_at = applied_stdout
            .find("gateway config stage: applied; authenticated fingerprint")
            .expect("authenticated config readiness receipt");
        let client_at = applied_stdout
            .find("client patch: applied")
            .expect("client patch receipt");
        assert!(ready_at < client_at);
        assert!(target_exists);
        assert!(
            disconnected.status.success(),
            "{client}: {}",
            String::from_utf8_lossy(&disconnected.stderr)
        );
        let restored = fs::read_to_string(&target_path).unwrap();
        match client {
            "pi" => {
                assert!(restored.contains("\"other\""));
                assert!(!restored.contains("\"llmgw\""));
            }
            "claude" => {
                assert_eq!(
                    serde_json::from_str::<serde_json::Value>(&restored).unwrap()["keep"],
                    true
                );
                assert!(!restored.contains("LLMGW"));
            }
            "codex" => {
                assert!(restored.contains("notify = [\"keep\"]"));
                assert!(!restored.contains("model_providers.llmgw"));
            }
            _ => unreachable!(),
        }
        let running: serde_json::Value = serde_json::from_slice(&running.stdout).unwrap();
        let loaded = llmgw::config::LoadedConfig::load(&config).unwrap();
        assert_eq!(running["state"], "running");
        assert_eq!(
            running["identity"]["fingerprint"],
            loaded.fingerprint.as_str()
        );
    }

    assert!(failures.is_empty(), "{}", failures.join("\n"));
}
