use super::*;
use serde_json::json;
use std::process::Command;

const CLIENT_AUTH_PLACEHOLDER: &str = "llmgw-local-only";

const OWNED_ENV: [&str; 5] = [
    "ANTHROPIC_BASE_URL",
    "ANTHROPIC_MODEL",
    "ANTHROPIC_CUSTOM_HEADERS",
    "ANTHROPIC_API_KEY",
    "CLAUDE_CODE_ENABLE_GATEWAY_MODEL_DISCOVERY",
];

pub(super) fn patches(
    request: &ProfileRequest,
    config_dir: &Path,
) -> Result<AdapterPatches, Error> {
    version_is(&request.installed_version, "2.1.63", "Claude Code")?;
    if request.protocol != Protocol::AnthropicMessages {
        return Err(Error::message(
            "Claude Code 2.1.63 requires anthropic_messages",
        ));
    }
    if request.discover_models {
        return Err(Error::message(
            "Claude Code 2.1.63 model discovery is not verified; leave discovery off",
        ));
    }
    for name in OWNED_ENV {
        if request.effective_environment.contains_key(name) {
            return Err(Error::message(format!(
                "higher-priority environment {name} conflicts with the reviewed Claude settings"
            )));
        }
    }
    for managed in &request.managed_settings {
        if managed_conflicts(managed)? {
            return Err(Error::message(format!(
                "managed policy {} controls an llmgw-owned Claude setting; the policy was not bypassed",
                absolute(managed)?.display()
            )));
        }
    }
    let (path, scope, scope_check) = if let Some(project) = &request.project_local {
        if !project.user_private_confirmed || !project.untracked_confirmed {
            return Err(Error::message(
                "Claude project-local token config requires explicit user-private and untracked confirmations",
            ));
        }
        let path = absolute(&project.directory)?.join(".claude/settings.local.json");
        validate_project_target(&project.directory, &path)?;
        (
            path.clone(),
            Scope::UserPrivate,
            PlanCheck::ClaudeProject {
                project: project.directory.clone(),
                target: path,
            },
        )
    } else {
        let path = config_dir.join("settings.json");
        validate_native_target(&path)?;
        (
            path.clone(),
            Scope::UserPrivate,
            PlanCheck::ClaudeNative { target: path },
        )
    };
    let existing_headers = existing_custom_headers(&path)?;
    let headers = replace_llmgw_header(&existing_headers, &request.local_data_token);
    let base = format!(
        "{}/r/{}",
        request.gateway_origin.trim_end_matches('/'),
        request.root
    );
    let edits = vec![
        set_public(&["env", "ANTHROPIC_BASE_URL"], json!(base))?,
        set_public(&["env", "ANTHROPIC_MODEL"], json!(request.model))?,
        set_token(&["env", "ANTHROPIC_CUSTOM_HEADERS"], &headers)?,
        set_public(
            &["env", "ANTHROPIC_API_KEY"],
            json!(CLIENT_AUTH_PLACEHOLDER),
        )?,
    ];
    Ok((
        vec![patch(path, Format::StrictJson, edits, Vec::new())?],
        base,
        scope,
        vec![
            format!(
                "ANTHROPIC_API_KEY={CLIENT_AUTH_PLACEHOLDER} is a reviewed client-availability placeholder, not upstream authentication; gateway auth none/env strips it before upstream"
            ),
            "model listing was not requested; configured selection, inference, and tools remain separate runtime results".into(),
        ],
        vec![scope_check],
    ))
}

pub(super) fn validate_native_target(target: &Path) -> Result<(), Error> {
    crate::config_patch::validate_local_token_target(target, target.exists())?;
    let canonical_target = crate::config_patch::normalize_resource_path(target)?;
    let Some(repository_hint) = git_repository_ancestor(&canonical_target)? else {
        return Ok(());
    };
    validate_git_nonshared(&repository_hint, &canonical_target)
}

pub(super) fn validate_project_target(project: &Path, target: &Path) -> Result<(), Error> {
    let project = absolute(project)?;
    let metadata = fs::metadata(&project)
        .map_err(|_| Error::message("Claude project-local directory could not be inspected"))?;
    if !metadata.is_dir() {
        return Err(Error::message(
            "Claude project-local directory must be an existing ordinary directory",
        ));
    }
    if fs::symlink_metadata(target)
        .ok()
        .is_some_and(|metadata| metadata.file_type().is_symlink())
    {
        return Err(Error::message(
            "Claude project-local settings target must not be a symlink",
        ));
    }
    crate::config_patch::validate_local_token_target(target, target.exists())?;
    if let Some(parent) = target.parent().filter(|parent| parent.exists()) {
        validate_private_directory(parent)?;
    }

    let canonical_target = crate::config_patch::normalize_resource_path(target)?;
    validate_git_nonshared(&project, &canonical_target)
}

fn git_repository_ancestor(target: &Path) -> Result<Option<PathBuf>, Error> {
    let parent = target
        .parent()
        .ok_or_else(|| Error::message("Claude settings target has no parent directory"))?;
    for ancestor in parent.ancestors() {
        match fs::symlink_metadata(ancestor.join(".git")) {
            Ok(_) => return Ok(Some(ancestor.to_owned())),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(_) => {
                return Err(Error::message(
                    "Claude native settings repository scope could not be inspected",
                ));
            }
        }
    }
    Ok(None)
}

fn validate_git_nonshared(directory: &Path, canonical_target: &Path) -> Result<(), Error> {
    let top = git_output(directory, &["rev-parse", "--show-toplevel"])?;
    if !top.status.success() {
        return Err(Error::message(
            "Claude settings Git repository could not be established; shared/tracking state is unknown",
        ));
    }
    let top_text = String::from_utf8(top.stdout)
        .map_err(|_| Error::message("Claude settings Git root was not UTF-8"))?;
    let top = fs::canonicalize(top_text.trim())
        .map_err(|_| Error::message("Claude settings Git root could not be resolved"))?;
    if !canonical_target.starts_with(&top) {
        return Err(Error::message(
            "Claude settings target is outside the detected Git worktree",
        ));
    }
    let relative = canonical_target.strip_prefix(&top).expect("checked prefix");
    let relative = relative
        .to_str()
        .ok_or_else(|| Error::message("Claude settings path is not UTF-8"))?;
    let literal_relative = format!(":(literal){relative}");
    let tracked = git_output(
        &top,
        &["ls-files", "--error-unmatch", "--", &literal_relative],
    )?;
    match tracked.status.code() {
        Some(0) => {
            return Err(Error::message(
                "Claude settings target is Git-tracked and cannot contain the local token",
            ));
        }
        Some(1) => {}
        _ => {
            return Err(Error::message(
                "Claude settings Git tracking state could not be verified",
            ));
        }
    }
    let ignored = git_output(&top, &["check-ignore", "-q", "--no-index", "--", relative])?;
    match ignored.status.code() {
        Some(0) => Ok(()),
        Some(1) => Err(Error::message(
            "Claude settings target is not ignored by Git and is treated as shared",
        )),
        _ => Err(Error::message(
            "Claude settings Git ignore state could not be verified",
        )),
    }
}

fn git_output(directory: &Path, args: &[&str]) -> Result<std::process::Output, Error> {
    let mut command = Command::new("git");
    command
        .args(args)
        .current_dir(directory)
        .env("GIT_OPTIONAL_LOCKS", "0");
    for name in [
        "GIT_DIR",
        "GIT_WORK_TREE",
        "GIT_INDEX_FILE",
        "GIT_COMMON_DIR",
        "GIT_OBJECT_DIRECTORY",
        "GIT_ALTERNATE_OBJECT_DIRECTORIES",
        "GIT_CEILING_DIRECTORIES",
        "GIT_CONFIG_GLOBAL",
        "GIT_CONFIG_SYSTEM",
        "GIT_CONFIG_NOSYSTEM",
        "GIT_CONFIG_COUNT",
        "GIT_CONFIG_PARAMETERS",
        "GIT_PREFIX",
    ] {
        command.env_remove(name);
    }
    command
        .output()
        .map_err(|_| Error::message("Git is required to verify a Claude repository target"))
}

fn validate_private_directory(path: &Path) -> Result<(), Error> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        let metadata = fs::metadata(path).map_err(|_| {
            Error::message("Claude project-local settings directory could not be inspected")
        })?;
        if metadata.uid() != rustix::process::geteuid().as_raw() || metadata.mode() & 0o077 != 0 {
            return Err(Error::message(
                "Claude project-local settings directory is accessible to other users",
            ));
        }
        #[cfg(target_os = "macos")]
        if !exacl::getfacl(path, exacl::AclOption::SYMLINK_ACL)
            .map_err(|_| {
                Error::message("Claude project-local settings directory ACL could not be inspected")
            })?
            .is_empty()
        {
            return Err(Error::message(
                "Claude project-local settings directory has an extended ACL",
            ));
        }
    }
    Ok(())
}

fn existing_custom_headers(path: &Path) -> Result<String, Error> {
    let (_, bytes) = snapshot(path)?;
    let Some(bytes) = bytes else {
        return Ok(String::new());
    };
    let value: serde_json::Value = serde_json::from_slice(&bytes)
        .map_err(|_| Error::message("Claude settings must be strict JSON"))?;
    Ok(value
        .pointer("/env/ANTHROPIC_CUSTOM_HEADERS")
        .and_then(serde_json::Value::as_str)
        .unwrap_or_default()
        .to_owned())
}

fn replace_llmgw_header(existing: &str, token: &str) -> String {
    let mut lines = existing
        .lines()
        .filter(|line| {
            line.split_once(':')
                .is_none_or(|(name, _)| !name.trim().eq_ignore_ascii_case("x-llmgw-token"))
        })
        .map(str::to_owned)
        .collect::<Vec<_>>();
    lines.push(format!("X-LLMGW-Token: {token}"));
    lines.join("\n")
}

fn managed_conflicts(path: &Path) -> Result<bool, Error> {
    let bytes = match fs::read(path) {
        Ok(bytes) => bytes,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(false),
        Err(error) => return Err(error.into()),
    };
    let value: serde_json::Value = serde_json::from_slice(&bytes)
        .map_err(|_| Error::message("Claude managed policy must be strict JSON"))?;
    Ok(OWNED_ENV
        .iter()
        .any(|name| value.pointer(&format!("/env/{name}")).is_some()))
}
