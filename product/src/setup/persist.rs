//! Final setup apply: snapshot conflicts, cooperating-writer lock, and protected atomic publication.
use super::*;
#[cfg(any(windows, test))]
use crate::file_replace::replace_existing_with_recovery;
use fs4::FileExt;
use serde::{Deserialize, Serialize};
use std::{
    ffi::OsString,
    fs::{self, OpenOptions},
    io::{Read, Write},
    path::{Path, PathBuf},
};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ApplyMode {
    SaveOnly,
    SaveAndStart,
    SaveAndRestart,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum RuntimeImpact {
    NotInspected {
        desired_fingerprint: String,
    },
    NewConfig {
        desired_fingerprint: String,
    },
    Stopped {
        desired_fingerprint: String,
    },
    Matching {
        fingerprint: String,
    },
    RestartRequired {
        desired_fingerprint: String,
        worker_fingerprint: String,
    },
    Unverified {
        desired_fingerprint: String,
    },
}

impl RuntimeImpact {
    pub(crate) fn save_and_start_mode(&self) -> ApplyMode {
        if matches!(self, Self::RestartRequired { .. }) {
            ApplyMode::SaveAndRestart
        } else {
            ApplyMode::SaveAndStart
        }
    }

    fn running(&self) -> bool {
        matches!(self, Self::Matching { .. } | Self::RestartRequired { .. })
    }

    fn restart_required(&self) -> bool {
        matches!(self, Self::RestartRequired { .. })
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum LifecycleAction {
    SaveOnly,
    On,
    Restart,
}

fn lifecycle_action(mode: ApplyMode, impact: &RuntimeImpact) -> Result<LifecycleAction, Error> {
    if mode == ApplyMode::SaveOnly {
        return Ok(LifecycleAction::SaveOnly);
    }
    if matches!(impact, RuntimeImpact::Unverified { .. }) {
        return Err(Error::message(
            "worker fingerprint could not be authenticated; setup did not write or restart",
        ));
    }
    if impact.restart_required() {
        return if mode == ApplyMode::SaveAndRestart {
            Ok(LifecycleAction::Restart)
        } else {
            Err(Error::message(
                "the running worker requires a restart, but its restart impact was not previewed; setup did not write or restart",
            ))
        };
    }
    Ok(LifecycleAction::On)
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ApplyResult {
    pub mode: ApplyMode,
    pub runtime_state: &'static str,
    pub worker_processes_started: u8,
    pub authenticated_readiness: bool,
    pub config_path: PathBuf,
    pub pending_path: PathBuf,
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(super) struct PendingSetup {
    pub(super) version: u8,
    pub(super) shared_with_other_pcs: bool,
    pub(super) separate_input_output: bool,
    pub(super) login_requested: bool,
    pub(super) clients: Vec<ClientIntent>,
}

pub(super) fn pending_path(config_path: &Path) -> Result<PathBuf, Error> {
    let file_name = config_path
        .file_name()
        .ok_or_else(|| Error::message("config path has no file name"))?;
    let mut name = OsString::from(".");
    name.push(file_name);
    name.push(".setup-pending.json");
    Ok(config_path.with_file_name(name))
}

pub fn runtime_impact(path: &Path, draft: &SetupDraft) -> Result<RuntimeImpact, Error> {
    let path = absolute(path)?;
    let bytes = draft.render_config()?;
    inspect_runtime(&path, bytes.as_bytes())
}

fn inspect_runtime(path: &Path, desired: &[u8]) -> Result<RuntimeImpact, Error> {
    let desired_fingerprint = crate::config::ConfigFingerprint::from_bytes(desired)
        .as_str()
        .to_owned();
    if !path.try_exists()? {
        return Ok(RuntimeImpact::NewConfig {
            desired_fingerprint,
        });
    }
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()?;
    let Ok(status) = runtime.block_on(crate::lifecycle::status(path)) else {
        return Ok(RuntimeImpact::Unverified {
            desired_fingerprint,
        });
    };
    match status["state"].as_str() {
        Some("stopped") => Ok(RuntimeImpact::Stopped {
            desired_fingerprint,
        }),
        Some("running") => {
            let Some(worker_fingerprint) = status["identity"]["fingerprint"].as_str() else {
                return Ok(RuntimeImpact::Unverified {
                    desired_fingerprint,
                });
            };
            if worker_fingerprint == desired_fingerprint {
                Ok(RuntimeImpact::Matching {
                    fingerprint: desired_fingerprint,
                })
            } else {
                Ok(RuntimeImpact::RestartRequired {
                    desired_fingerprint,
                    worker_fingerprint: worker_fingerprint.to_owned(),
                })
            }
        }
        _ => Ok(RuntimeImpact::Unverified {
            desired_fingerprint,
        }),
    }
}

fn render_pending(draft: &SetupDraft) -> Result<Vec<u8>, Error> {
    serde_json::to_vec_pretty(&PendingSetup {
        version: 1,
        shared_with_other_pcs: draft.shared_with_other_pcs,
        separate_input_output: draft.separate_input_output,
        login_requested: draft.login_requested,
        clients: draft.clients.clone(),
    })
    .map_err(|_| Error::message("setup pending metadata could not be serialized"))
}

pub fn apply(path: &Path, draft: &SetupDraft, mode: ApplyMode) -> Result<ApplyResult, Error> {
    let path = absolute(path)?;
    let bytes = draft.render_config()?;
    let pending_path = pending_path(&path)?;
    let pending = render_pending(draft)?;
    ensure_parent(&path)?;
    let _lock = setup_lock(&path)?;
    verify_snapshot(
        &path,
        draft.original.as_ref().map(|(_, bytes)| bytes.as_slice()),
        "config",
    )?;
    if let Some(expected) = &draft.original_pending {
        verify_snapshot(&pending_path, expected.as_deref(), "setup pending metadata")?;
    }
    let runtime_impact = inspect_runtime(&path, bytes.as_bytes())?;
    let was_running = runtime_impact.running();
    let action = lifecycle_action(mode, &runtime_impact)?;
    write_protected_file(
        &path,
        bytes.as_bytes(),
        "config",
        draft.original.as_ref().map(|(_, bytes)| bytes.as_slice()),
    )?;
    write_protected_file(
        &pending_path,
        &pending,
        "setup pending metadata",
        draft
            .original_pending
            .as_ref()
            .and_then(|bytes| bytes.as_deref()),
    )?;
    let desired_fingerprint = crate::config::ConfigFingerprint::from_bytes(bytes.as_bytes());
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .map_err(|error| {
            Error::message(format!(
                "configuration and setup pending metadata were applied, but protected state initialization failed; no worker or client file was changed: {error}"
            ))
        })?;
    runtime
        .block_on(crate::lifecycle::initialize_setup_state(
            &path,
            desired_fingerprint.as_str(),
        ))
        .map_err(|error| {
            Error::message(format!(
                "configuration and setup pending metadata were applied, but protected state initialization failed; no worker or client file was changed: {error}"
            ))
        })?;
    if mode == ApplyMode::SaveOnly {
        return Ok(ApplyResult {
            mode,
            runtime_state: "not_started",
            worker_processes_started: 0,
            authenticated_readiness: false,
            config_path: path,
            pending_path,
        });
    }
    let status = match action {
        LifecycleAction::Restart => runtime.block_on(crate::lifecycle::restart_expected(
            &path,
            desired_fingerprint.as_str(),
        ))?,
        LifecycleAction::On => runtime.block_on(crate::lifecycle::on_expected(
            &path,
            desired_fingerprint.as_str(),
        ))?,
        LifecycleAction::SaveOnly => unreachable!("save-only returned before lifecycle action"),
    };
    if status["state"] != "running"
        || status["identity"]["fingerprint"].as_str() != Some(desired_fingerprint.as_str())
    {
        return Err(Error::message(
            "authenticated lifecycle readiness for the intended configuration was not established",
        ));
    }
    Ok(ApplyResult {
        mode,
        runtime_state: "running",
        worker_processes_started: u8::from(!was_running || action == LifecycleAction::Restart),
        authenticated_readiness: true,
        config_path: path,
        pending_path,
    })
}

pub fn absolute(path: &Path) -> Result<PathBuf, Error> {
    Ok(std::path::absolute(path)?)
}

fn ensure_parent(path: &Path) -> Result<(), Error> {
    let parent = path
        .parent()
        .ok_or_else(|| Error::message("config path has no parent"))?;
    if !parent.exists() {
        #[cfg(unix)]
        {
            use std::os::unix::fs::DirBuilderExt;
            fs::DirBuilder::new()
                .recursive(true)
                .mode(0o700)
                .create(parent)?;
        }
        #[cfg(not(unix))]
        fs::create_dir_all(parent)?;
    }
    Ok(())
}

fn setup_lock(path: &Path) -> Result<fs::File, Error> {
    let file_name = path
        .file_name()
        .ok_or_else(|| Error::message("config path has no file name"))?;
    let mut name = OsString::from(".");
    name.push(file_name);
    name.push(".setup.lock");
    let lock_path = path.with_file_name(name);
    let file = crate::lifecycle::platform::open(&lock_path, true, false)?;
    match FileExt::try_lock(&file) {
        Ok(()) => Ok(file),
        Err(fs4::TryLockError::WouldBlock) => Err(Error::message(
            "another setup apply is in progress; retry after it finishes",
        )),
        Err(fs4::TryLockError::Error(error)) => Err(error.into()),
    }
}

fn read_regular(path: &Path, kind: &str) -> Result<Option<(fs::Metadata, Vec<u8>)>, Error> {
    let metadata = match fs::symlink_metadata(path) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(error.into()),
    };
    if !metadata.is_file() || metadata.file_type().is_symlink() {
        return Err(Error::message(format!(
            "existing {kind} must be a regular file, not a link"
        )));
    }
    #[cfg(unix)]
    let mut file = {
        use std::os::unix::fs::{MetadataExt, OpenOptionsExt};
        let file = OpenOptions::new()
            .read(true)
            .custom_flags(libc::O_NOFOLLOW)
            .open(path)?;
        let opened = file.metadata()?;
        if opened.dev() != metadata.dev() || opened.ino() != metadata.ino() {
            return Err(Error::message(format!(
                "existing {kind} changed while setup was opening it"
            )));
        }
        file
    };
    #[cfg(not(unix))]
    let mut file = OpenOptions::new().read(true).open(path)?;
    let mut bytes = Vec::new();
    file.read_to_end(&mut bytes)?;
    Ok(Some((metadata, bytes)))
}

fn verify_snapshot(path: &Path, expected: Option<&[u8]>, kind: &str) -> Result<(), Error> {
    match (read_regular(path, kind)?, expected) {
        (None, None) => Ok(()),
        (Some((_, current)), Some(expected)) if current == expected => Ok(()),
        _ => Err(Error::message(format!(
            "{kind} changed since setup loaded it; rerun setup to review the latest file"
        ))),
    }
}

fn write_protected_file(
    path: &Path,
    bytes: &[u8],
    kind: &str,
    expected: Option<&[u8]>,
) -> Result<(), Error> {
    ensure_parent(path)?;
    let parent = path
        .parent()
        .ok_or_else(|| Error::message("config path has no parent"))?;
    let existing = read_regular(path, kind)?;
    match (&existing, expected) {
        (None, None) => {}
        (Some((_, current)), Some(expected)) if current == expected => {}
        _ => {
            return Err(Error::message(format!(
                "{kind} changed since setup loaded it; rerun setup to review the latest file"
            )));
        }
    }
    if existing
        .as_ref()
        .is_some_and(|(_, current)| current == bytes)
    {
        return Ok(());
    }
    #[cfg(unix)]
    if let Some((metadata, _)) = &existing {
        use std::os::unix::fs::MetadataExt;
        if metadata.uid() != rustix::process::geteuid().as_raw() {
            return Err(Error::message(format!(
                "existing {kind} is not owned by the current user; refusing to replace it"
            )));
        }
    }
    let temporary = parent.join(format!(
        ".llmgw-setup-{}-{:016x}.tmp",
        std::process::id(),
        rand::random::<u64>()
    ));
    let result = (|| -> std::io::Result<()> {
        let mut file = crate::lifecycle::platform::open(&temporary, true, true)?;
        #[cfg(target_os = "linux")]
        refuse_linux_posix_acl(&temporary, kind)?;
        file.write_all(bytes)?;
        file.sync_all()?;
        drop(file);
        if let Some((metadata, _)) = &existing {
            #[cfg(unix)]
            fs::set_permissions(&temporary, metadata.permissions())?;
            #[cfg(unix)]
            {
                use std::os::unix::fs::MetadataExt;
                let temporary_metadata = fs::metadata(&temporary)?;
                if temporary_metadata.uid() != metadata.uid()
                    || temporary_metadata.gid() != metadata.gid()
                {
                    return Err(std::io::Error::other(format!(
                        "existing {kind} ownership could not be preserved"
                    )));
                }
            }
            #[cfg(target_os = "linux")]
            {
                refuse_linux_posix_acl(path, kind)?;
                refuse_linux_posix_acl(&temporary, kind)?;
            }
            #[cfg(all(unix, not(any(target_os = "macos", target_os = "linux"))))]
            return Err(std::io::Error::other(format!(
                "existing {kind} ACL preservation is unsupported on this operating system"
            )));
            #[cfg(target_os = "macos")]
            {
                use std::os::unix::fs::MetadataExt;
                let acl = read_acl(path)?;
                let current = fs::symlink_metadata(path)?;
                if current.dev() != metadata.dev() || current.ino() != metadata.ino() {
                    return Err(std::io::Error::other(format!(
                        "existing {kind} changed while setup copied its ACL"
                    )));
                }
                exacl::setfacl(&[&temporary], &acl, None)?;
                if read_acl(&temporary)? != acl
                    || fs::metadata(&temporary)?.permissions() != metadata.permissions()
                {
                    return Err(std::io::Error::other(format!(
                        "existing {kind} permissions or ACL could not be preserved"
                    )));
                }
            }
            fs::File::open(&temporary)?.sync_all()?;
            let Some((current_metadata, current)) = read_regular(path, kind)
                .map_err(|error| std::io::Error::other(error.to_string()))?
            else {
                return Err(std::io::Error::other(format!(
                    "existing {kind} changed before setup could replace it"
                )));
            };
            #[cfg(unix)]
            {
                use std::os::unix::fs::MetadataExt;
                if current_metadata.dev() != metadata.dev()
                    || current_metadata.ino() != metadata.ino()
                    || current_metadata.uid() != metadata.uid()
                    || current_metadata.gid() != metadata.gid()
                    || current_metadata.permissions() != metadata.permissions()
                    || current.as_slice() != expected.expect("existing file has snapshot")
                {
                    return Err(std::io::Error::other(format!(
                        "existing {kind} changed before setup could replace it"
                    )));
                }
            }
            #[cfg(target_os = "linux")]
            refuse_linux_posix_acl(path, kind)?;
            #[cfg(target_os = "macos")]
            if read_acl(path)? != read_acl(&temporary)? {
                return Err(std::io::Error::other(format!(
                    "existing {kind} ACL changed before setup could replace it"
                )));
            }
            #[cfg(not(unix))]
            if current.as_slice() != expected.expect("existing file has snapshot") {
                return Err(std::io::Error::other(format!(
                    "existing {kind} changed before setup could replace it"
                )));
            }
            #[cfg(unix)]
            fs::rename(&temporary, path)?;
            #[cfg(windows)]
            replace_existing_with_recovery(
                &temporary,
                path,
                expected.expect("existing file has snapshot"),
                bytes,
                kind,
                crate::lifecycle::platform::replace_file,
            )?;
        } else {
            fs::hard_link(&temporary, path)?;
            fs::remove_file(&temporary)?;
        }
        #[cfg(unix)]
        fs::File::open(parent)?.sync_all()?;
        Ok(())
    })();
    if result.is_err() {
        let _ = fs::remove_file(&temporary);
    }
    result.map_err(Error::from)
}

#[cfg(target_os = "macos")]
fn read_acl(path: &Path) -> std::io::Result<Vec<exacl::AclEntry>> {
    exacl::getfacl(path, exacl::AclOption::SYMLINK_ACL)
}

#[cfg(target_os = "linux")]
fn refuse_linux_posix_acl(path: &Path, kind: &str) -> std::io::Result<()> {
    match rustix::fs::lgetxattr(path, "system.posix_acl_access", Vec::with_capacity(65_536)) {
        Ok(_) => Err(std::io::Error::new(
            std::io::ErrorKind::Unsupported,
            format!("existing {kind} has a POSIX ACL that setup cannot preserve; edit it manually"),
        )),
        Err(rustix::io::Errno::NODATA | rustix::io::Errno::NOTSUP) => Ok(()),
        Err(error) => Err(error.into()),
    }
}

#[cfg(test)]
mod tests {
    use super::{
        ApplyMode, LifecycleAction, RuntimeImpact, lifecycle_action, replace_existing_with_recovery,
    };
    use std::{cell::Cell, fs, io::Write, path::Path};

    fn protected_write(path: &Path, bytes: &[u8]) {
        let mut file = crate::lifecycle::platform::open(path, true, true).unwrap();
        file.write_all(bytes).unwrap();
        file.sync_all().unwrap();
    }

    #[test]
    fn pending_worker_requires_the_explicit_restart_mode() {
        let impact = RuntimeImpact::RestartRequired {
            desired_fingerprint: "desired".into(),
            worker_fingerprint: "worker".into(),
        };
        assert_eq!(
            lifecycle_action(ApplyMode::SaveAndRestart, &impact).unwrap(),
            LifecycleAction::Restart
        );
        let error = lifecycle_action(ApplyMode::SaveAndStart, &impact)
            .unwrap_err()
            .to_string();
        assert!(error.contains("restart impact was not previewed"));
    }

    #[test]
    fn matching_worker_uses_on_and_unverified_worker_refuses_start() {
        let matching = RuntimeImpact::Matching {
            fingerprint: "same".into(),
        };
        assert_eq!(
            lifecycle_action(ApplyMode::SaveAndStart, &matching).unwrap(),
            LifecycleAction::On
        );
        let unverified = RuntimeImpact::Unverified {
            desired_fingerprint: "desired".into(),
        };
        assert!(lifecycle_action(ApplyMode::SaveAndRestart, &unverified).is_err());
        assert_eq!(
            lifecycle_action(ApplyMode::SaveOnly, &unverified).unwrap(),
            LifecycleAction::SaveOnly
        );
    }

    #[test]
    fn ambiguous_windows_replace_keeps_original_and_candidate_recovery() {
        let parent = std::env::temp_dir().join(format!(
            "llmgw-windows-recovery-{}-{:016x}",
            std::process::id(),
            rand::random::<u64>()
        ));
        fs::create_dir(&parent).unwrap();
        let destination = parent.join("config.toml");
        let replacement = parent.join("replacement.tmp");
        protected_write(&destination, b"original");
        protected_write(&replacement, b"replacement");

        let error = replace_existing_with_recovery(
            &replacement,
            &destination,
            b"original",
            b"replacement",
            "config",
            |_, destination| {
                fs::remove_file(destination)?;
                Err(std::io::Error::from_raw_os_error(1176))
            },
        )
        .unwrap_err();
        let entries = fs::read_dir(&parent)
            .unwrap()
            .map(|entry| entry.unwrap().path())
            .collect::<Vec<_>>();
        let recovery = entries
            .iter()
            .find(|path| path.to_string_lossy().ends_with(".recovery"))
            .unwrap();
        let candidate = entries
            .iter()
            .find(|path| path.to_string_lossy().ends_with(".candidate"))
            .unwrap();
        assert_eq!(fs::read(recovery).unwrap(), b"original");
        assert_eq!(fs::read(candidate).unwrap(), b"replacement");
        assert!(error.to_string().contains(&recovery.display().to_string()));
        assert!(error.to_string().contains(&candidate.display().to_string()));
        fs::remove_dir_all(parent).unwrap();
    }

    #[test]
    fn windows_recovery_refuses_stale_source_before_replace() {
        let parent = std::env::temp_dir().join(format!(
            "llmgw-windows-preflight-{}-{:016x}",
            std::process::id(),
            rand::random::<u64>()
        ));
        fs::create_dir(&parent).unwrap();
        let destination = parent.join("config.toml");
        let replacement = parent.join("replacement.tmp");
        protected_write(&destination, b"external edit");
        protected_write(&replacement, b"replacement");
        let called = Cell::new(false);

        let result = replace_existing_with_recovery(
            &replacement,
            &destination,
            b"stale original",
            b"replacement",
            "config",
            |_, _| {
                called.set(true);
                Ok(())
            },
        );
        assert!(result.is_err());
        assert!(!called.get());
        assert_eq!(fs::read(&destination).unwrap(), b"external edit");
        assert_eq!(fs::read_dir(&parent).unwrap().count(), 2);
        fs::remove_dir_all(parent).unwrap();
    }

    #[test]
    fn verified_windows_replace_removes_recovery_copies() {
        let parent = std::env::temp_dir().join(format!(
            "llmgw-windows-success-{}-{:016x}",
            std::process::id(),
            rand::random::<u64>()
        ));
        fs::create_dir(&parent).unwrap();
        let destination = parent.join("config.toml");
        let replacement = parent.join("replacement.tmp");
        protected_write(&destination, b"original");
        protected_write(&replacement, b"replacement");

        replace_existing_with_recovery(
            &replacement,
            &destination,
            b"original",
            b"replacement",
            "config",
            |from, to| fs::rename(from, to),
        )
        .unwrap();
        assert_eq!(fs::read(&destination).unwrap(), b"replacement");
        assert_eq!(fs::read_dir(&parent).unwrap().count(), 1);
        fs::remove_dir_all(parent).unwrap();
    }
}
