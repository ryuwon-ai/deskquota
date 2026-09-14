//! Narrow, version-pinned native client profiles for Pi, Claude Code, and Codex.
//!
//! This module only prepares and applies reviewed `ConfigPatch` transactions.
//! It never starts a client, performs login, resolves config helpers, or changes
//! gateway routes.
mod claude;
mod codex;
mod pi;

use crate::config_patch::{
    ApplyReport, Edit, EditValue, Format, KeyPath, Patch, RestoreReport, Scope, Transaction,
};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeMap,
    fmt, fs,
    path::{Path, PathBuf},
};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ClientKind {
    Pi,
    Claude,
    Codex,
}

impl ClientKind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Pi => "pi",
            Self::Claude => "claude",
            Self::Codex => "codex",
        }
    }
}

impl fmt::Display for ClientKind {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.as_str())
    }
}

impl std::str::FromStr for ClientKind {
    type Err = Error;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value {
            "pi" => Ok(Self::Pi),
            "claude" | "claude-code" => Ok(Self::Claude),
            "codex" => Ok(Self::Codex),
            _ => Err(Error::message("client must be pi, claude, or codex")),
        }
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Protocol {
    OpenAiCompletions,
    AnthropicMessages,
    OpenAiResponses,
}

impl Protocol {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::OpenAiCompletions => "openai_completions",
            Self::AnthropicMessages => "anthropic_messages",
            Self::OpenAiResponses => "openai_responses",
        }
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Verification {
    NotRequested,
    Configured,
    Unverified,
    Verified,
    Unsupported,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ModelMetadata {
    pub context_window: Option<u64>,
    pub max_output_tokens: Option<u64>,
    pub reasoning: Verification,
    pub tools: Verification,
}

impl ModelMetadata {
    pub fn unknown() -> Self {
        Self {
            context_window: None,
            max_output_tokens: None,
            reasoning: Verification::Unverified,
            tools: Verification::Unverified,
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ProjectLocalApproval {
    pub directory: PathBuf,
    pub user_private_confirmed: bool,
    pub untracked_confirmed: bool,
}

#[derive(Clone, Eq, PartialEq)]
pub struct ProfileRequest {
    pub client: ClientKind,
    pub installed_version: String,
    /// The already-resolved native client config directory. Adapters never
    /// consult HOME or client-specific environment variables themselves.
    pub config_dir: PathBuf,
    pub config_dir_source: String,
    pub project_local: Option<ProjectLocalApproval>,
    pub gateway_origin: String,
    pub gateway_fingerprint: String,
    pub root: String,
    pub model: String,
    pub protocol: Protocol,
    pub local_data_token: String,
    pub token_source: PathBuf,
    pub journal_path: PathBuf,
    pub set_default: bool,
    pub discover_models: bool,
    pub metadata: ModelMetadata,
    pub effective_environment: BTreeMap<String, String>,
    pub managed_settings: Vec<PathBuf>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CapabilityResults {
    pub listing: Verification,
    pub selection: Verification,
    pub inference: Verification,
    pub tools: Verification,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ClientPreview {
    pub hash: String,
    pub text: String,
}

#[derive(Clone, Debug)]
pub struct PreparedProfile {
    client: ClientKind,
    transaction: Transaction,
    preview: ClientPreview,
    config_patch_hash: String,
    targets: Vec<PathBuf>,
    capabilities: CapabilityResults,
    plan_checks: Vec<PlanCheck>,
}

#[derive(Clone, Debug)]
enum PlanCheck {
    ClaudeNative {
        target: PathBuf,
    },
    ClaudeProject {
        project: PathBuf,
        target: PathBuf,
    },
    OwnedProvider {
        journal_path: PathBuf,
        requirement: crate::config_patch::OwnedKeyRequirement,
    },
}

type AdapterPatches = (Vec<Patch>, String, Scope, Vec<String>, Vec<PlanCheck>);

impl PlanCheck {
    fn validate(&self) -> Result<(), Error> {
        match self {
            Self::ClaudeNative { target } => claude::validate_native_target(target),
            Self::ClaudeProject { project, target } => {
                claude::validate_project_target(project, target)
            }
            Self::OwnedProvider {
                journal_path,
                requirement,
            } => match crate::config_patch::journal::owned_key_current_matches(
                journal_path,
                &requirement.resource_path,
                requirement.format,
                &requirement.key,
            )? {
                Some(true) => Ok(()),
                Some(false) => Err(Error::message(
                    "the reviewed dedicated provider changed after preview; review the client profile again",
                )),
                None => Err(Error::message(
                    "the reviewed dedicated provider is no longer owned by the active connection journal; review the client profile again",
                )),
            },
        }
    }
}

impl PreparedProfile {
    pub fn client(&self) -> ClientKind {
        self.client
    }

    pub fn preview(&self) -> &ClientPreview {
        &self.preview
    }

    pub fn target_paths(&self) -> &[PathBuf] {
        &self.targets
    }

    pub fn capabilities(&self) -> &CapabilityResults {
        &self.capabilities
    }

    pub fn journal_path(&self) -> &Path {
        &self.transaction.journal_path
    }
}

#[derive(Debug)]
pub struct Error(String);

impl Error {
    fn message(message: impl Into<String>) -> Self {
        Self(message.into())
    }
}

impl fmt::Display for Error {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}

impl std::error::Error for Error {}

impl From<std::io::Error> for Error {
    fn from(value: std::io::Error) -> Self {
        Self(value.to_string())
    }
}

impl From<crate::config_patch::Error> for Error {
    fn from(value: crate::config_patch::Error) -> Self {
        Self(value.to_string())
    }
}

pub fn prepare(request: &ProfileRequest) -> Result<PreparedProfile, Error> {
    validate_common(request)?;
    let config_dir = absolute(&request.config_dir)?;
    let journal_path = absolute(&request.journal_path)?;
    let (patches, public_base, scope, warnings, plan_checks) = match request.client {
        ClientKind::Pi => pi::patches(request, &config_dir)?,
        ClientKind::Claude => claude::patches(request, &config_dir)?,
        ClientKind::Codex => codex::patches(request, &config_dir)?,
    };
    let transaction = Transaction {
        journal_path,
        patches,
    };
    let generic = crate::config_patch::preview(&transaction)?;
    let targets = transaction
        .patches
        .iter()
        .map(|patch| patch.path.clone())
        .collect::<Vec<_>>();
    let target_lines = targets
        .iter()
        .map(|path| format!("  - {}", path.display()))
        .collect::<Vec<_>>()
        .join("\n");
    let warning_lines = warnings
        .iter()
        .map(|warning| format!("warning: {warning}"))
        .collect::<Vec<_>>()
        .join("\n");
    let token_source = absolute(&request.token_source)?;
    let scope_label = match scope {
        Scope::UserPrivate => "user-private",
        Scope::ProjectShared => "project-shared",
    };
    if request.gateway_fingerprint.len() != 64
        || !request
            .gateway_fingerprint
            .bytes()
            .all(|byte| byte.is_ascii_hexdigit())
    {
        return Err(Error::message(
            "gateway fingerprint must be a 64-character hex digest",
        ));
    }
    let mut bound = Sha256::new();
    bound.update(b"llmgw-client-connection-preview-v1\0");
    bound.update(generic.hash.as_bytes());
    bound.update(request.gateway_fingerprint.as_bytes());
    bound.update(request.client.as_str().as_bytes());
    bound.update(request.root.as_bytes());
    bound.update(request.model.as_bytes());
    bound.update(request.protocol.as_str().as_bytes());
    let bound_hash = bound
        .finalize()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect::<String>();
    let text = format!(
        "client: {} {}\nscope: {}\nnative config directory: {} ({})\npublic URL: {}\ngateway config fingerprint: {}\nroot: {} (all clients on this root share one FIFO/fairness share)\nmodel: {}\nprotocol: {}\ntarget files:\n{}\nowned keys:\n{}\ncredential: local data token from {} [REDACTED]\npreview hash: {}\ndisconnect: llmgw disconnect {} --journal {}\nlisting: {}\nselection: configured\ninference: unverified\ntools: {}\n{}{}",
        request.client,
        request.installed_version,
        scope_label,
        config_dir.display(),
        request.config_dir_source,
        public_base,
        request.gateway_fingerprint,
        request.root,
        request.model,
        request.protocol.as_str(),
        target_lines,
        generic.summary,
        token_source.display(),
        bound_hash,
        request.client,
        transaction.journal_path.display(),
        if request.discover_models {
            "configured; runtime unverified"
        } else {
            "not requested"
        },
        verification_label(request.metadata.tools),
        if warning_lines.is_empty() { "" } else { "\n" },
        warning_lines,
    );
    Ok(PreparedProfile {
        client: request.client,
        transaction,
        preview: ClientPreview {
            hash: bound_hash,
            text,
        },
        config_patch_hash: generic.hash,
        targets,
        capabilities: CapabilityResults {
            listing: if request.discover_models {
                Verification::Configured
            } else {
                Verification::NotRequested
            },
            selection: Verification::Configured,
            inference: Verification::Unverified,
            tools: request.metadata.tools,
        },
        plan_checks,
    })
}

pub fn apply_reviewed(plan: &PreparedProfile, reviewed_hash: &str) -> Result<ApplyReport, Error> {
    validate_reviewed_hash(plan, reviewed_hash)?;
    validate_plan_checks(plan)?;
    let requirements = plan
        .plan_checks
        .iter()
        .filter_map(|check| match check {
            PlanCheck::OwnedProvider { requirement, .. } => Some(requirement.clone()),
            PlanCheck::ClaudeNative { .. } | PlanCheck::ClaudeProject { .. } => None,
        })
        .collect::<Vec<_>>();
    Ok(crate::config_patch::apply_with_owned_key_requirements(
        &plan.transaction,
        &plan.config_patch_hash,
        &requirements,
    )?)
}

pub fn validate_reviewed_hash(plan: &PreparedProfile, reviewed_hash: &str) -> Result<(), Error> {
    if plan.preview.hash == reviewed_hash {
        Ok(())
    } else {
        Err(Error::message(
            "preview hash does not match this exact client selection and gateway config fingerprint; review again before activation or client apply",
        ))
    }
}

pub fn validate_current_snapshot(plan: &PreparedProfile) -> Result<(), Error> {
    validate_plan_checks(plan)?;
    let current = crate::config_patch::preview(&plan.transaction)?;
    if current.hash == plan.config_patch_hash {
        Ok(())
    } else {
        Err(Error::message(
            "client config changed after the reviewed preview; review again before gateway activation",
        ))
    }
}

fn validate_plan_checks(plan: &PreparedProfile) -> Result<(), Error> {
    for check in &plan.plan_checks {
        check.validate()?;
    }
    Ok(())
}

pub fn disconnect(journal_path: impl AsRef<Path>) -> Result<RestoreReport, Error> {
    Ok(crate::config_patch::restore(journal_path)?)
}

fn validate_common(request: &ProfileRequest) -> Result<(), Error> {
    if request.root.is_empty()
        || request.model.is_empty()
        || request.local_data_token.is_empty()
        || request.root.contains('/')
    {
        return Err(Error::message(
            "root, model, and local data token must be nonempty; root cannot contain '/'",
        ));
    }
    let origin = url::Url::parse(&request.gateway_origin)
        .map_err(|_| Error::message("gateway origin must be an absolute HTTP URL"))?;
    if !matches!(origin.scheme(), "http" | "https")
        || origin.host_str().is_none()
        || origin.query().is_some()
        || origin.fragment().is_some()
        || origin.path() != "/"
    {
        return Err(Error::message(
            "gateway origin must contain only an HTTP scheme, host, and optional port",
        ));
    }
    Ok(())
}

fn absolute(path: &Path) -> Result<PathBuf, Error> {
    Ok(std::path::absolute(path)?)
}

fn snapshot(path: &Path) -> Result<(String, Option<Vec<u8>>), Error> {
    match fs::read(path) {
        Ok(bytes) => Ok((
            crate::config_patch::snapshot_hash(Some(&bytes)),
            Some(bytes),
        )),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            Ok((crate::config_patch::snapshot_hash(None), None))
        }
        Err(error) => Err(error.into()),
    }
}

fn key(parts: &[&str]) -> Result<KeyPath, Error> {
    KeyPath::new(parts.iter().copied()).map_err(Error::from)
}

fn set_public(parts: &[&str], value: serde_json::Value) -> Result<Edit, Error> {
    Ok(Edit::Set {
        key: key(parts)?,
        value: EditValue::Public(value),
    })
}

fn set_token(parts: &[&str], token: &str) -> Result<Edit, Error> {
    Ok(Edit::Set {
        key: key(parts)?,
        value: EditValue::LocalDataToken(token.to_owned()),
    })
}

fn patch(
    path: PathBuf,
    format: Format,
    edits: Vec<Edit>,
    warnings: Vec<String>,
) -> Result<Patch, Error> {
    let (expected_hash, _) = snapshot(&path)?;
    let owned_keys = edits.iter().map(|edit| edit.key().clone()).collect();
    Ok(Patch {
        path,
        expected_hash,
        format,
        scope: Scope::UserPrivate,
        edits,
        owned_keys,
        warnings,
    })
}

fn version_is(value: &str, expected: &str, client: &str) -> Result<(), Error> {
    if value == expected {
        Ok(())
    } else {
        Err(Error::message(format!(
            "unsupported {client} version {value}; this build only generates the format verified for {expected}"
        )))
    }
}

fn verification_label(value: Verification) -> &'static str {
    match value {
        Verification::NotRequested => "not requested",
        Verification::Configured => "configured; runtime unverified",
        Verification::Unverified => "unverified",
        Verification::Verified => "verified",
        Verification::Unsupported => "unsupported",
    }
}
