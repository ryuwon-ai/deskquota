use super::{Error, RestoreReport, snapshot_hash, value_hash};
use crate::config_patch::{Edit, EditValue};
use crate::config_patch::{document, journal, storage};
use std::{fs, path::Path};

pub fn restore(journal_path: impl AsRef<Path>) -> Result<RestoreReport, Error> {
    let journal_path = journal_path.as_ref();
    journal::ensure_state_parent(journal_path)?;
    let _transaction_lock =
        storage::lock_path(&storage::journal_lock_path(journal_path), "transaction")?;
    let mut document = journal::JournalDocument::load(journal_path)?;
    if document.status == journal::JournalStatus::Restored {
        return Ok(RestoreReport {
            restored_resources: 0,
            conflicts: Vec::new(),
            preserved_created_files: Vec::new(),
            summary: "owned client resources were already restored".into(),
        });
    }
    document.status = journal::JournalStatus::Restoring;
    document.save(journal_path)?;

    let mut restored_resources = 0usize;
    let mut conflicts = Vec::new();
    let mut preserved_created_files = Vec::new();
    for index in 0..document.resources.len() {
        if !document.resources[index].has_owned_state() {
            continue;
        }
        let resource_path = document.resources[index].path.clone();
        let (_resource_lock, normalized_path) = match storage::lock_resource(&resource_path) {
            Ok(locked) => locked,
            Err(error) => {
                return Err(record_partial_restore(
                    &mut document,
                    journal_path,
                    index,
                    error,
                ));
            }
        };
        document.resources[index].path = normalized_path;
        let result = if document.resources[index].created {
            restore_created(
                &mut document.resources[index],
                &mut restored_resources,
                &mut preserved_created_files,
            )
        } else {
            restore_existing(
                &mut document.resources[index],
                &mut restored_resources,
                &mut conflicts,
            )
        };
        if let Err(error) = result {
            return Err(record_partial_restore(
                &mut document,
                journal_path,
                index,
                error,
            ));
        }
        if let Err(error) = document.save(journal_path) {
            return Err(record_partial_restore(
                &mut document,
                journal_path,
                index,
                error,
            ));
        }
    }
    document.status = if conflicts.is_empty() && preserved_created_files.is_empty() {
        journal::JournalStatus::Restored
    } else {
        journal::JournalStatus::RestoreConflicts
    };
    document.save(journal_path)?;
    Ok(RestoreReport {
        restored_resources,
        conflicts,
        preserved_created_files,
        summary: format!(
            "restored {restored_resources} client resource(s); conflicts and changed created files were preserved; journal {}",
            journal_path.display()
        ),
    })
}

fn record_partial_restore(
    document: &mut journal::JournalDocument,
    journal_path: &Path,
    resource_index: usize,
    error: Error,
) -> Error {
    document.resources[resource_index].stage = journal::ResourceStage::Failed;
    document.status = journal::JournalStatus::RestorePartial;
    match document.save(journal_path) {
        Ok(()) => Error::message(format!(
            "restore is partial; journal {} records the failed resource and protected recovery references; inspect it and retry: {error}",
            journal_path.display()
        )),
        Err(save_error) => Error::message(format!(
            "restore is partial and its status could not be recorded in journal {}; inspect the protected journal and retry: {error}; journal update: {save_error}",
            journal_path.display()
        )),
    }
}

fn restore_created(
    resource: &mut journal::ResourceRecord,
    restored_resources: &mut usize,
    preserved: &mut Vec<std::path::PathBuf>,
) -> Result<(), Error> {
    let current = storage::read_regular(&resource.path, "created client config")?;
    match current {
        None => {
            resource.stage = journal::ResourceStage::Restored;
            for owned in &mut resource.owned_keys {
                owned.stage = journal::KeyStage::Restored;
            }
            *restored_resources += 1;
        }
        Some(current)
            if resource.created_hash.as_deref()
                == Some(snapshot_hash(Some(&current.bytes)).as_str())
                || resource.pending_created_hash.as_deref()
                    == Some(snapshot_hash(Some(&current.bytes)).as_str()) =>
        {
            let recheck = storage::read_regular(&resource.path, "created client config")?
                .ok_or_else(|| {
                    Error::message("created client config disappeared during restore")
                })?;
            if recheck.bytes != current.bytes {
                return Err(Error::message(
                    "created client config changed immediately before removal; preserved it",
                ));
            }
            fs::remove_file(&resource.path)?;
            #[cfg(unix)]
            fs::File::open(
                resource
                    .path
                    .parent()
                    .ok_or_else(|| Error::message("created client config has no parent"))?,
            )?
            .sync_all()?;
            resource.stage = journal::ResourceStage::Restored;
            for owned in &mut resource.owned_keys {
                owned.stage = journal::KeyStage::Restored;
            }
            *restored_resources += 1;
        }
        Some(_) => {
            resource.stage = journal::ResourceStage::Preserved;
            preserved.push(resource.path.clone());
        }
    }
    Ok(())
}

fn restore_existing(
    resource: &mut journal::ResourceRecord,
    restored_resources: &mut usize,
    conflicts: &mut Vec<super::KeyPath>,
) -> Result<(), Error> {
    let current = storage::read_regular(&resource.path, "client config")?.ok_or_else(|| {
        Error::message("owned client config is missing; restore cannot recreate it")
    })?;
    let semantic = document::parse_document(resource.format, &current.bytes)?;
    let mut edits = Vec::new();
    let mut restored_after_write = Vec::new();
    for (owned_index, owned) in resource.owned_keys.iter_mut().enumerate() {
        if owned.stage == journal::KeyStage::Restored {
            continue;
        }
        let current_value = document::get_value(&semantic, &owned.key);
        let previous = match &owned.before_image {
            Some(backup) => {
                let bytes = journal::read_private(backup)?;
                let document = document::parse_document(resource.format, &bytes)?;
                document::get_value(&document, &owned.key).cloned()
            }
            None => None,
        };
        let current_hash = value_hash(current_value);
        if current_hash == value_hash(previous.as_ref()) {
            owned.stage = journal::KeyStage::Restored;
            continue;
        }
        if owned.written_value_hash.as_deref() != Some(current_hash.as_str())
            && owned.pending_written_value_hash.as_deref() != Some(current_hash.as_str())
        {
            owned.stage = journal::KeyStage::Conflict;
            conflicts.push(owned.key.clone());
            continue;
        }
        edits.push(match previous {
            Some(value) => Edit::Set {
                key: owned.key.clone(),
                value: EditValue::Public(value),
            },
            None => Edit::Remove {
                key: owned.key.clone(),
            },
        });
        restored_after_write.push(owned_index);
    }
    if edits.is_empty() {
        let fully_restored = resource
            .owned_keys
            .iter()
            .all(|owned| owned.stage == journal::KeyStage::Restored);
        resource.stage = if fully_restored {
            *restored_resources += 1;
            journal::ResourceStage::Restored
        } else {
            journal::ResourceStage::RestoreConflict
        };
        return Ok(());
    }
    let candidate = document::render_edits(resource.format, &current.bytes, &edits)?;
    storage::atomic_write_client(
        &resource.path,
        &candidate,
        Some(&current),
        "client config restore",
        resource.requires_user_private,
    )?;
    let published = storage::read_regular(&resource.path, "restored client config")?
        .ok_or_else(|| Error::message("restored client config disappeared"))?;
    if published.bytes != candidate {
        resource.stage = journal::ResourceStage::Failed;
        return Err(Error::message(
            "restored client config did not match the verified candidate",
        ));
    }
    document::parse_document(resource.format, &published.bytes)?;
    for owned_index in restored_after_write {
        resource.owned_keys[owned_index].stage = journal::KeyStage::Restored;
    }
    resource.stage = if resource
        .owned_keys
        .iter()
        .all(|owned| owned.stage == journal::KeyStage::Restored)
    {
        *restored_resources += 1;
        journal::ResourceStage::Restored
    } else {
        journal::ResourceStage::RestoreConflict
    };
    Ok(())
}
