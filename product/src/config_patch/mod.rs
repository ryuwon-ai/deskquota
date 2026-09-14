//! Focused, reversible edits for supported client JSON and TOML files.
//!
//! Adapters construct reviewed patches. This module never runs config helpers,
//! shells, environment substitutions, client programs, or login flows.
mod apply;
pub(crate) mod document;
pub mod journal;
mod restore;
mod storage;

pub(crate) use apply::apply_with_owned_key_requirements;
pub use apply::{apply, preview};
pub use journal::{JournalStatus, inspect_journal};
pub use restore::restore;
pub use storage::cooperating_lock_path;
pub(crate) use storage::{normalize_resource_path, validate_local_token_target};

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{fmt, path::PathBuf};

const ABSENT_HASH_DOMAIN: &[u8] = b"llmgw-config-patch-absent-v1";

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Format {
    StrictJson,
    JsonWithComments,
    Toml,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Scope {
    UserPrivate,
    ProjectShared,
}

#[derive(Clone, Debug)]
pub(crate) struct OwnedKeyRequirement {
    pub(crate) resource_path: PathBuf,
    pub(crate) format: Format,
    pub(crate) key: KeyPath,
}

#[derive(Clone, Deserialize, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(transparent)]
pub struct KeyPath(Vec<String>);

impl KeyPath {
    pub fn new<'a>(parts: impl IntoIterator<Item = &'a str>) -> Result<Self, Error> {
        let parts = parts.into_iter().map(str::to_owned).collect::<Vec<_>>();
        if parts.is_empty()
            || parts
                .iter()
                .any(|part| part.is_empty() || part.contains('\0'))
        {
            return Err(Error::message(
                "owned key path must contain nonempty components",
            ));
        }
        Ok(Self(parts))
    }

    pub fn components(&self) -> &[String] {
        &self.0
    }

    pub fn display(&self) -> String {
        self.0.join(".")
    }

    fn is_prefix_of(&self, other: &Self) -> bool {
        self.0.len() < other.0.len() && other.0.starts_with(&self.0)
    }
}

impl fmt::Debug for KeyPath {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_tuple("KeyPath")
            .field(&self.display())
            .finish()
    }
}

#[derive(Clone, PartialEq)]
pub enum EditValue {
    Public(serde_json::Value),
    LocalDataToken(String),
    /// A dedicated provider object containing only the local data token and
    /// reviewed public metadata. Its contents are always opaque in output.
    LocalDataTokenObject(serde_json::Value),
    UpstreamCredential(String),
    ControlToken(String),
}

impl fmt::Debug for EditValue {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Public(value) => formatter.debug_tuple("Public").field(value).finish(),
            Self::LocalDataToken(_) => formatter.write_str("LocalDataToken([REDACTED])"),
            Self::LocalDataTokenObject(_) => {
                formatter.write_str("LocalDataTokenObject([REDACTED])")
            }
            Self::UpstreamCredential(_) => formatter.write_str("UpstreamCredential([REDACTED])"),
            Self::ControlToken(_) => formatter.write_str("ControlToken([REDACTED])"),
        }
    }
}

impl EditValue {
    fn value(&self) -> Option<serde_json::Value> {
        match self {
            Self::Public(value) => Some(value.clone()),
            Self::LocalDataToken(value) => Some(serde_json::Value::String(value.clone())),
            Self::LocalDataTokenObject(value) => Some(value.clone()),
            Self::UpstreamCredential(_) | Self::ControlToken(_) => None,
        }
    }

    fn update_preview_hash(&self, hasher: &mut Sha256) {
        match self {
            Self::Public(value) => {
                hasher.update(b"public\0");
                hasher.update(serde_json::to_vec(value).expect("JSON value serializes"));
            }
            Self::LocalDataToken(value) => {
                hasher.update(b"local-data-token\0");
                hasher.update(value.as_bytes());
            }
            Self::LocalDataTokenObject(value) => {
                hasher.update(b"local-data-token-object-v1\0");
                hasher.update(serde_json::to_vec(value).expect("JSON value serializes"));
            }
            Self::UpstreamCredential(value) => {
                hasher.update(b"upstream-credential\0");
                hasher.update(value.as_bytes());
            }
            Self::ControlToken(value) => {
                hasher.update(b"control-token\0");
                hasher.update(value.as_bytes());
            }
        }
    }

    fn is_local_data_token(&self) -> bool {
        matches!(
            self,
            Self::LocalDataToken(_) | Self::LocalDataTokenObject(_)
        )
    }
}

#[derive(Clone, Debug, PartialEq)]
pub enum Edit {
    Set { key: KeyPath, value: EditValue },
    Remove { key: KeyPath },
}

impl Edit {
    pub fn key(&self) -> &KeyPath {
        match self {
            Self::Set { key, .. } | Self::Remove { key } => key,
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct Patch {
    pub path: PathBuf,
    pub expected_hash: String,
    pub format: Format,
    pub scope: Scope,
    pub edits: Vec<Edit>,
    pub owned_keys: Vec<KeyPath>,
    pub warnings: Vec<String>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Transaction {
    pub journal_path: PathBuf,
    pub patches: Vec<Patch>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Preview {
    pub hash: String,
    pub summary: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ApplyReport {
    pub applied_resources: usize,
    pub summary: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RestoreReport {
    pub restored_resources: usize,
    pub conflicts: Vec<KeyPath>,
    pub preserved_created_files: Vec<PathBuf>,
    pub summary: String,
}

#[derive(Debug)]
pub struct Error(String);

impl Error {
    pub(crate) fn message(message: impl Into<String>) -> Self {
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

pub fn snapshot_hash(bytes: Option<&[u8]>) -> String {
    let mut hasher = Sha256::new();
    match bytes {
        Some(bytes) => {
            hasher.update(b"llmgw-config-patch-bytes-v1\0");
            hasher.update(bytes);
        }
        None => hasher.update(ABSENT_HASH_DOMAIN),
    }
    hex_digest(hasher.finalize().as_slice())
}

pub(crate) fn value_hash(value: Option<&serde_json::Value>) -> String {
    let mut hasher = Sha256::new();
    match value {
        Some(value) => {
            hasher.update(b"llmgw-config-patch-value-v1\0");
            hasher.update(serde_json::to_vec(value).expect("JSON value serializes"));
        }
        None => hasher.update(b"llmgw-config-patch-value-absent-v1"),
    }
    hex_digest(hasher.finalize().as_slice())
}

pub(crate) fn hex_digest(bytes: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut encoded = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        encoded.push(HEX[(byte >> 4) as usize] as char);
        encoded.push(HEX[(byte & 0x0f) as usize] as char);
    }
    encoded
}

fn validate_patch_shape(patch: &Patch) -> Result<(), Error> {
    if patch.edits.is_empty() {
        return Err(Error::message("patch must contain at least one edit"));
    }
    let edit_keys = patch.edits.iter().map(Edit::key).collect::<Vec<_>>();
    if edit_keys.len() != patch.owned_keys.len()
        || edit_keys
            .iter()
            .zip(&patch.owned_keys)
            .any(|(edit, owned)| *edit != owned)
    {
        return Err(Error::message(
            "owned_keys must match edit keys in the same order",
        ));
    }
    for (index, key) in patch.owned_keys.iter().enumerate() {
        for other in patch.owned_keys.iter().skip(index + 1) {
            if key == other || key.is_prefix_of(other) || other.is_prefix_of(key) {
                return Err(Error::message(
                    "owned key paths must be unique and must not overlap",
                ));
            }
        }
    }
    for edit in &patch.edits {
        if let Edit::Set { value, .. } = edit {
            match value {
                EditValue::UpstreamCredential(_) => {
                    return Err(Error::message(
                        "upstream credentials cannot be copied into a client config",
                    ));
                }
                EditValue::ControlToken(_) => {
                    return Err(Error::message(
                        "control tokens cannot be copied into a client config",
                    ));
                }
                EditValue::LocalDataToken(_) | EditValue::LocalDataTokenObject(_)
                    if patch.scope != Scope::UserPrivate =>
                {
                    return Err(Error::message(
                        "local data tokens require a user-private client config",
                    ));
                }
                EditValue::LocalDataTokenObject(value) if !value.is_object() => {
                    return Err(Error::message(
                        "local data token object must be a JSON object",
                    ));
                }
                EditValue::Public(_)
                | EditValue::LocalDataToken(_)
                | EditValue::LocalDataTokenObject(_) => {}
            }
        }
    }
    Ok(())
}
