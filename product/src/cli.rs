use crate::config::LoadedConfig;
use clap::{Parser, Subcommand};
use std::{
    collections::BTreeMap,
    io::IsTerminal,
    path::{Path, PathBuf},
    process::{Command as ProcessCommand, ExitCode},
};
#[derive(Debug, Parser)]
#[command(
    name = "llmgw",
    version,
    about = "Run and control a local quota gateway"
)]
pub struct Cli {
    #[arg(long, global = true, value_name = "PATH")]
    config: Option<PathBuf>,
    #[command(subcommand)]
    command: Option<Command>,
}
#[derive(Debug, Subcommand)]
enum Command {
    /// Configure the gateway interactively. No files are changed before final apply.
    Setup,
    /// Run in the foreground until Ctrl-C or authenticated stop.
    Run,
    /// Start one background worker and confirm authenticated readiness.
    On,
    /// Stop this worker; preserve client URLs and login settings.
    Off,
    /// Drain the current worker and apply the current configuration.
    Restart,
    /// Register or unregister start at the next user login without changing the worker.
    Autostart {
        #[command(subcommand)]
        action: AutostartCommand,
    },
    /// Inspect this configuration's worker without calling its upstream.
    Status {
        #[arg(long)]
        json: bool,
    },
    /// Validate without external connections. JSON includes computed state paths.
    Doctor {
        #[arg(long)]
        json: bool,
        /// Perform one bounded TLS and GET /models check against the configured upstream.
        #[arg(long, conflicts_with = "inference")]
        network: bool,
        /// Perform one explicit paid-capable generation smoke. Never runs as part of --network.
        #[arg(long)]
        inference: bool,
        #[arg(long, requires = "inference", required_if_eq("inference", "true"))]
        model: Option<String>,
        #[arg(long, requires = "inference", required_if_eq("inference", "true"), value_parser = clap::value_parser!(u64).range(1..))]
        max_output_tokens: Option<u64>,
    },
    /// Preview or apply a reviewed native client profile.
    Connect {
        #[arg(value_parser = ["pi", "claude", "codex"])]
        client: String,
        #[arg(long, value_name = "PATH")]
        client_executable: Option<PathBuf>,
        #[arg(long, value_name = "DIR")]
        client_home: Option<PathBuf>,
        /// Exact native config directory; overrides --client-home and native environment variables.
        #[arg(long, value_name = "DIR", conflicts_with = "client_home")]
        client_config_dir: Option<PathBuf>,
        #[arg(long)]
        root: String,
        #[arg(long)]
        model: String,
        #[arg(long)]
        apply_hash: Option<String>,
        /// Explicitly start or restart the saved gateway before applying the client patch.
        #[arg(long, requires = "apply_hash")]
        restart: bool,
        #[arg(long)]
        no_default: bool,
        #[arg(long)]
        discover_models: bool,
        #[arg(long, value_name = "DIR")]
        project_dir: Option<PathBuf>,
        #[arg(long, requires = "project_dir")]
        confirm_private_project: bool,
        #[arg(long, requires = "project_dir")]
        confirm_untracked: bool,
        #[arg(long, value_name = "PATH")]
        managed_settings: Vec<PathBuf>,
    },
    /// Restore only values still equal to the selected llmgw client profile.
    Disconnect {
        #[arg(value_parser = ["pi", "claude", "codex"])]
        client: String,
        #[arg(long, value_name = "PATH")]
        journal: Option<PathBuf>,
    },
    #[command(hide = true)]
    Worker {
        #[arg(long)]
        fingerprint: String,
    },
}

#[derive(Debug, Subcommand)]
enum AutostartCommand {
    /// Register the exact foreground run command after preview review.
    On {
        #[arg(long)]
        apply_hash: Option<String>,
    },
    /// Unregister this config after preview review.
    Off {
        #[arg(long)]
        apply_hash: Option<String>,
    },
}
pub fn run() -> Result<ExitCode, crate::lifecycle::Error> {
    let cli = Cli::parse();
    let config = resolve_config(cli.config.as_deref())?;
    let command = cli.command;
    if matches!(command, Some(Command::Setup)) || (command.is_none() && !config.exists()) {
        if !std::io::stdin().is_terminal() || !std::io::stderr().is_terminal() {
            eprintln!(
                "setup required: run `llmgw setup` in a terminal; configuration path: {}",
                config.display()
            );
            return Ok(ExitCode::from(2));
        }
        let existing = if config.exists() {
            match LoadedConfig::load(&config) {
                Ok(loaded) => Some(loaded),
                Err(error) if invalid_config(&error) => {
                    eprintln!("error: {error}");
                    return Ok(ExitCode::from(2));
                }
                Err(error) => return Err(error.into()),
            }
        } else {
            None
        };
        let mut prompts = crate::setup::prompts::DialoguerIo::new(config.clone());
        return match crate::setup::run_wizard(&mut prompts, existing.as_ref())? {
            crate::setup::SetupOutcome::Cancelled => Ok(crate::setup::cancelled_exit()),
            crate::setup::SetupOutcome::Ready { draft, mode } => {
                let selected_clients = draft.clients().to_vec();
                let default_root = draft.config().roots.first().map(|root| root.id.clone());
                let default_model = draft.config().models.first().map(|model| model.id.clone());
                let result = crate::setup::apply(&config, &draft, mode)?;
                println!(
                    "gateway config stage: applied ({:?}); runtime: {}; authenticated readiness: {}",
                    result.mode, result.runtime_state, result.authenticated_readiness
                );
                setup_autostart(&config, draft.login_requested())?;
                if !result.authenticated_readiness {
                    for client in selected_clients {
                        if !matches!(client, crate::setup::ClientIntent::Manual) {
                            println!(
                                "client stage {client:?}: pending; save-only never points clients at an unfinished route"
                            );
                        }
                    }
                    return Ok(ExitCode::SUCCESS);
                }
                let Some(root) = default_root else {
                    return Err(std::io::Error::other("setup has no configured root").into());
                };
                let Some(model) = default_model else {
                    return Err(std::io::Error::other("setup has no configured model").into());
                };
                setup_selected_clients(&config, &selected_clients, &root, &model)
            }
        };
    }
    if !config.exists() && !matches!(&command, Some(Command::Worker { .. })) {
        eprintln!(
            "gateway is not configured at {}; run `llmgw setup --config {}`",
            config.display(),
            config.display()
        );
        return Ok(ExitCode::from(2));
    }
    let command = command.unwrap_or(Command::On);
    if let Command::Doctor {
        json,
        network,
        inference,
        model,
        max_output_tokens,
    } = command
    {
        let loaded = match LoadedConfig::load(&config) {
            Ok(loaded) => loaded,
            Err(error) if invalid_config(&error) => {
                eprintln!("error: {error}");
                return Ok(ExitCode::from(2));
            }
            Err(error) => return Err(error.into()),
        };
        return doctor(
            loaded,
            json,
            network,
            inference,
            model.as_deref(),
            max_output_tokens,
        );
    }
    if let Command::Connect {
        client,
        client_executable,
        client_home,
        client_config_dir,
        root,
        model,
        apply_hash,
        restart,
        no_default,
        discover_models,
        project_dir,
        confirm_private_project,
        confirm_untracked,
        managed_settings,
    } = command
    {
        return connect(ConnectArgs {
            config,
            client,
            client_executable,
            client_home,
            client_config_dir,
            root,
            model,
            apply_hash,
            restart,
            no_default,
            discover_models,
            project_dir,
            confirm_private_project,
            confirm_untracked,
            managed_settings,
            interactive_apply: false,
        });
    }
    if let Command::Disconnect { client, journal } = command {
        return disconnect(&config, &client, journal.as_deref());
    }
    if let Command::Autostart { action } = command {
        return autostart(&config, action);
    }
    if matches!(command, Command::Worker { .. }) {
        crate::lifecycle::prepare_worker()?;
    }
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()?;
    runtime.block_on(async move {
        let json = matches!(command, Command::Status { json: true });
        let off = matches!(command, Command::Off);
        let result = match command {
            Command::Setup => unreachable!(),
            Command::Run => {
                return match crate::lifecycle::worker(&config, None).await {
                    Err(error)
                        if error
                            .downcast_ref::<crate::config::ConfigError>()
                            .is_some_and(invalid_config) =>
                    {
                        eprintln!("error: {error}");
                        Ok(ExitCode::from(2))
                    }
                    result => result.map(|()| ExitCode::SUCCESS),
                };
            }
            Command::Worker { fingerprint } => {
                return match crate::lifecycle::worker(&config, Some(&fingerprint)).await {
                    Err(error) if matches!(error.downcast_ref::<crate::server::StartError>(), Some(crate::server::StartError::Bind(e)) if e.kind() == std::io::ErrorKind::AddrInUse) => Ok(ExitCode::from(crate::lifecycle::WORKER_PORT_IN_USE_EXIT)),
                    result => result.map(|()| ExitCode::SUCCESS),
                };
            }
            Command::On => crate::lifecycle::on(&config).await,
            Command::Off => crate::lifecycle::off(&config).await,
            Command::Restart => {
                println!("Restart cancels queued requests and drains active requests for up to 10 seconds before applying the saved configuration.");
                crate::lifecycle::restart(&config).await
            }
            Command::Autostart { .. } => unreachable!(),
            Command::Status { .. } => crate::lifecycle::status(&config).await,
            Command::Doctor { .. } => unreachable!(),
            Command::Connect { .. } | Command::Disconnect { .. } => unreachable!(),
        };
        let mut value = match result {
            Ok(value) => value,
            Err(error) => {
                if json {
                    let autostart = LoadedConfig::load(&config).map_or_else(
                        |_| serde_json::json!({
                            "registration":"unknown",
                            "reason":"configuration unavailable for registration query",
                            "current_shell_auth_is_login_proof":false,
                        }),
                        |loaded| {
                            crate::autostart::query_current(
                                &loaded.source_path,
                                &loaded.config.upstream.auth,
                            )
                        },
                    );
                    println!(
                        "{}",
                        serde_json::json!({"state":"failed", "error":error.to_string(), "autostart":autostart, "clients":"not_inspected"})
                    );
                }
                if error
                    .downcast_ref::<crate::config::ConfigError>()
                    .is_some_and(invalid_config)
                {
                    eprintln!("error: {error}");
                    return Ok(ExitCode::from(2));
                }
                return Err(error);
            }
        };
        if matches!(command, Command::Status { .. }) {
            if let Ok(clients) = client_statuses(&config) {
                value["clients"] = clients;
            }
            if let Ok(loaded) = LoadedConfig::load(&config) {
                value["autostart"] = crate::autostart::query_current(
                    &loaded.source_path,
                    &loaded.config.upstream.auth,
                );
            }
        }
        if json { println!("{value}"); } else { display(&value, off); }
        Ok(ExitCode::SUCCESS)
    })
}

fn autostart(
    config: &Path,
    command: AutostartCommand,
) -> Result<ExitCode, crate::lifecycle::Error> {
    let (action, apply_hash) = match command {
        AutostartCommand::On { apply_hash } => (crate::autostart::Action::On, apply_hash),
        AutostartCommand::Off { apply_hash } => (crate::autostart::Action::Off, apply_hash),
    };
    let loaded = LoadedConfig::load(config)?;
    let plan = crate::autostart::prepare_current(&loaded.source_path, action)?;
    println!("{}", plan.preview());
    println!("current runtime: unchanged; this command never starts or stops the worker");
    let reviewed = if let Some(hash) = apply_hash {
        if hash != plan.preview_hash() {
            eprintln!("error: apply hash does not match this fresh autostart preview");
            return Ok(ExitCode::from(2));
        }
        true
    } else if std::io::stdin().is_terminal() && std::io::stderr().is_terminal() {
        dialoguer::Confirm::new()
            .with_prompt("Apply this exact user-login registration change?")
            .default(false)
            .interact()?
    } else {
        false
    };
    if !reviewed {
        eprintln!(
            "autostart preview only: rerun with --apply-hash {} or confirm in a terminal",
            plan.preview_hash()
        );
        return Ok(ExitCode::from(2));
    }
    plan.apply()?;
    println!("autostart registration stage: {action:?}; current runtime: unchanged");
    Ok(ExitCode::SUCCESS)
}

fn setup_autostart(config: &Path, requested: bool) -> Result<(), crate::lifecycle::Error> {
    let loaded = LoadedConfig::load(config)?;
    let action = if requested {
        crate::autostart::Action::On
    } else {
        crate::autostart::Action::Off
    };
    let plan = match crate::autostart::prepare_current(&loaded.source_path, action) {
        Ok(plan) => plan,
        Err(error) => {
            println!(
                "autostart registration stage: blocked ({error}); core config and manual on/off remain available"
            );
            return Ok(());
        }
    };
    if !requested && plan.registration_absent() {
        println!("autostart registration stage: disabled; no OS resource change required");
        return Ok(());
    }
    println!("{}", plan.preview());
    println!(
        "login auth availability: {} (current terminal environment is not proof for next login; no token is copied into registration)",
        crate::autostart::login_auth_availability(&loaded.config.upstream.auth),
    );
    let confirmed = dialoguer::Confirm::new()
        .with_prompt("Apply this exact user-login registration change as a separate resource?")
        .default(false)
        .interact()?;
    if !confirmed {
        println!(
            "autostart registration stage: pending; core config and manual on/off remain available"
        );
        return Ok(());
    }
    match plan.apply() {
        Ok(()) => println!(
            "autostart registration stage: applied ({action:?}); current worker was not started or stopped"
        ),
        Err(error) => println!(
            "autostart registration stage: blocked ({error}); core config and manual on/off remain available"
        ),
    }
    Ok(())
}

struct ConnectArgs {
    config: PathBuf,
    client: String,
    client_executable: Option<PathBuf>,
    client_home: Option<PathBuf>,
    client_config_dir: Option<PathBuf>,
    root: String,
    model: String,
    apply_hash: Option<String>,
    restart: bool,
    no_default: bool,
    discover_models: bool,
    project_dir: Option<PathBuf>,
    confirm_private_project: bool,
    confirm_untracked: bool,
    managed_settings: Vec<PathBuf>,
    interactive_apply: bool,
}

fn connect(args: ConnectArgs) -> Result<ExitCode, crate::lifecycle::Error> {
    use crate::clients::{
        ClientKind, ModelMetadata, ProfileRequest, ProjectLocalApproval, Protocol,
    };
    let loaded = LoadedConfig::load(&args.config)?;
    if matches!(loaded.config.upstream.auth, crate::config::Auth::Forward) {
        return Err(std::io::Error::other(
            "automatic client connection is unsupported with upstream auth=forward because existing client subscription or placeholder credentials could reach an arbitrary upstream; configure the client manually",
        )
        .into());
    }
    let client: ClientKind = args.client.parse()?;
    let protocol = match client {
        ClientKind::Pi => Protocol::OpenAiCompletions,
        ClientKind::Claude => Protocol::AnthropicMessages,
        ClientKind::Codex => Protocol::OpenAiResponses,
    };
    let endpoint = match protocol {
        Protocol::OpenAiCompletions => crate::config::Endpoint::ChatCompletions,
        Protocol::AnthropicMessages => crate::config::Endpoint::Messages,
        Protocol::OpenAiResponses => crate::config::Endpoint::Responses,
    };
    let draft = crate::setup::SetupDraft::from_loaded(&loaded)?.ensure_client_route(
        &args.root,
        &args.model,
        endpoint,
    )?;
    let config_changed = draft.config() != &loaded.config;
    let desired_config = draft.config().clone();
    let desired_bytes = draft.render_config()?;
    let desired_fingerprint =
        crate::config::ConfigFingerprint::from_bytes(desired_bytes.as_bytes());
    let route = desired_config
        .roots
        .iter()
        .find(|route| route.id == args.root)
        .expect("ensure_client_route creates the selected root");
    if !route.endpoints.contains(&endpoint) {
        return Err(std::io::Error::other(format!(
            "selected root does not contain required {} endpoint; update and review gateway config first",
            endpoint.path()
        ))
        .into());
    }
    if !route.models.iter().any(|model| model == &args.model)
        || !desired_config
            .models
            .iter()
            .any(|model| model.id == args.model)
    {
        return Err(std::io::Error::other(
            "selected model is not present in both the configured root and model catalog",
        )
        .into());
    }
    let location = resolve_client_location(client, args.client_home, args.client_config_dir)?;
    let executable = args
        .client_executable
        .unwrap_or_else(|| PathBuf::from(client.as_str()));
    let version = installed_version(client, &executable, &location)?;
    let token_bytes = crate::server::read_secret_file(&loaded.state_paths.data_token)?;
    let local_data_token = String::from_utf8(token_bytes)
        .map_err(|_| std::io::Error::other("gateway data token is not UTF-8"))?;
    let local_data_token = local_data_token.trim().to_owned();
    if local_data_token.is_empty() {
        return Err(std::io::Error::other("gateway data token is empty").into());
    }
    let effective_environment = relevant_environment(client);
    let project_local = args.project_dir.map(|directory| ProjectLocalApproval {
        directory,
        user_private_confirmed: args.confirm_private_project,
        untracked_confirmed: args.confirm_untracked,
    });
    let request = ProfileRequest {
        client,
        installed_version: version,
        config_dir: location.config_dir,
        config_dir_source: location.source,
        project_local,
        gateway_origin: format!("http://{}", desired_config.listen),
        gateway_fingerprint: desired_fingerprint.as_str().to_owned(),
        root: args.root,
        model: args.model,
        protocol,
        local_data_token,
        token_source: loaded.state_paths.data_token.clone(),
        journal_path: loaded
            .state_paths
            .directory
            .join("clients")
            .join(format!("{}.journal.json", client.as_str())),
        set_default: !args.no_default,
        discover_models: args.discover_models,
        metadata: ModelMetadata {
            context_window: None,
            max_output_tokens: None,
            reasoning: crate::clients::Verification::Unverified,
            tools: crate::clients::Verification::Unverified,
        },
        effective_environment,
        managed_settings: managed_settings(client, args.managed_settings),
    };
    let plan = crate::clients::prepare(&request)?;
    let config_impact = crate::setup::runtime_impact(&loaded.source_path, &draft)?;
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()?;
    let status = runtime.block_on(crate::lifecycle::status(&loaded.source_path))?;
    if config_changed {
        println!(
            "gateway config stage preview:\n  config: {}\n  listen: {}\n  fixed route: root={} model={} endpoint={}\n  desired fingerprint: {}\n  runtime impact: {:?}",
            loaded.source_path.display(),
            desired_config.listen,
            request.root,
            request.model,
            endpoint.path(),
            desired_fingerprint.as_str(),
            config_impact,
        );
    }
    let reviewed_runtime_action = client_runtime_action(&status, desired_fingerprint.as_str());
    let runtime_impact = runtime_impact_label(reviewed_runtime_action);
    println!("{}", plan.preview().text);
    println!("runtime impact: {runtime_impact}");
    let reviewed_hash = if args.interactive_apply {
        if !dialoguer::Confirm::new()
            .with_prompt(format!(
                "Apply the exact {} client preview with hash {}?",
                client,
                plan.preview().hash
            ))
            .default(false)
            .interact()?
        {
            println!("client stage {client}: pending; client files were unchanged");
            return Ok(ExitCode::from(2));
        }
        plan.preview().hash.clone()
    } else {
        let Some(reviewed_hash) = args.apply_hash else {
            println!("preview only: gateway and client files were unchanged");
            return Ok(ExitCode::SUCCESS);
        };
        reviewed_hash
    };
    crate::clients::validate_reviewed_hash(&plan, &reviewed_hash)?;
    crate::clients::validate_current_snapshot(&plan)?;
    let fresh_status = runtime.block_on(crate::lifecycle::status(&loaded.source_path))?;
    let fresh_runtime_action = client_runtime_action(&fresh_status, desired_fingerprint.as_str());
    if fresh_runtime_action != reviewed_runtime_action {
        return Err(std::io::Error::other(
            "gateway runtime impact changed after the client preview; client files are unchanged; review and confirm the connection again",
        )
        .into());
    }
    if fresh_runtime_action == ClientRuntimeAction::Blocked {
        return Err(std::io::Error::other(
            "gateway authenticated state is unverified after the client confirmation; client files are unchanged",
        )
        .into());
    }
    if config_changed {
        if !args.restart {
            return Err(std::io::Error::other(
                "gateway config stage is pending: apply requires the exact reviewed client hash and --restart before any client patch",
            )
            .into());
        }
        let applied = crate::setup::apply(
            &loaded.source_path,
            &draft,
            config_impact.save_and_start_mode(),
        )?;
        if !applied.authenticated_readiness {
            return Err(std::io::Error::other(
                "gateway config was saved without authenticated readiness; client files are unchanged",
            )
            .into());
        }
        println!(
            "gateway config stage: applied; authenticated fingerprint {}",
            desired_fingerprint.as_str()
        );
    }
    let status = if config_changed {
        runtime.block_on(crate::lifecycle::status(&loaded.source_path))?
    } else {
        fresh_status
    };
    let ready =
        client_runtime_action(&status, desired_fingerprint.as_str()) == ClientRuntimeAction::Ready;
    if config_changed && !ready {
        return Err(std::io::Error::other(
            "gateway authenticated readiness changed after the reviewed config apply; client files are unchanged",
        )
        .into());
    }
    if !ready {
        if !args.restart {
            return Err(std::io::Error::other(
                "client patch is pending: run connect again with the exact reviewed hash and --restart after reviewing runtime impact",
            )
            .into());
        }
        let activated = if status["state"] == "running" {
            runtime.block_on(crate::lifecycle::restart_expected(
                &loaded.source_path,
                desired_fingerprint.as_str(),
            ))?
        } else {
            runtime.block_on(crate::lifecycle::on_expected(
                &loaded.source_path,
                desired_fingerprint.as_str(),
            ))?
        };
        if activated["state"] != "running"
            || activated["identity"]["fingerprint"].as_str() != Some(desired_fingerprint.as_str())
        {
            return Err(std::io::Error::other(
                "authenticated readiness for the reviewed gateway config was not established; client files are unchanged",
            )
            .into());
        }
    }
    let report = crate::clients::apply_reviewed(&plan, &reviewed_hash)?;
    println!("client patch: applied; {}", report.summary);
    Ok(ExitCode::SUCCESS)
}

fn installed_version(
    client: crate::clients::ClientKind,
    executable: &Path,
    location: &ClientLocation,
) -> Result<String, crate::lifecycle::Error> {
    let mut command = ProcessCommand::new(executable);
    command
        .arg("--version")
        .env("HOME", &location.base_home)
        .env("USERPROFILE", &location.base_home);
    match client {
        crate::clients::ClientKind::Pi => {
            command.env("PI_CODING_AGENT_DIR", &location.config_dir);
        }
        crate::clients::ClientKind::Claude => {
            command.env("CLAUDE_CONFIG_DIR", &location.config_dir);
        }
        crate::clients::ClientKind::Codex => {
            command.env("CODEX_HOME", &location.config_dir);
        }
    }
    let output = command.output()?;
    if !output.status.success() {
        return Err(std::io::Error::other("client --version failed").into());
    }
    let stdout = String::from_utf8(output.stdout)
        .map_err(|_| std::io::Error::other("client --version output is not UTF-8"))?;
    stdout
        .split_whitespace()
        .find(|word| {
            word.chars()
                .next()
                .is_some_and(|character| character.is_ascii_digit())
        })
        .map(|word| {
            word.trim_matches(|character: char| !character.is_ascii_digit() && character != '.')
                .to_owned()
        })
        .filter(|value| !value.is_empty())
        .ok_or_else(|| std::io::Error::other("client --version did not contain a version").into())
}

struct ClientLocation {
    base_home: PathBuf,
    config_dir: PathBuf,
    source: String,
}

fn resolve_client_location(
    client: crate::clients::ClientKind,
    explicit_home: Option<PathBuf>,
    explicit_config_dir: Option<PathBuf>,
) -> Result<ClientLocation, crate::lifecycle::Error> {
    let environment_home = std::env::var_os("HOME")
        .or_else(|| std::env::var_os("USERPROFILE"))
        .map(PathBuf::from);
    let native_name = match client {
        crate::clients::ClientKind::Pi => "PI_CODING_AGENT_DIR",
        crate::clients::ClientKind::Claude => "CLAUDE_CONFIG_DIR",
        crate::clients::ClientKind::Codex => "CODEX_HOME",
    };
    let suffix = match client {
        crate::clients::ClientKind::Pi => Path::new(".pi/agent"),
        crate::clients::ClientKind::Claude => Path::new(".claude"),
        crate::clients::ClientKind::Codex => Path::new(".codex"),
    };
    let current_config_dir = if let Some(path) = std::env::var_os(native_name) {
        Some(std::path::absolute(PathBuf::from(path))?)
    } else if let Some(home) = &environment_home {
        Some(std::path::absolute(home)?.join(suffix))
    } else {
        None
    };
    let explicit_home = explicit_home.map(std::path::absolute).transpose()?;
    let (config_dir, source) = if let Some(path) = explicit_config_dir {
        (std::path::absolute(path)?, "--client-config-dir".to_owned())
    } else if let Some(home) = &explicit_home {
        (home.join(suffix), "--client-home".to_owned())
    } else if let Some(path) = std::env::var_os(native_name) {
        (
            std::path::absolute(PathBuf::from(path))?,
            format!("{native_name} environment"),
        )
    } else if let Some(home) = &environment_home {
        (
            std::path::absolute(home)?.join(suffix),
            if std::env::var_os("HOME").is_some() {
                "HOME default".to_owned()
            } else {
                "USERPROFILE default".to_owned()
            },
        )
    } else {
        return Err(std::io::Error::other(
            "client location is unavailable; pass --client-home or --client-config-dir",
        )
        .into());
    };
    let base_home = explicit_home
        .or(environment_home)
        .unwrap_or_else(|| config_dir.clone());
    let source = if current_config_dir.as_ref() == Some(&config_dir) {
        source
    } else {
        format!(
            "{source}; later client launches must set {native_name}={}",
            config_dir.display()
        )
    };
    Ok(ClientLocation {
        base_home,
        config_dir,
        source,
    })
}

fn relevant_environment(client: crate::clients::ClientKind) -> BTreeMap<String, String> {
    let names: &[&str] = match client {
        crate::clients::ClientKind::Pi => &[],
        crate::clients::ClientKind::Claude => &[
            "ANTHROPIC_BASE_URL",
            "ANTHROPIC_MODEL",
            "ANTHROPIC_CUSTOM_HEADERS",
            "ANTHROPIC_API_KEY",
            "CLAUDE_CODE_ENABLE_GATEWAY_MODEL_DISCOVERY",
        ],
        crate::clients::ClientKind::Codex => &[],
    };
    names
        .iter()
        .filter_map(|name| {
            std::env::var(name)
                .ok()
                .map(|value| ((*name).to_owned(), value))
        })
        .collect()
}

fn managed_settings(
    client: crate::clients::ClientKind,
    mut supplied: Vec<PathBuf>,
) -> Vec<PathBuf> {
    if client != crate::clients::ClientKind::Claude {
        return supplied;
    }
    #[cfg(target_os = "macos")]
    supplied.push(PathBuf::from(
        "/Library/Application Support/ClaudeCode/managed-settings.json",
    ));
    #[cfg(all(unix, not(target_os = "macos")))]
    supplied.push(PathBuf::from("/etc/claude-code/managed-settings.json"));
    #[cfg(windows)]
    if let Some(program_data) = std::env::var_os("ProgramData") {
        supplied.push(PathBuf::from(program_data).join("ClaudeCode/managed-settings.json"));
    }
    supplied.sort();
    supplied.dedup();
    supplied
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum ClientRuntimeAction {
    Ready,
    Start,
    Restart,
    Blocked,
}

fn client_runtime_action(status: &serde_json::Value, fingerprint: &str) -> ClientRuntimeAction {
    match status["state"].as_str() {
        Some("running") if status["identity"]["fingerprint"].as_str() == Some(fingerprint) => {
            ClientRuntimeAction::Ready
        }
        Some("running") => ClientRuntimeAction::Restart,
        Some("stopped") => ClientRuntimeAction::Start,
        _ => ClientRuntimeAction::Blocked,
    }
}

fn runtime_impact_label(action: ClientRuntimeAction) -> &'static str {
    match action {
        ClientRuntimeAction::Ready => "running with the reviewed config; no restart required",
        ClientRuntimeAction::Restart => {
            "running with another config; explicit restart required before client patch"
        }
        ClientRuntimeAction::Start => "stopped; activation required before client patch",
        ClientRuntimeAction::Blocked => "unverified; client patch is blocked",
    }
}

fn disconnect(
    config: &Path,
    client: &str,
    journal: Option<&Path>,
) -> Result<ExitCode, crate::lifecycle::Error> {
    let client: crate::clients::ClientKind = client.parse()?;
    let journal = match journal {
        Some(path) => std::path::absolute(path)?,
        None => LoadedConfig::load(config)?
            .state_paths
            .directory
            .join("clients")
            .join(format!("{}.journal.json", client.as_str())),
    };
    let report = crate::clients::disconnect(&journal)?;
    println!("disconnect {}: {}", client, report.summary);
    if !report.conflicts.is_empty() {
        println!("conflicts preserved: {}", report.conflicts.len());
    }
    if !report.preserved_created_files.is_empty() {
        println!(
            "user-changed created files preserved: {}",
            report.preserved_created_files.len()
        );
    }
    Ok(ExitCode::SUCCESS)
}

fn invalid_config(error: &crate::config::ConfigError) -> bool {
    matches!(
        error,
        crate::config::ConfigError::Parse { .. } | crate::config::ConfigError::Validation(_)
    )
}

fn doctor(
    loaded: LoadedConfig,
    json: bool,
    network: bool,
    inference: bool,
    model: Option<&str>,
    max_output_tokens: Option<u64>,
) -> Result<ExitCode, crate::lifecycle::Error> {
    let autostart =
        crate::autostart::query_current(&loaded.source_path, &loaded.config.upstream.auth);
    let auth = match &loaded.config.upstream.auth {
        crate::config::Auth::None => "not required".to_owned(),
        crate::config::Auth::Forward => {
            "incoming client credential required; direct network doctor unsupported".to_owned()
        }
        crate::config::Auth::Env { name, .. } if std::env::var_os(name).is_some() => {
            format!("environment reference {name} available; value hidden")
        }
        crate::config::Auth::Env { name, .. } => {
            format!("environment reference {name} unavailable")
        }
    };
    let ca = loaded.config.upstream.ca_bundle.as_ref().map_or_else(
        || "system trust only".to_owned(),
        |path| {
            format!(
                "system trust plus explicit bundle {} ({})",
                path.display(),
                if path.is_file() {
                    "available"
                } else {
                    "unavailable"
                }
            )
        },
    );
    let port = match std::net::TcpListener::bind(loaded.config.listen) {
        Ok(listener) => {
            drop(listener);
            "available"
        }
        Err(_) => "unavailable (possibly occupied by this gateway or another process)",
    };
    let static_unavailable = matches!(loaded.config.upstream.auth, crate::config::Auth::Env { ref name, .. } if std::env::var_os(name).is_none())
        || loaded
            .config
            .upstream
            .ca_bundle
            .as_ref()
            .is_some_and(|path| !path.is_file());
    if !network && !inference {
        if json {
            println!(
                "{}",
                serde_json::json!({
                    "configuration":"valid", "source_path":loaded.source_path,
                    "fingerprint":loaded.fingerprint.as_str(), "state_directory":loaded.state_paths.directory,
                    "network":"not_requested", "inference":"not_requested", "auth_availability":auth,
                    "tls":ca, "proxy":loaded.config.upstream.proxy.as_ref().map_or("disabled", |_| "explicit"),
                    "port_availability":port, "worker_readiness":"not_checked", "autostart":autostart
                })
            );
        } else {
            println!(
                "configuration valid: {} ({})",
                loaded.source_path.display(),
                loaded.fingerprint.as_str()
            );
            println!("TLS: {ca}");
            println!("auth availability: {auth}");
            println!("port availability: {port}");
            println!("worker readiness: not checked");
            println!(
                "autostart registration: {}; manager: {}; scope: {}; ownership: {}{}",
                autostart["registration"].as_str().unwrap_or("unknown"),
                autostart["manager"].as_str().unwrap_or("unknown"),
                autostart["manager_scope"].as_str().unwrap_or("unknown"),
                autostart["manager_ownership"].as_str().unwrap_or("unknown"),
                autostart["reason"]
                    .as_str()
                    .map_or(String::new(), |reason| format!("; reason: {reason}")),
            );
            println!(
                "login auth availability: {} (current shell is not proof for next login)",
                autostart["login_auth_availability"]
                    .as_str()
                    .unwrap_or("unknown")
            );
            if autostart["registered_binary_available"] == false {
                println!("autostart registered binary: unavailable or moved");
            }
            println!("network: not requested");
            println!("inference: not requested");
        }
        if static_unavailable {
            return Err(std::io::Error::other(
                "required TLS or environment reference is unavailable; values remain hidden",
            )
            .into());
        }
        return Ok(ExitCode::SUCCESS);
    }
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()?;
    if network {
        let listing = runtime.block_on(crate::setup::fetch_models(&loaded.config.upstream))?;
        match listing {
            crate::setup::ModelListing::Available(models) => {
                if json {
                    println!(
                        "{}",
                        serde_json::json!({"configuration":"valid","network":"models_get_succeeded","models":models,"inference":"not_requested","auth_availability":auth,"tls":ca,"worker_readiness":"not_checked","autostart":autostart})
                    );
                } else {
                    println!(
                        "network: bounded GET /models succeeded; inference: not requested\nmodels: {}",
                        models.join(", ")
                    );
                }
                return Ok(ExitCode::SUCCESS);
            }
            crate::setup::ModelListing::Unavailable { status, reason } => {
                if json {
                    println!(
                        "{}",
                        serde_json::json!({"configuration":"valid","network":"models_get_failed","status":status,"error":reason,"inference":"not_requested","autostart":autostart})
                    );
                }
                return Err(std::io::Error::other(reason).into());
            }
        }
    }
    let model =
        model.ok_or_else(|| std::io::Error::other("--model is required with --inference"))?;
    let output = max_output_tokens
        .ok_or_else(|| std::io::Error::other("--max-output-tokens is required with --inference"))?;
    let result = runtime.block_on(crate::setup::smoke_inference(&loaded.config, model, output))?;
    if json {
        println!(
            "{}",
            serde_json::json!({"configuration":"valid","network":"not_requested","inference":"succeeded","inference_calls":1,"model":model,"output_bound":output,"http_status":result.status,"worker_readiness":"not_checked","autostart":autostart})
        );
    } else {
        println!(
            "network listing: not requested\ninference calls: 1\nmodel: {model}\noutput bound: {output}\nHTTP status: {}",
            result.status
        );
    }
    Ok(ExitCode::SUCCESS)
}

fn resolve_config(path: Option<&Path>) -> Result<PathBuf, crate::lifecycle::Error> {
    let path = match path {
        Some(path) => path.to_owned(),
        None => standard_config_path()?,
    };
    Ok(crate::setup::absolute(&path)?)
}

fn standard_config_path() -> Result<PathBuf, crate::lifecycle::Error> {
    #[cfg(target_os = "windows")]
    let base = std::env::var_os("APPDATA").map(PathBuf::from);
    #[cfg(target_os = "macos")]
    let base = std::env::var_os("HOME")
        .map(PathBuf::from)
        .map(|home| home.join("Library/Application Support"));
    #[cfg(all(unix, not(target_os = "macos")))]
    let base = std::env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .or_else(|| {
            std::env::var_os("HOME")
                .map(PathBuf::from)
                .map(|home| home.join(".config"))
        });
    base.map(|base| base.join("llmgw/config.toml"))
        .ok_or_else(|| {
            std::io::Error::other("cannot determine standard user config directory").into()
        })
}

fn display(value: &serde_json::Value, off: bool) {
    println!("gateway: {}", value["state"].as_str().unwrap_or("unknown"));
    if let Some(identity) = value.get("identity") {
        println!(
            "address: {}; fingerprint: {}",
            identity["address"], identity["fingerprint"]
        );
    }
    if value["pending_restart"] == true {
        println!("pending_restart=true: run restart to apply the saved configuration");
    }
    if let Some(admission) = value.pointer("/runtime/admission") {
        println!(
            "quota startup hold: {} ms; shared cooldown: {} ms; queued: {}; active: {}",
            admission["startup_hold_ms"],
            admission["shared_cooldown_ms"],
            admission["queue_length"],
            admission["active"]
        );
    }
    if let Some(clients) = value.get("clients").and_then(serde_json::Value::as_object) {
        for (client, state) in clients {
            println!(
                "client {client}: {}",
                state["state"].as_str().unwrap_or("unknown")
            );
        }
    }
    if let Some(autostart) = value
        .get("autostart")
        .and_then(serde_json::Value::as_object)
    {
        println!(
            "autostart registration: {}; manager: {}; scope: {}; ownership: {}{}",
            autostart
                .get("registration")
                .and_then(serde_json::Value::as_str)
                .unwrap_or("unknown"),
            autostart
                .get("manager")
                .and_then(serde_json::Value::as_str)
                .unwrap_or("unknown"),
            autostart
                .get("manager_scope")
                .and_then(serde_json::Value::as_str)
                .unwrap_or("unknown"),
            autostart
                .get("manager_ownership")
                .and_then(serde_json::Value::as_str)
                .unwrap_or("unknown"),
            autostart
                .get("reason")
                .and_then(serde_json::Value::as_str)
                .map_or(String::new(), |reason| format!("; reason: {reason}")),
        );
        println!(
            "login auth availability: {} (current shell is not proof for next login)",
            autostart
                .get("login_auth_availability")
                .and_then(serde_json::Value::as_str)
                .unwrap_or("unknown")
        );
    }
    if off {
        println!(
            "Client URLs remain configured while the gateway is off. Run `llmgw on` or `llmgw disconnect <client>`; autostart intent is preserved."
        );
    }
}

fn setup_selected_clients(
    config: &Path,
    selected: &[crate::setup::ClientIntent],
    root: &str,
    model: &str,
) -> Result<ExitCode, crate::lifecycle::Error> {
    use dialoguer::Input;
    let mut failed = false;
    for intent in selected {
        let client_kind = match intent {
            crate::setup::ClientIntent::Pi => crate::clients::ClientKind::Pi,
            crate::setup::ClientIntent::ClaudeCode => crate::clients::ClientKind::Claude,
            crate::setup::ClientIntent::Codex => crate::clients::ClientKind::Codex,
            crate::setup::ClientIntent::Manual => {
                println!(
                    "client stage manual: pending; use the reviewed public route and local data-token header"
                );
                continue;
            }
        };
        let client = client_kind.as_str();
        let default_location = resolve_client_location(client_kind, None, None)?;
        let config_dir: String = Input::new()
            .with_prompt(format!(
                "Native config directory for {client}; this exact path will be previewed"
            ))
            .default(default_location.config_dir.display().to_string())
            .interact_text()?;
        let chosen_config_dir = std::path::absolute(PathBuf::from(config_dir))?;
        let explicit_config_dir =
            (chosen_config_dir != default_location.config_dir).then_some(chosen_config_dir);
        let base = ConnectArgs {
            config: config.to_owned(),
            client: client.into(),
            client_executable: None,
            client_home: None,
            client_config_dir: explicit_config_dir,
            root: root.into(),
            model: model.into(),
            apply_hash: None,
            restart: true,
            no_default: false,
            discover_models: false,
            project_dir: None,
            confirm_private_project: false,
            confirm_untracked: false,
            managed_settings: Vec::new(),
            interactive_apply: true,
        };
        match connect(base) {
            Ok(code) if code == ExitCode::SUCCESS => {
                println!("client stage {client}: applied and independently journaled");
            }
            Ok(_) => {
                println!("client stage {client}: pending; gateway config remains applied");
            }
            Err(error) => {
                eprintln!("client stage {client}: failed; gateway config remains applied: {error}");
                failed = true;
            }
        }
    }
    Ok(if failed {
        ExitCode::FAILURE
    } else {
        ExitCode::SUCCESS
    })
}

fn client_statuses(config: &Path) -> Result<serde_json::Value, crate::lifecycle::Error> {
    let loaded = LoadedConfig::load(config)?;
    let mut statuses = serde_json::Map::new();
    for client in ["pi", "claude", "codex"] {
        let path = loaded
            .state_paths
            .directory
            .join("clients")
            .join(format!("{client}.journal.json"));
        let value = if !path.exists() {
            serde_json::json!({"state":"not_connected"})
        } else {
            match crate::config_patch::inspect_journal(&path) {
                Ok(journal) => serde_json::json!({
                    "state": format!("{:?}", journal.status).to_ascii_lowercase(),
                    "resources": journal.resources.len(),
                    "journal": path,
                }),
                Err(_) => serde_json::json!({"state":"invalid_journal","journal":path}),
            }
        };
        statuses.insert(client.into(), value);
    }
    Ok(serde_json::Value::Object(statuses))
}
