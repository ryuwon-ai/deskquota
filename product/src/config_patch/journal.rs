use super::{Error, Format, KeyPath, snapshot_hash};
use serde::{Deserialize, Serialize};
use std::{
    io::{Read, Write},
    path::{Path, PathBuf},
};

const JOURNAL_VERSION: u8 = 1;

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum JournalStatus {
    Applying,
    Partial,
    Applied,
    Restoring,
    RestorePartial,
    RestoreConflicts,
    Restored,
    Failed,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum ResourceStage {
    Unstarted,
    BackedUp,
    Written,
    Verified,
    RestoreConflict,
    Restored,
    Preserved,
    Failed,
}

impl ResourceStage {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Unstarted => "unstarted",
            Self::BackedUp => "backed_up",
            Self::Written => "written",
            Self::Verified => "verified",
            Self::RestoreConflict => "restore_conflict",
            Self::Restored => "restored",
            Self::Preserved => "preserved",
            Self::Failed => "failed",
        }
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum KeyStage {
    Owned,
    Conflict,
    Restored,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub(crate) struct OwnedKeyRecord {
    pub key: KeyPath,
    pub before_image: Option<PathBuf>,
    pub written_value_hash: Option<String>,
    pub pending_written_value_hash: Option<String>,
    pub stage: KeyStage,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub(crate) struct ResourceRecord {
    pub path: PathBuf,
    pub format: Format,
    pub original_hash: String,
    pub written_hash: Option<String>,
    pub pending_written_hash: Option<String>,
    pub created: bool,
    pub created_hash: Option<String>,
    pub pending_created_hash: Option<String>,
    pub requires_user_private: bool,
    pub stage: ResourceStage,
    pub owned_keys: Vec<OwnedKeyRecord>,
    pub backup_refs: Vec<PathBuf>,
}

impl ResourceRecord {
    pub(crate) fn has_owned_state(&self) -> bool {
        !self.owned_keys.is_empty()
            || self.written_hash.is_some()
            || self.pending_written_hash.is_some()
            || !self.backup_refs.is_empty()
    }
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub(crate) struct JournalDocument {
    pub version: u8,
    pub status: JournalStatus,
    pub resources: Vec<ResourceRecord>,
}

impl JournalDocument {
    pub fn new() -> Self {
        Self {
            version: JOURNAL_VERSION,
            status: JournalStatus::Applying,
            resources: Vec::new(),
        }
    }

    pub(crate) fn load_or_new_preserving_status(path: &Path) -> Result<Self, Error> {
        match read_private(path) {
            Ok(bytes) => {
                let document: Self = serde_json::from_slice(&bytes)
                    .map_err(|_| Error::message("config patch journal is invalid"))?;
                if document.version != JOURNAL_VERSION {
                    return Err(Error::message(
                        "config patch journal version is unsupported",
                    ));
                }
                Ok(document)
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(Self::new()),
            Err(error) => Err(error.into()),
        }
    }

    pub fn load(path: &Path) -> Result<Self, Error> {
        let bytes = read_private(path)?;
        let document: Self = serde_json::from_slice(&bytes)
            .map_err(|_| Error::message("config patch journal is invalid"))?;
        if document.version != JOURNAL_VERSION {
            return Err(Error::message(
                "config patch journal version is unsupported",
            ));
        }
        Ok(document)
    }

    pub fn save(&self, path: &Path) -> Result<(), Error> {
        let bytes = serde_json::to_vec_pretty(self)
            .map_err(|_| Error::message("config patch journal could not be serialized"))?;
        super::storage::write_private_state_atomic(path, &bytes)
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct JournalResourceSummary {
    pub path: PathBuf,
    pub stage: JournalResourceStage,
    pub owned_keys: Vec<KeyPath>,
    pub backup_refs: Vec<PathBuf>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct JournalResourceStage(ResourceStage);

impl JournalResourceStage {
    pub fn as_str(self) -> &'static str {
        self.0.as_str()
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct JournalSummary {
    pub status: JournalStatus,
    pub resources: Vec<JournalResourceSummary>,
    pub recovery: String,
}

pub fn inspect_journal(path: impl AsRef<Path>) -> Result<JournalSummary, Error> {
    let document = JournalDocument::load(path.as_ref())?;
    let has_unowned_targets = document
        .resources
        .iter()
        .any(|resource| !resource.has_owned_state());
    let recovery = match document.status {
        JournalStatus::Applied => {
            "all client resources were verified; run disconnect/restore to reverse owned keys"
        }
        JournalStatus::Restored if has_unowned_targets => {
            "all owned resources were restored; reviewed failed or unstarted resources were never owned and were left unchanged"
        }
        JournalStatus::Restored => "all owned resources were restored",
        JournalStatus::RestoreConflicts => {
            "some user changes were preserved; inspect conflicts and rerun restore only after review"
        }
        JournalStatus::RestorePartial => {
            "restore did not finish; inspect per-resource stages and retry restore using protected backups"
        }
        JournalStatus::Applying | JournalStatus::Restoring => {
            "the transaction was interrupted; inspect per-resource stages and run restore using protected backups"
        }
        JournalStatus::Partial | JournalStatus::Failed if has_unowned_targets => {
            "the transaction did not fully succeed; reviewed failed or unstarted resources were never owned and remain unchanged; inspect per-resource stages and restore owned resources using protected backups"
        }
        JournalStatus::Partial | JournalStatus::Failed => {
            "the transaction did not fully succeed; inspect per-resource stages and run restore using protected backups"
        }
    }
    .to_owned();
    Ok(JournalSummary {
        status: document.status,
        resources: document
            .resources
            .into_iter()
            .map(|resource| JournalResourceSummary {
                path: resource.path,
                stage: JournalResourceStage(resource.stage),
                owned_keys: resource
                    .owned_keys
                    .into_iter()
                    .map(|owned| owned.key)
                    .collect(),
                backup_refs: resource.backup_refs,
            })
            .collect(),
        recovery,
    })
}

pub(crate) fn owned_key_current_matches(
    journal_path: &Path,
    resource_path: &Path,
    format: Format,
    key: &KeyPath,
) -> Result<Option<bool>, Error> {
    if !journal_path.exists() {
        return Ok(None);
    }
    let document = JournalDocument::load(journal_path)?;
    owned_key_current_matches_in(&document, resource_path, format, key)
}

pub(crate) fn owned_key_current_matches_in(
    document: &JournalDocument,
    resource_path: &Path,
    format: Format,
    key: &KeyPath,
) -> Result<Option<bool>, Error> {
    let normalized = super::storage::normalize_resource_path(resource_path)?;
    let Some(resource) = document
        .resources
        .iter()
        .find(|resource| resource.path == normalized && resource.format == format)
    else {
        return Ok(None);
    };
    let Some(owned) = resource.owned_keys.iter().find(|owned| &owned.key == key) else {
        return Ok(None);
    };
    if !matches!(
        document.status,
        JournalStatus::Applying
            | JournalStatus::Partial
            | JournalStatus::Applied
            | JournalStatus::Failed
    ) || owned.stage != KeyStage::Owned
    {
        return Ok(None);
    }
    let current = super::storage::read_regular(resource_path, "client config")?
        .ok_or_else(|| Error::message("owned client config is missing"))?;
    let semantic = super::document::parse_document(format, &current.bytes)?;
    let current_hash = super::value_hash(super::document::get_value(&semantic, key));
    Ok(Some(
        owned.written_value_hash.as_deref() == Some(current_hash.as_str())
            || owned.pending_written_value_hash.as_deref() == Some(current_hash.as_str()),
    ))
}

pub(crate) fn create_backup(
    journal_path: &Path,
    resource_path: &Path,
    bytes: &[u8],
) -> Result<PathBuf, Error> {
    let state_dir = journal_path
        .parent()
        .ok_or_else(|| Error::message("journal path has no parent directory"))?;
    let stem = snapshot_hash(Some(resource_path.to_string_lossy().as_bytes()));
    let path = state_dir.join(format!(
        "before-{}-{:016x}.bin",
        &stem[..16],
        rand::random::<u64>()
    ));
    let mut file = crate::lifecycle::platform::open(&path, true, true)?;
    file.write_all(bytes)?;
    file.sync_all()?;
    drop(file);
    if read_private(&path)? != bytes {
        return Err(Error::message("protected before-image verification failed"));
    }
    Ok(path)
}

pub(crate) fn read_private(path: &Path) -> std::io::Result<Vec<u8>> {
    let mut file = crate::lifecycle::platform::read(path)?;
    let mut bytes = Vec::new();
    file.read_to_end(&mut bytes)?;
    Ok(bytes)
}

pub(crate) fn ensure_state_parent(path: &Path) -> Result<(), Error> {
    let parent = path
        .parent()
        .ok_or_else(|| Error::message("journal path has no parent directory"))?;
    crate::lifecycle::platform::directory_tree(parent)?;
    Ok(())
}
