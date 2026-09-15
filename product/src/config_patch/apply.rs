use super::{
    ApplyReport, Edit, Error, Format, OwnedKeyRequirement, Patch, Preview, Scope, Transaction,
    document, hex_digest, snapshot_hash, storage, validate_patch_shape, value_hash,
};
use crate::config_patch::journal::{
    JournalDocument, JournalStatus, KeyStage, OwnedKeyRecord, ResourceRecord, ResourceStage,
    create_backup, ensure_state_parent,
};
use sha2::{Digest, Sha256};
use std::{collections::BTreeSet, path::Path};

pub fn preview(transaction: &Transaction) -> Result<Preview, Error> {
    validate_transaction(transaction)?;
    let mut hasher = Sha256::new();
    hasher.update(b"llmgw-config-patch-preview-v1\0");
    hash_path(&mut hasher, &transaction.journal_path);
    let mut lines = Vec::new();
    for patch in &transaction.patches {
        let patch = normalized_patch(patch)?;
        let prepared = prepare_patch(&patch)?;
        hash_path(&mut hasher, &patch.path);
        hasher.update(format_tag(patch.format));
        hasher.update(scope_tag(patch.scope));
        hasher.update(patch.expected_hash.as_bytes());
        hasher.update(snapshot_hash(Some(&prepared.candidate)).as_bytes());
        lines.push(format!(
            "{}: {} owned key(s), {} warning(s)",
            patch.path.display(),
            patch.owned_keys.len(),
            patch.warnings.len()
        ));
        for edit in &patch.edits {
            hasher.update(edit.key().display().as_bytes());
            match edit {
                Edit::Set { value, .. } => {
                    hasher.update(b"set\0");
                    value.update_preview_hash(&mut hasher);
                    lines.push(format!(
                        "  set {} = {}",
                        edit.key().display(),
                        "[reviewed value]"
                    ));
                }
                Edit::Remove { .. } => {
                    hasher.update(b"remove\0");
                    lines.push(format!("  remove {}", edit.key().display()));
                }
            }
        }
        for warning in &patch.warnings {
            hasher.update(b"warning\0");
            hasher.update(warning.as_bytes());
            lines.push(format!("  warning: {warning}"));
        }
    }
    lines.push(
        "cooperating llmgw editors use one stable adjacent lock per normalized client resource; external editor CAS is not guaranteed"
            .into(),
    );
    Ok(Preview {
        hash: hex_digest(hasher.finalize().as_slice()),
        summary: lines.join("\n"),
    })
}

pub fn apply(transaction: &Transaction, reviewed_preview_hash: &str) -> Result<ApplyReport, Error> {
    apply_with_owned_key_requirements(transaction, reviewed_preview_hash, &[])
}

pub(crate) fn apply_with_owned_key_requirements(
    transaction: &Transaction,
    reviewed_preview_hash: &str,
    requirements: &[OwnedKeyRequirement],
) -> Result<ApplyReport, Error> {
    let reviewed = preview(transaction)?;
    if reviewed.hash != reviewed_preview_hash {
        return Err(Error::message(
            "preview hash does not match this exact config patch; review again before applying",
        ));
    }
    ensure_state_parent(&transaction.journal_path)?;
    let _journal_lock = storage::lock_path(
        &storage::journal_lock_path(&transaction.journal_path),
        "transaction",
    )?;
    let existing = JournalDocument::load_or_new_preserving_status(&transaction.journal_path)?;
    validate_owned_key_requirements(&existing, requirements)?;
    let mut journal = if existing.status == JournalStatus::Restored {
        JournalDocument::new()
    } else {
        existing
    };
    plan_reviewed_targets(transaction, &mut journal)?;
    journal.status = JournalStatus::Applying;
    journal.save(&transaction.journal_path)?;

    let mut applied = 0usize;
    for patch in &transaction.patches {
        let result = apply_one(transaction, patch, &mut journal);
        if let Err(error) = result {
            record_unstarted_failure(patch, &mut journal);
            journal.status = if applied == 0 {
                JournalStatus::Failed
            } else {
                JournalStatus::Partial
            };
            let _ = journal.save(&transaction.journal_path);
            let state = if applied == 0 { "failed" } else { "partial" };
            return Err(Error::message(format!(
                "config patch {state}; journal {} records per-resource stages and protected recovery references; run restore after inspection: {error}",
                transaction.journal_path.display()
            )));
        }
        applied += 1;
    }
    journal.status = JournalStatus::Applied;
    journal.save(&transaction.journal_path)?;
    Ok(ApplyReport {
        applied_resources: applied,
        summary: format!(
            "verified {applied} client resource(s); journal {}; client configuration and gateway state remain separate resources",
            transaction.journal_path.display()
        ),
    })
}

fn validate_owned_key_requirements(
    journal: &JournalDocument,
    requirements: &[OwnedKeyRequirement],
) -> Result<(), Error> {
    for requirement in requirements {
        let (_resource_lock, normalized_path) = storage::lock_resource(&requirement.resource_path)?;
        if crate::config_patch::journal::owned_key_current_matches_in(
            journal,
            &normalized_path,
            requirement.format,
            &requirement.key,
        )? != Some(true)
        {
            return Err(Error::message(
                "a reviewed dedicated provider is no longer owned by the active connection journal; review the client profile again",
            ));
        }
    }
    Ok(())
}

fn plan_reviewed_targets(
    transaction: &Transaction,
    journal: &mut JournalDocument,
) -> Result<(), Error> {
    for patch in &transaction.patches {
        let path = storage::normalize_resource_path(&patch.path)?;
        if let Some(resource) = journal
            .resources
            .iter()
            .find(|resource| resource.path == path)
        {
            if resource.format != patch.format {
                return Err(Error::message(
                    "an existing owned resource cannot change document format",
                ));
            }
            for new_key in &patch.owned_keys {
                if resource.owned_keys.iter().any(|owned| {
                    owned.key != *new_key
                        && (owned.key.is_prefix_of(new_key) || new_key.is_prefix_of(&owned.key))
                }) {
                    return Err(Error::message(
                        "a new owned key overlaps an existing owned key; restore the current ownership before changing its shape",
                    ));
                }
            }
            continue;
        }
        journal.resources.push(ResourceRecord {
            path,
            format: patch.format,
            original_hash: patch.expected_hash.clone(),
            written_hash: None,
            pending_written_hash: None,
            created: patch.expected_hash == snapshot_hash(None),
            created_hash: None,
            pending_created_hash: None,
            requires_user_private: patch.scope == Scope::UserPrivate,
            stage: ResourceStage::Unstarted,
            owned_keys: Vec::new(),
            backup_refs: Vec::new(),
        });
    }
    Ok(())
}

fn record_unstarted_failure(patch: &Patch, journal: &mut JournalDocument) {
    let Ok(path) = storage::normalize_resource_path(&patch.path) else {
        return;
    };
    if let Some(resource) = journal
        .resources
        .iter_mut()
        .find(|resource| resource.path == path)
    {
        resource.stage = ResourceStage::Failed;
        return;
    }
    journal.resources.push(ResourceRecord {
        path,
        format: patch.format,
        original_hash: patch.expected_hash.clone(),
        written_hash: None,
        pending_written_hash: None,
        created: patch.expected_hash == snapshot_hash(None),
        created_hash: None,
        pending_created_hash: None,
        requires_user_private: patch.scope == Scope::UserPrivate,
        stage: ResourceStage::Failed,
        owned_keys: Vec::new(),
        backup_refs: Vec::new(),
    });
}

fn apply_one(
    transaction: &Transaction,
    patch: &Patch,
    journal: &mut JournalDocument,
) -> Result<(), Error> {
    let (resource_lock, normalized_path) = storage::lock_resource(&patch.path)?;
    let _resource_lock = resource_lock;
    let mut patch = patch.clone();
    patch.path = normalized_path;
    let patch = &patch;
    let prepared = prepare_patch(patch)?;
    if patch.scope == Scope::UserPrivate {
        storage::validate_private_target(&patch.path, prepared.before.is_some())?;
    }

    let resource_index = journal
        .resources
        .iter()
        .position(|resource| resource.path == patch.path);
    if let Some(index) = resource_index
        && journal.resources[index].created
        && (journal.resources[index].created_hash.is_some()
            || journal.resources[index].pending_created_hash.is_some())
    {
        let before_hash = snapshot_hash(prepared.before.as_deref());
        let still_wholly_owned = journal.resources[index].created_hash.as_deref()
            == Some(before_hash.as_str())
            || journal.resources[index].pending_created_hash.as_deref()
                == Some(before_hash.as_str());
        if !still_wholly_owned {
            journal.resources[index].created = false;
            journal.resources[index].created_hash = None;
            journal.resources[index].pending_created_hash = None;
        }
    }
    let needs_backup = prepared.before.is_some()
        && patch.owned_keys.iter().any(|key| {
            resource_index.is_none_or(|index| {
                !journal.resources[index]
                    .owned_keys
                    .iter()
                    .any(|owned| &owned.key == key)
            })
        });
    let backup = if needs_backup {
        Some(create_backup(
            &transaction.journal_path,
            &patch.path,
            prepared.before.as_deref().expect("backup has source"),
        )?)
    } else {
        None
    };

    if let Some(index) = resource_index
        && !journal.resources[index].has_owned_state()
    {
        let resource = &mut journal.resources[index];
        resource.original_hash = snapshot_hash(prepared.before.as_deref());
        resource.created = prepared.before.is_none();
        resource.created_hash = None;
        resource.pending_created_hash = None;
        resource.requires_user_private = patch.scope == Scope::UserPrivate;
        resource.stage = ResourceStage::Unstarted;
    }

    let index = match resource_index {
        Some(index) => index,
        None => {
            journal.resources.push(ResourceRecord {
                path: patch.path.clone(),
                format: patch.format,
                original_hash: patch.expected_hash.clone(),
                written_hash: None,
                pending_written_hash: Some(snapshot_hash(Some(&prepared.candidate))),
                created: prepared.before.is_none(),
                created_hash: None,
                pending_created_hash: prepared
                    .before
                    .is_none()
                    .then(|| snapshot_hash(Some(&prepared.candidate))),
                requires_user_private: patch.scope == Scope::UserPrivate,
                stage: ResourceStage::Unstarted,
                owned_keys: Vec::new(),
                backup_refs: Vec::new(),
            });
            journal.resources.len() - 1
        }
    };
    if journal.resources[index].format != patch.format {
        return Err(Error::message(
            "an existing owned resource cannot change document format",
        ));
    }
    journal.resources[index].requires_user_private |= patch.scope == Scope::UserPrivate;
    if let Some(backup) = &backup {
        journal.resources[index].backup_refs.push(backup.clone());
    }
    for edit in &patch.edits {
        let written = document::edit_value(edit);
        let written_value_hash = value_hash(written.as_ref());
        if let Some(owned) = journal.resources[index]
            .owned_keys
            .iter_mut()
            .find(|owned| owned.key == *edit.key())
        {
            owned.pending_written_value_hash = Some(written_value_hash);
            owned.stage = KeyStage::Owned;
        } else {
            journal.resources[index].owned_keys.push(OwnedKeyRecord {
                key: edit.key().clone(),
                before_image: backup.clone(),
                written_value_hash: None,
                pending_written_value_hash: Some(written_value_hash),
                stage: KeyStage::Owned,
            });
        }
    }
    journal.resources[index].pending_written_hash = Some(snapshot_hash(Some(&prepared.candidate)));
    if journal.resources[index].created {
        journal.resources[index].pending_created_hash =
            journal.resources[index].pending_written_hash.clone();
    }
    journal.resources[index].stage = ResourceStage::BackedUp;
    journal.save(&transaction.journal_path)?;

    let current = storage::read_regular(&patch.path, "client config")?;
    if snapshot_hash(current.as_ref().map(|snapshot| snapshot.bytes.as_slice()))
        != patch.expected_hash
    {
        journal.resources[index].stage = ResourceStage::Failed;
        journal.save(&transaction.journal_path)?;
        return Err(Error::message(
            "client config changed immediately before replacement; create a new preview",
        ));
    }
    storage::atomic_write_client(
        &patch.path,
        &prepared.candidate,
        current.as_ref(),
        "client config",
        journal.resources[index].requires_user_private,
    )?;
    journal.resources[index].stage = ResourceStage::Written;
    journal.save(&transaction.journal_path)?;
    let published = storage::read_regular(&patch.path, "client config")?
        .ok_or_else(|| Error::message("client config disappeared after replacement"))?;
    if published.bytes != prepared.candidate {
        journal.resources[index].stage = ResourceStage::Failed;
        journal.save(&transaction.journal_path)?;
        return Err(Error::message(
            "published client config does not match the reviewed candidate",
        ));
    }
    document::parse_document(patch.format, &published.bytes)?;
    journal.resources[index].written_hash = journal.resources[index].pending_written_hash.take();
    if journal.resources[index].created {
        journal.resources[index].created_hash =
            journal.resources[index].pending_created_hash.take();
    }
    for key in &patch.owned_keys {
        let owned = journal.resources[index]
            .owned_keys
            .iter_mut()
            .find(|owned| &owned.key == key)
            .expect("patch ownership was recorded before publication");
        owned.written_value_hash = owned.pending_written_value_hash.take();
    }
    journal.resources[index].stage = ResourceStage::Verified;
    journal.save(&transaction.journal_path)?;
    Ok(())
}

struct PreparedPatch {
    before: Option<Vec<u8>>,
    candidate: Vec<u8>,
}

fn prepare_patch(patch: &Patch) -> Result<PreparedPatch, Error> {
    validate_patch_shape(patch)?;
    let before =
        storage::read_regular(&patch.path, "client config")?.map(|snapshot| snapshot.bytes);
    if snapshot_hash(before.as_deref()) != patch.expected_hash {
        return Err(Error::message(
            "client config changed since the reviewed snapshot; create a new preview",
        ));
    }
    let source = before.as_deref().unwrap_or(match patch.format {
        Format::StrictJson | Format::JsonWithComments => b"{}",
        Format::Toml => b"",
    });
    let candidate = document::render_edits(patch.format, source, &patch.edits)?;
    document::parse_document(patch.format, &candidate)?;
    Ok(PreparedPatch { before, candidate })
}

fn validate_transaction(transaction: &Transaction) -> Result<(), Error> {
    if transaction.patches.is_empty() {
        return Err(Error::message(
            "transaction must contain at least one patch",
        ));
    }
    if transaction.journal_path.file_name().is_none() {
        return Err(Error::message("journal path must name a file"));
    }
    let mut paths = BTreeSet::new();
    for patch in &transaction.patches {
        if !paths.insert(storage::normalize_resource_path(&patch.path)?) {
            return Err(Error::message(
                "a transaction cannot patch the same client resource twice",
            ));
        }
        validate_patch_shape(patch)?;
    }
    Ok(())
}

fn normalized_patch(patch: &Patch) -> Result<Patch, Error> {
    let mut normalized = patch.clone();
    normalized.path = storage::normalize_resource_path(&patch.path)?;
    Ok(normalized)
}

fn hash_path(hasher: &mut Sha256, path: &Path) {
    hasher.update(path.to_string_lossy().as_bytes());
    hasher.update(b"\0");
}

fn format_tag(format: Format) -> &'static [u8] {
    match format {
        Format::StrictJson => b"strict-json\0",
        Format::JsonWithComments => b"json-comments-only\0",
        Format::Toml => b"toml\0",
    }
}

fn scope_tag(scope: Scope) -> &'static [u8] {
    match scope {
        Scope::UserPrivate => b"user-private\0",
        Scope::ProjectShared => b"project-shared\0",
    }
}
