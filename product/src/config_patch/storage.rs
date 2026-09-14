use super::Error;
use fs4::FileExt;
use std::{
    ffi::OsString,
    fs::{self, OpenOptions},
    io::{Read, Write},
    path::{Path, PathBuf},
};

pub(crate) struct Snapshot {
    pub metadata: fs::Metadata,
    pub bytes: Vec<u8>,
}

pub(crate) fn read_regular(path: &Path, kind: &str) -> Result<Option<Snapshot>, Error> {
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
            .custom_flags(libc::O_NOFOLLOW | libc::O_CLOEXEC)
            .open(path)?;
        let opened = file.metadata()?;
        if opened.dev() != metadata.dev() || opened.ino() != metadata.ino() {
            return Err(Error::message(format!(
                "existing {kind} changed while it was opened"
            )));
        }
        file
    };
    #[cfg(windows)]
    let mut file = {
        use std::os::windows::fs::{MetadataExt, OpenOptionsExt};
        use windows_sys::Win32::Storage::FileSystem::{
            FILE_ATTRIBUTE_REPARSE_POINT, FILE_FLAG_BACKUP_SEMANTICS, FILE_FLAG_OPEN_REPARSE_POINT,
        };
        if metadata.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT != 0 {
            return Err(Error::message(format!(
                "existing {kind} reparse points are unsupported"
            )));
        }
        let file = OpenOptions::new()
            .read(true)
            .share_mode(0x1 | 0x2 | 0x4)
            .custom_flags(FILE_FLAG_OPEN_REPARSE_POINT | FILE_FLAG_BACKUP_SEMANTICS)
            .open(path)?;
        let opened = file.metadata()?;
        if opened.volume_serial_number() != metadata.volume_serial_number()
            || opened.file_index() != metadata.file_index()
        {
            return Err(Error::message(format!(
                "existing {kind} changed while it was opened"
            )));
        }
        file
    };
    let mut bytes = Vec::new();
    file.read_to_end(&mut bytes)?;
    Ok(Some(Snapshot { metadata, bytes }))
}

pub(crate) fn validate_local_token_target(path: &Path, exists: bool) -> Result<(), Error> {
    if !exists {
        return Ok(());
    }
    let snapshot = read_regular(path, "token-bearing client config")?
        .ok_or_else(|| Error::message("token-bearing client config disappeared"))?;
    validate_local_token_snapshot(path, Some(&snapshot))
}

pub(crate) fn validate_local_token_snapshot(
    path: &Path,
    snapshot: Option<&Snapshot>,
) -> Result<(), Error> {
    let Some(snapshot) = snapshot else {
        return Ok(());
    };
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        if snapshot.metadata.uid() != rustix::process::geteuid().as_raw()
            || snapshot.metadata.mode() & 0o077 != 0
        {
            return Err(Error::message(
                "client config is readable by other users; correct the permission or ACL manually, then retry; llmgw did not chmod the file",
            ));
        }
        #[cfg(target_os = "macos")]
        if !exacl::getfacl(path, exacl::AclOption::SYMLINK_ACL)
            .map_err(|_| Error::message("client config ACL could not be validated"))?
            .is_empty()
        {
            return Err(Error::message(
                "client config has an extended ACL that may grant other readers; correct the permission or ACL manually, then retry; llmgw did not alter it",
            ));
        }
        let current = read_regular(path, "token-bearing client config")?
            .ok_or_else(|| Error::message("token-bearing client config disappeared"))?;
        if !same_snapshot(&current, snapshot) {
            return Err(Error::message(
                "client config permission, ACL, identity, or bytes changed during token privacy validation; create a new preview",
            ));
        }
    }
    #[cfg(windows)]
    {
        let _ = snapshot;
        crate::lifecycle::platform::read(path).map_err(|_| {
            Error::message(
                "client config DACL is not a supported current-user-only form; correct the permission manually, then retry; llmgw did not alter it",
            )
        })?;
    }
    Ok(())
}

/// Stable adjacent lock identity used by every ConfigPatch journal that edits
/// the same physical client path. It does not depend on process temp or state
/// directories and the lock file remains in place when a lock is released.
pub fn cooperating_lock_path(resource: &Path) -> Result<PathBuf, Error> {
    let normalized = normalize_resource_path(resource)?;
    cooperating_lock_path_normalized(&normalized)
}

pub(crate) fn normalize_resource_path(resource: &Path) -> Result<PathBuf, Error> {
    let absolute = if resource.is_absolute() {
        resource.to_path_buf()
    } else {
        std::env::current_dir()?.join(resource)
    };
    if absolute.file_name().is_none() {
        return Err(Error::message("client resource path must name a file"));
    }
    match fs::symlink_metadata(&absolute) {
        Ok(metadata) => {
            if metadata.file_type().is_symlink() {
                return Err(Error::message(
                    "client resource path must name a regular file, not a link",
                ));
            }
            #[cfg(windows)]
            {
                use std::os::windows::fs::MetadataExt;
                use windows_sys::Win32::Storage::FileSystem::FILE_ATTRIBUTE_REPARSE_POINT;
                if metadata.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT != 0 {
                    return Err(Error::message(
                        "client resource reparse points are unsupported",
                    ));
                }
            }
            Ok(fs::canonicalize(absolute)?)
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            normalize_missing_resource(&absolute)
        }
        Err(error) => Err(error.into()),
    }
}

fn normalize_missing_resource(resource: &Path) -> Result<PathBuf, Error> {
    let mut cursor = resource.to_path_buf();
    let mut missing = Vec::new();
    loop {
        match fs::symlink_metadata(&cursor) {
            Ok(_) => {
                let mut normalized = fs::canonicalize(&cursor)?;
                for component in missing.iter().rev() {
                    normalized.push(component);
                }
                return Ok(normalized);
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                let name = cursor.file_name().ok_or_else(|| {
                    Error::message(
                        "a missing client resource path containing unresolved parent aliases is unsupported",
                    )
                })?;
                missing.push(name.to_os_string());
                cursor = cursor
                    .parent()
                    .ok_or_else(|| Error::message("client resource path has no existing ancestor"))?
                    .to_path_buf();
            }
            Err(error) => return Err(error.into()),
        }
    }
}

fn cooperating_lock_path_normalized(resource: &Path) -> Result<PathBuf, Error> {
    let parent = resource
        .parent()
        .ok_or_else(|| Error::message("client resource path has no parent directory"))?;
    let mut name = resource
        .file_name()
        .ok_or_else(|| Error::message("client resource path must name a file"))?
        .to_os_string();
    name.push(".llmgw-config-patch.lock");
    Ok(parent.join(name))
}

pub(crate) fn journal_lock_path(journal_path: &Path) -> PathBuf {
    let file = journal_path
        .file_name()
        .map_or_else(|| OsString::from("journal"), OsString::from);
    let mut name = file;
    name.push(".lock");
    journal_path.with_file_name(name)
}

pub(crate) fn lock_path(path: &Path, kind: &str) -> Result<fs::File, Error> {
    let parent = path
        .parent()
        .ok_or_else(|| Error::message("config patch lock path has no parent"))?;
    crate::lifecycle::platform::directory(parent)?;
    let file = crate::lifecycle::platform::open(path, true, false)?;
    try_lock(file, kind)
}

pub(crate) fn lock_resource(resource: &Path) -> Result<(fs::File, PathBuf), Error> {
    let initial = normalize_resource_path(resource)?;
    let parent = initial
        .parent()
        .ok_or_else(|| Error::message("client resource path has no parent directory"))?;
    ensure_client_parent(parent)?;
    let normalized = normalize_resource_path(&initial)?;
    let lock_path = cooperating_lock_path_normalized(&normalized)?;
    let file = crate::lifecycle::platform::open(&lock_path, true, false)?;
    Ok((try_lock(file, "client resource")?, normalized))
}

fn try_lock(file: fs::File, kind: &str) -> Result<fs::File, Error> {
    match FileExt::try_lock(&file) {
        Ok(()) => Ok(file),
        Err(fs4::TryLockError::WouldBlock) => Err(Error::message(format!(
            "another llmgw {kind} edit is in progress; retry after it finishes"
        ))),
        Err(fs4::TryLockError::Error(error)) => Err(error.into()),
    }
}

pub(crate) fn atomic_write_client(
    path: &Path,
    bytes: &[u8],
    expected: Option<&Snapshot>,
    kind: &str,
    require_user_private: bool,
) -> Result<(), Error> {
    let parent = path
        .parent()
        .ok_or_else(|| Error::message("client config path has no parent directory"))?;
    ensure_client_parent(parent)?;
    let temporary = temporary_path(path, "candidate");
    let result = (|| -> Result<(), Error> {
        let mut file = crate::lifecycle::platform::open(&temporary, true, true)?;
        file.write_all(bytes)?;
        file.sync_all()?;
        drop(file);
        if let Some(expected) = expected {
            prepare_existing_replacement(path, &temporary, expected, kind)?;
            if require_user_private {
                // Perform the access check after candidate metadata has been
                // prepared, immediately before the final identity/byte check.
                // An external editor can still race the final check and rename;
                // ConfigPatch does not claim filesystem CAS against it.
                validate_local_token_snapshot(path, Some(expected))?;
                let candidate = read_regular(&temporary, "token-bearing client candidate")?
                    .ok_or_else(|| Error::message("token-bearing client candidate disappeared"))?;
                validate_local_token_snapshot(&temporary, Some(&candidate))?;
            }
            let current = read_regular(path, kind)?
                .ok_or_else(|| Error::message(format!("existing {kind} disappeared")))?;
            if !same_snapshot(&current, expected) {
                return Err(Error::message(format!(
                    "existing {kind} changed before replacement"
                )));
            }
            validate_replacement_metadata(path, &temporary, kind)?;
            replace_existing_with_recovery(&temporary, path, &expected.bytes, bytes, kind)?;
        } else {
            fs::hard_link(&temporary, path).map_err(|error| {
                if error.kind() == std::io::ErrorKind::AlreadyExists {
                    Error::message(format!("new {kind} appeared before publication"))
                } else {
                    error.into()
                }
            })?;
            fs::remove_file(&temporary)?;
        }
        #[cfg(unix)]
        fs::File::open(parent)?.sync_all()?;
        Ok(())
    })();
    if result.is_err() {
        let _ = fs::remove_file(&temporary);
    }
    result
}

fn ensure_client_parent(parent: &Path) -> Result<(), Error> {
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
    let metadata = fs::symlink_metadata(parent)?;
    if !metadata.is_dir() || metadata.file_type().is_symlink() {
        return Err(Error::message(
            "client resource parent must be a directory, not a link",
        ));
    }
    Ok(())
}

fn temporary_path(path: &Path, label: &str) -> PathBuf {
    let file = path
        .file_name()
        .map_or_else(|| OsString::from("config"), OsString::from);
    let mut name = OsString::from(".llmgw-");
    name.push(file);
    name.push(format!(
        "-{label}-{}-{:016x}.tmp",
        std::process::id(),
        rand::random::<u64>()
    ));
    path.with_file_name(name)
}

fn same_snapshot(current: &Snapshot, expected: &Snapshot) -> bool {
    if current.bytes != expected.bytes {
        return false;
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        current.metadata.dev() == expected.metadata.dev()
            && current.metadata.ino() == expected.metadata.ino()
            && current.metadata.uid() == expected.metadata.uid()
            && current.metadata.gid() == expected.metadata.gid()
            && current.metadata.permissions() == expected.metadata.permissions()
    }
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        current.metadata.volume_serial_number() == expected.metadata.volume_serial_number()
            && current.metadata.file_index() == expected.metadata.file_index()
            && current.metadata.file_attributes() == expected.metadata.file_attributes()
    }
}

#[cfg(unix)]
fn prepare_existing_replacement(
    path: &Path,
    temporary: &Path,
    expected: &Snapshot,
    kind: &str,
) -> Result<(), Error> {
    use std::os::unix::fs::MetadataExt;
    if expected.metadata.uid() != rustix::process::geteuid().as_raw() {
        return Err(Error::message(format!(
            "existing {kind} is not owned by the current user"
        )));
    }
    fs::set_permissions(temporary, expected.metadata.permissions())?;
    let temp_metadata = fs::metadata(temporary)?;
    if temp_metadata.uid() != expected.metadata.uid()
        || temp_metadata.gid() != expected.metadata.gid()
    {
        return Err(Error::message(format!(
            "existing {kind} ownership could not be preserved"
        )));
    }
    #[cfg(target_os = "linux")]
    {
        refuse_linux_posix_acl(path, kind)?;
        refuse_linux_posix_acl(temporary, kind)?;
    }
    #[cfg(target_os = "macos")]
    {
        let acl = exacl::getfacl(path, exacl::AclOption::SYMLINK_ACL)?;
        let current = fs::symlink_metadata(path)?;
        if current.dev() != expected.metadata.dev() || current.ino() != expected.metadata.ino() {
            return Err(Error::message(format!(
                "existing {kind} changed while its ACL was read"
            )));
        }
        exacl::setfacl(&[temporary], &acl, None)?;
        if exacl::getfacl(temporary, exacl::AclOption::SYMLINK_ACL)? != acl
            || fs::metadata(temporary)?.permissions() != expected.metadata.permissions()
        {
            return Err(Error::message(format!(
                "existing {kind} permissions or ACL could not be preserved"
            )));
        }
    }
    #[cfg(not(any(target_os = "macos", target_os = "linux")))]
    return Err(Error::message(format!(
        "existing {kind} ACL preservation is unsupported on this Unix platform"
    )));
    fs::File::open(temporary)?.sync_all()?;
    Ok(())
}

#[cfg(windows)]
fn prepare_existing_replacement(
    _path: &Path,
    _temporary: &Path,
    _expected: &Snapshot,
    _kind: &str,
) -> Result<(), Error> {
    // ReplaceFileW merges the destination's attributes and DACL. Independent
    // private recovery copies are created by the shared replacement helper.
    Ok(())
}

fn replace_existing_with_recovery(
    replacement: &Path,
    destination: &Path,
    expected_original: &[u8],
    expected_replacement: &[u8],
    kind: &str,
) -> Result<(), Error> {
    #[cfg(unix)]
    {
        let _ = (expected_original, expected_replacement, kind);
        fs::rename(replacement, destination)?;
        Ok(())
    }
    #[cfg(windows)]
    crate::file_replace::replace_existing_with_recovery(
        replacement,
        destination,
        expected_original,
        expected_replacement,
        kind,
        crate::lifecycle::platform::replace_file,
    )
    .map_err(Into::into)
}

#[cfg(unix)]
fn validate_replacement_metadata(path: &Path, temporary: &Path, kind: &str) -> Result<(), Error> {
    #[cfg(target_os = "linux")]
    {
        refuse_linux_posix_acl(path, kind)?;
        refuse_linux_posix_acl(temporary, kind)?;
    }
    #[cfg(target_os = "macos")]
    if exacl::getfacl(path, exacl::AclOption::SYMLINK_ACL)?
        != exacl::getfacl(temporary, exacl::AclOption::SYMLINK_ACL)?
    {
        return Err(Error::message(format!(
            "existing {kind} ACL changed before replacement"
        )));
    }
    Ok(())
}

#[cfg(windows)]
fn validate_replacement_metadata(
    _path: &Path,
    _temporary: &Path,
    _kind: &str,
) -> Result<(), Error> {
    Ok(())
}

pub(crate) fn write_private_state_atomic(path: &Path, bytes: &[u8]) -> Result<(), Error> {
    let existing = match crate::config_patch::journal::read_private(path) {
        Ok(bytes) => Some(bytes),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => None,
        Err(error) => return Err(error.into()),
    };
    let expected = match existing {
        Some(bytes) => Some(Snapshot {
            metadata: fs::symlink_metadata(path)?,
            bytes,
        }),
        None => None,
    };
    atomic_write_client(
        path,
        bytes,
        expected.as_ref(),
        "protected patch journal",
        true,
    )
}

#[cfg(target_os = "linux")]
fn refuse_linux_posix_acl(path: &Path, kind: &str) -> Result<(), Error> {
    match rustix::fs::lgetxattr(path, "system.posix_acl_access", Vec::with_capacity(65_536)) {
        Ok(_) => Err(Error::message(format!(
            "existing {kind} has a POSIX ACL that cannot be preserved safely"
        ))),
        Err(rustix::io::Errno::NODATA | rustix::io::Errno::NOTSUP) => Ok(()),
        Err(error) => Err(std::io::Error::from(error).into()),
    }
}
