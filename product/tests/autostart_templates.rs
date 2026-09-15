#[cfg(target_os = "macos")]
use llmgw::autostart::RegistrationState;
use llmgw::{
    autostart::{
        Action, LoginAuthAvailability, Platform, RegistrationPlan, RegistrationSpec,
        login_auth_availability,
    },
    config::Auth,
};
use std::path::Path;

fn paths() -> (&'static Path, &'static Path) {
    (
        Path::new("/Applications/LLM 게이트웨이 & 도구/llmgw<실행>.exe"),
        Path::new("/Users/테스트 계정/설정 $100% \\\"<&>/gateway 설정.toml"),
    )
}

#[test]
fn absolute_paths_are_checked_for_the_registration_platform() {
    for platform in [Platform::Macos, Platform::Linux] {
        assert!(
            RegistrationSpec::new(platform, Path::new("/bin/llmgw"), Path::new("/config")).is_ok()
        );
        assert!(
            RegistrationSpec::new(platform, Path::new(r"C:\llmgw.exe"), Path::new("/config"))
                .is_err()
        );
    }
    assert!(
        RegistrationSpec::new_for_windows_user(
            Path::new("/bin/llmgw"),
            Path::new(r"C:\config"),
            "S-1-5-21-123"
        )
        .is_err()
    );
}

#[test]
fn macos_launch_agent_has_literal_program_arguments_and_never_restarts() {
    let (binary, config) = paths();
    let spec = RegistrationSpec::new(Platform::Macos, binary, config).unwrap();
    let xml = spec.definition();

    assert!(xml.contains("<key>ProgramArguments</key>"));
    assert!(xml.contains(
        "<string>/Applications/LLM 게이트웨이 &amp; 도구/llmgw&lt;실행&gt;.exe</string>"
    ));
    assert!(xml.contains("<string>--config</string>"));
    assert!(xml.contains(
        "<string>/Users/테스트 계정/설정 $100% \\&quot;&lt;&amp;&gt;/gateway 설정.toml</string>"
    ));
    assert!(xml.contains("<string>run</string>"));
    assert!(xml.contains("<key>RunAtLoad</key>\n<true/>"));
    assert!(xml.contains("<key>KeepAlive</key>\n<false/>"));
    assert!(!xml.contains("EnvironmentVariables"));
    assert!(!xml.contains("api-token-fixture"));
}

#[test]
fn linux_user_unit_uses_foreground_run_literal_argv_and_no_restart() {
    let (binary, config) = paths();
    let spec = RegistrationSpec::new(Platform::Linux, binary, config).unwrap();
    let unit = spec.definition();

    assert!(unit.contains("ExecStart=:"));
    assert!(unit.contains("\"/Applications/LLM 게이트웨이 & 도구/llmgw<실행>.exe\""));
    assert!(unit.contains("\"/Users/테스트 계정/설정 $100%% \\\\\\\"<&>/gateway 설정.toml\""));
    assert!(unit.contains(" --config "));
    assert!(unit.contains(" run\n"));
    assert!(unit.contains("Restart=no"));
    assert!(unit.contains("WantedBy=default.target"));
    assert!(!unit.contains("Environment="));
    assert!(!unit.contains("--now"));
}

#[test]
fn windows_current_user_task_is_interactive_least_privilege_and_continuous() {
    let binary = Path::new(r"C:\Program Files\LLM 게이트웨이 & 도구\llmgw<실행>.exe");
    let config = Path::new(r#"C:\Users\테스트 계정\설정 & "quoted"\gateway.toml"#);
    let user = "S-1-5-21-1000-2000-3000-1001";
    let spec = RegistrationSpec::new_for_windows_user(binary, config, user).unwrap();
    let xml = spec.definition();

    assert!(xml.starts_with("<?xml version=\"1.0\" encoding=\"UTF-16\"?>"));
    assert!(xml.contains("<LogonTrigger>"));
    assert!(xml.contains(&format!("<UserId>{user}</UserId>")));
    assert!(xml.contains("<LogonType>InteractiveToken</LogonType>"));
    assert!(xml.contains("<RunLevel>LeastPrivilege</RunLevel>"));
    assert!(xml.contains("<MultipleInstancesPolicy>IgnoreNew</MultipleInstancesPolicy>"));
    assert!(xml.contains("<ExecutionTimeLimit>PT0S</ExecutionTimeLimit>"));
    assert!(xml.contains("<DisallowStartIfOnBatteries>false</DisallowStartIfOnBatteries>"));
    assert!(xml.contains("<StopIfGoingOnBatteries>false</StopIfGoingOnBatteries>"));
    assert!(xml.contains("<RunOnlyIfIdle>false</RunOnlyIfIdle>"));
    assert!(xml.contains("<RunOnlyIfNetworkAvailable>false</RunOnlyIfNetworkAvailable>"));
    assert!(xml.contains("<WakeToRun>false</WakeToRun>"));
    assert!(xml.contains(
        "<Command>C:\\Program Files\\LLM 게이트웨이 &amp; 도구\\llmgw&lt;실행&gt;.exe</Command>"
    ));
    assert!(xml.contains(r#"<Arguments>--config &quot;C:\Users\테스트 계정\설정 &amp; \&quot;quoted\&quot;\gateway.toml&quot; run</Arguments>"#));
    assert!(!xml.contains("RestartOnFailure"));
    assert!(!xml.contains("Environment"));
    assert!(!xml.contains("api-token-fixture"));
}

#[test]
fn every_registration_identity_is_config_scoped_and_contains_only_argv() {
    let (binary, config) = paths();
    for platform in [Platform::Macos, Platform::Linux] {
        let first = RegistrationSpec::new(platform, binary, config).unwrap();
        let second = RegistrationSpec::new(
            platform,
            binary,
            Path::new("/Users/테스트 계정/other config.toml"),
        )
        .unwrap();
        assert_ne!(first.label(), second.label());
        assert_eq!(
            first.argv(),
            [binary, Path::new("--config"), config, Path::new("run")]
        );
        assert!(!first.definition().contains("api-token-fixture"));
    }
}

#[test]
fn windows_user_identity_is_part_of_the_reviewed_plan_hash() {
    let binary = Path::new(r"C:\Program Files\llmgw.exe");
    let config = Path::new(r"C:\Users\fixture\gateway.toml");
    let home = Path::new(r"C:\Users\fixture");
    let first = RegistrationPlan::prepare_for_windows_user(
        binary,
        config,
        home,
        Action::On,
        "S-1-5-21-1-2-3-1001",
    )
    .unwrap();
    let second = RegistrationPlan::prepare_for_windows_user(
        binary,
        config,
        home,
        Action::On,
        "S-1-5-21-1-2-3-1002",
    )
    .unwrap();
    assert_ne!(first.preview_hash(), second.preview_hash());
    assert!(first.preview().contains("S-1-5-21-1-2-3-1001"));
}

#[test]
fn windows_percent_paths_are_blocked_before_registration() {
    let error = RegistrationSpec::new_for_windows_user(
        Path::new(r"C:\Program Files\%USERNAME%\llmgw.exe"),
        Path::new(r"C:\Users\fixture\gateway.toml"),
        "S-1-5-21-1-2-3-1001",
    )
    .unwrap_err()
    .to_string();
    assert!(error.contains("percent"), "{error}");
}

#[test]
fn registration_rejects_relative_binary_or_config_paths() {
    assert!(RegistrationSpec::new(Platform::Macos, Path::new("llmgw"), Path::new("/a")).is_err());
    assert!(RegistrationSpec::new(Platform::Macos, Path::new("/llmgw"), Path::new("a")).is_err());
}

#[test]
fn unobserved_login_auth_availability_is_unknown() {
    let name = "PATH".to_owned();
    let availability = login_auth_availability(&Auth::Env {
        header: "authorization".into(),
        name: name.clone(),
    });
    assert_eq!(availability, LoginAuthAvailability::Unknown);
    assert_eq!(
        login_auth_availability(&Auth::None),
        LoginAuthAvailability::Available
    );
    assert_eq!(
        login_auth_availability(&Auth::Forward),
        LoginAuthAvailability::Available
    );
}

#[cfg(target_os = "macos")]
#[test]
fn macos_owned_registration_is_file_only_and_reversible() {
    let root = std::env::temp_dir().join(format!(
        "llmgw-autostart-{}-등록 fixture",
        std::process::id()
    ));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).unwrap();
    let executable = root.join("bin folder/llmgw & 실행");
    let config = root.join("config folder/gateway 설정.toml");
    std::fs::create_dir_all(executable.parent().unwrap()).unwrap();
    std::fs::create_dir_all(config.parent().unwrap()).unwrap();
    std::fs::write(&executable, "fixture executable").unwrap();
    std::fs::write(&config, "fixture config").unwrap();

    let on = RegistrationPlan::prepare(Platform::Macos, &executable, &config, &root, Action::On)
        .unwrap();
    assert_eq!(on.state(), RegistrationState::Disabled);
    assert!(on.registration_absent());
    assert_eq!(
        on.manager_commands().len(),
        0,
        "macOS install must not bootstrap"
    );
    on.apply().unwrap();
    assert!(on.target_path().is_file());

    let off = RegistrationPlan::prepare(Platform::Macos, &executable, &config, &root, Action::Off)
        .unwrap();
    assert_eq!(off.state(), RegistrationState::Registered);
    assert!(!off.registration_absent());
    assert_eq!(
        off.manager_commands().len(),
        0,
        "macOS removal must not bootout"
    );
    off.apply().unwrap();
    assert!(!off.target_path().exists());
    std::fs::remove_dir_all(root).unwrap();
}

#[cfg(target_os = "macos")]
#[test]
fn stale_preview_and_foreign_collision_are_refused_without_overwrite() {
    let root = std::env::temp_dir().join(format!(
        "llmgw-autostart-{}-collision fixture",
        std::process::id()
    ));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).unwrap();
    let executable = root.join("llmgw");
    let config = root.join("gateway.toml");
    std::fs::write(&executable, "fixture executable").unwrap();
    std::fs::write(&config, "fixture config").unwrap();
    let plan = RegistrationPlan::prepare(Platform::Macos, &executable, &config, &root, Action::On)
        .unwrap();
    std::fs::create_dir_all(plan.target_path().parent().unwrap()).unwrap();
    std::fs::write(plan.target_path(), "foreign registration").unwrap();
    let error = plan.apply().unwrap_err().to_string();
    assert!(error.contains("changed after preview"), "{error}");
    assert_eq!(
        std::fs::read_to_string(plan.target_path()).unwrap(),
        "foreign registration"
    );

    let collision =
        RegistrationPlan::prepare(Platform::Macos, &executable, &config, &root, Action::Off)
            .unwrap();
    assert_eq!(collision.state(), RegistrationState::Blocked);
    let error = collision.apply().unwrap_err().to_string();
    assert!(error.contains("not owned"), "{error}");
    assert!(plan.target_path().exists());
    std::fs::remove_dir_all(root).unwrap();
}

#[cfg(target_os = "macos")]
#[test]
fn dangling_temporary_symlink_is_refused_without_writing_its_destination() {
    use std::os::unix::fs::symlink;

    let root = std::env::temp_dir().join(format!(
        "llmgw-autostart-{}-dangling temporary fixture",
        std::process::id()
    ));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).unwrap();
    let executable = root.join("llmgw");
    let config = root.join("gateway.toml");
    std::fs::write(&executable, "fixture executable").unwrap();
    std::fs::write(&config, "fixture config").unwrap();
    let plan = RegistrationPlan::prepare(Platform::Macos, &executable, &config, &root, Action::On)
        .unwrap();
    let parent = plan.target_path().parent().unwrap();
    std::fs::create_dir_all(parent).unwrap();
    let temporary = parent.join(format!(
        ".{}.{}.tmp",
        plan.spec().label(),
        std::process::id()
    ));
    let unrelated = root.join("unrelated missing file");
    symlink(&unrelated, &temporary).unwrap();

    let error = plan.apply().unwrap_err().to_string();

    assert!(!unrelated.exists(), "{error}");
    assert!(!plan.target_path().exists(), "{error}");
    assert!(
        std::fs::symlink_metadata(&temporary)
            .unwrap()
            .file_type()
            .is_symlink(),
        "the pre-existing temporary symlink must be preserved"
    );
    std::fs::remove_dir_all(root).unwrap();
}

#[cfg(target_os = "macos")]
#[test]
fn predictable_install_path_collisions_are_preserved() {
    let root = std::env::temp_dir().join(format!(
        "llmgw-autostart-{}-regular temporary fixture",
        std::process::id()
    ));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).unwrap();
    let executable = root.join("llmgw");
    let config = root.join("gateway.toml");
    std::fs::write(&executable, "fixture executable").unwrap();
    std::fs::write(&config, "fixture config").unwrap();
    let plan = RegistrationPlan::prepare(Platform::Macos, &executable, &config, &root, Action::On)
        .unwrap();
    let parent = plan.target_path().parent().unwrap();
    std::fs::create_dir_all(parent).unwrap();
    let temporary = parent.join(format!(
        ".{}.{}.tmp",
        plan.spec().label(),
        std::process::id()
    ));
    std::fs::write(&temporary, "pre-existing fixture").unwrap();

    assert!(plan.apply().is_err());
    assert_eq!(
        std::fs::read_to_string(&temporary).unwrap(),
        "pre-existing fixture"
    );
    assert!(!plan.target_path().exists());
    std::fs::remove_dir_all(root).unwrap();

    let backup_root = std::env::temp_dir().join(format!(
        "llmgw-autostart-{}-dangling backup fixture",
        std::process::id()
    ));
    let _ = std::fs::remove_dir_all(&backup_root);
    std::fs::create_dir_all(&backup_root).unwrap();
    let old_executable = backup_root.join("old llmgw");
    let new_executable = backup_root.join("new llmgw");
    let backup_config = backup_root.join("gateway.toml");
    std::fs::write(&old_executable, "old fixture executable").unwrap();
    std::fs::write(&new_executable, "new fixture executable").unwrap();
    std::fs::write(&backup_config, "fixture config").unwrap();
    let initial = RegistrationPlan::prepare(
        Platform::Macos,
        &old_executable,
        &backup_config,
        &backup_root,
        Action::On,
    )
    .unwrap();
    let initial_definition = initial.spec().definition().to_owned();
    initial.apply().unwrap();
    let replacement = RegistrationPlan::prepare(
        Platform::Macos,
        &new_executable,
        &backup_config,
        &backup_root,
        Action::On,
    )
    .unwrap();
    let backup = replacement.target_path().parent().unwrap().join(format!(
        ".{}.{}.previous",
        replacement.spec().label(),
        std::process::id()
    ));
    let unrelated = backup_root.join("unrelated missing backup destination");
    std::os::unix::fs::symlink(&unrelated, &backup).unwrap();

    let error = replacement.apply().unwrap_err().to_string();

    assert!(!unrelated.exists(), "{error}");
    assert!(
        std::fs::symlink_metadata(&backup)
            .unwrap()
            .file_type()
            .is_symlink(),
        "the pre-existing backup symlink must be preserved"
    );
    assert_eq!(
        std::fs::read_to_string(replacement.target_path()).unwrap(),
        initial_definition
    );
    std::fs::remove_dir_all(backup_root).unwrap();
}

#[cfg(target_os = "macos")]
#[test]
fn registered_missing_or_moved_binary_is_exposed_before_update() {
    let root = std::env::temp_dir().join(format!(
        "llmgw-autostart-{}-moved fixture",
        std::process::id()
    ));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).unwrap();
    let old_executable = root.join("old bin/llmgw");
    let new_executable = root.join("new bin/llmgw");
    let config = root.join("gateway.toml");
    std::fs::create_dir_all(old_executable.parent().unwrap()).unwrap();
    std::fs::create_dir_all(new_executable.parent().unwrap()).unwrap();
    std::fs::write(&old_executable, "old").unwrap();
    std::fs::write(&new_executable, "new").unwrap();
    std::fs::write(&config, "config").unwrap();
    RegistrationPlan::prepare(Platform::Macos, &old_executable, &config, &root, Action::On)
        .unwrap()
        .apply()
        .unwrap();

    let moved =
        RegistrationPlan::prepare(Platform::Macos, &new_executable, &config, &root, Action::On)
            .unwrap();
    assert_eq!(moved.state(), RegistrationState::RegisteredBinaryMoved);
    assert_eq!(
        moved.registered_executable(),
        Some(old_executable.as_path())
    );
    std::fs::remove_file(&old_executable).unwrap();
    let missing =
        RegistrationPlan::prepare(Platform::Macos, &new_executable, &config, &root, Action::On)
            .unwrap();
    assert_eq!(missing.state(), RegistrationState::RegisteredBinaryMissing);
    assert_eq!(
        missing.registered_executable(),
        Some(old_executable.as_path())
    );
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn manager_commands_never_start_runtime_or_escalate_privilege() {
    let (binary, config) = paths();
    let root = Path::new("/Users/테스트 계정");
    let linux =
        RegistrationPlan::prepare(Platform::Linux, binary, config, root, Action::On).unwrap();
    let linux_commands = linux.manager_commands().join("\n");
    assert!(linux_commands.contains("systemctl --user enable"));
    assert!(!linux_commands.contains("--now"));
    assert!(!linux_commands.contains("sudo"));
    assert!(!linux_commands.contains(" start"));

    let linux_off =
        RegistrationPlan::prepare(Platform::Linux, binary, config, root, Action::Off).unwrap();
    let removal_commands = linux_off.manager_commands();
    let removal_preview = removal_commands.join("\n");
    assert!(removal_commands[0].contains(" disable "));
    assert!(removal_commands[1].contains("remove the owned user unit"));
    assert!(removal_commands[2].ends_with("daemon-reload"));
    assert!(removal_commands[3].contains("re-observe remaining influence"));
    assert!(!removal_preview.contains("--now"));
    assert!(!removal_preview.contains(" stop"));

    let windows = RegistrationPlan::prepare_for_windows_user(
        Path::new(r"C:\Program Files\llmgw.exe"),
        Path::new(r"C:\Users\fixture\gateway.toml"),
        Path::new(r"C:\Users\fixture"),
        Action::On,
        "S-1-5-21-1-2-3-1001",
    )
    .unwrap();
    let windows_commands = windows.manager_commands().join("\n");
    assert!(windows_commands.contains("schtasks /Create"));
    assert!(
        !windows_commands.contains(" /F"),
        "a missing-task preview must not authorize overwriting a scheduler collision"
    );
    assert!(!windows_commands.contains(" /Run"));
    assert!(!windows_commands.contains("runas"));
}

#[cfg(target_os = "macos")]
#[test]
fn cli_requires_exact_preview_hash_and_status_reports_registration_separately() {
    use std::process::Command;
    let root = std::env::temp_dir().join(format!(
        "llmgw-autostart-{}-cli fixture",
        std::process::id()
    ));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).unwrap();
    let config = root.join("gateway 설정.toml");
    std::fs::copy(
        Path::new(env!("CARGO_MANIFEST_DIR")).join("examples/fixture.toml"),
        &config,
    )
    .unwrap();
    let run = |args: &[&str]| {
        let mut command = Command::new(env!("CARGO_BIN_EXE_llmgw"));
        command
            .env_clear()
            .env("HOME", &root)
            .args(args)
            .arg("--config")
            .arg(&config);
        command.output().unwrap()
    };

    let preview = run(&["autostart", "on"]);
    assert_eq!(preview.status.code(), Some(2), "{preview:?}");
    let preview_text = String::from_utf8(preview.stdout).unwrap();
    assert!(preview_text.contains("current runtime: unchanged"));
    let hash = preview_text
        .lines()
        .find_map(|line| line.strip_prefix("preview hash: "))
        .unwrap();
    let applied = run(&["autostart", "on", "--apply-hash", hash]);
    assert!(applied.status.success(), "{applied:?}");

    let status = run(&["status", "--json"]);
    assert!(status.status.success(), "{status:?}");
    let status: serde_json::Value = serde_json::from_slice(&status.stdout).unwrap();
    assert_eq!(status["state"], "stopped");
    assert_eq!(status["autostart"]["registration"], "registered");
    assert_eq!(status["autostart"]["login_auth_availability"], "available");

    let off_preview = run(&["autostart", "off"]);
    let off_text = String::from_utf8(off_preview.stdout).unwrap();
    let off_hash = off_text
        .lines()
        .find_map(|line| line.strip_prefix("preview hash: "))
        .unwrap();
    let removed = run(&["autostart", "off", "--apply-hash", off_hash]);
    assert!(removed.status.success(), "{removed:?}");
    let status = run(&["status", "--json"]);
    let status: serde_json::Value = serde_json::from_slice(&status.stdout).unwrap();
    assert_eq!(status["autostart"]["registration"], "disabled");
    let text_status = run(&["status"]);
    let text_status = String::from_utf8(text_status.stdout).unwrap();
    assert!(text_status.contains("autostart registration: disabled"));
    assert!(text_status.contains("login auth availability: available"));
    std::fs::remove_dir_all(root).unwrap();
}
