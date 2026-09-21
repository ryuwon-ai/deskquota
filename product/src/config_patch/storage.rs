use super::Error;
use crate::lifecycle::Lock;
use std::{
    ffi::OsString,
    fs::{self, OpenOptions},
    io::{Read, Write},
    path::{Path, PathBuf},
};

pub(crate) struct Snapshot {
    pub metadata: fs::Metadata,
    pub bytes: Vec<u8>,
    #[cfg(windows)]
    identity: crate::lifecycle::platform::FileIdentity,
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
    let file = {
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
    let file = {
        use std::os::windows::fs::{MetadataExt, OpenOptionsExt};
        use windows_sys::Win32::Storage::FileSystem::{
            FILE_ATTRIBUTE_REPARSE_POINT, FILE_FLAG_BACKUP_SEMANTICS, FILE_FLAG_OPEN_REPARSE_POINT,
        };
        if metadata.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT != 0 {
            return Err(Error::message(format!(
                "existing {kind} reparse points are unsupported"
            )));
        }
        OpenOptions::new()
            .read(true)
            .share_mode(0x1 | 0x2 | 0x4)
            .custom_flags(FILE_FLAG_OPEN_REPARSE_POINT | FILE_FLAG_BACKUP_SEMANTICS)
            .open(path)?
    };
    read_snapshot(file, kind).map(Some)
}

fn read_snapshot(mut file: fs::File, kind: &str) -> Result<Snapshot, Error> {
    let metadata = file.metadata()?;
    if !metadata.is_file() || metadata.file_type().is_symlink() {
        return Err(Error::message(format!(
            "opened {kind} must be a regular file, not a link"
        )));
    }
    #[cfg(windows)]
    let identity = {
        use std::os::windows::fs::MetadataExt;
        use windows_sys::Win32::Storage::FileSystem::FILE_ATTRIBUTE_REPARSE_POINT;
        if metadata.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT != 0 {
            return Err(Error::message(format!(
                "opened {kind} reparse points are unsupported"
            )));
        }
        crate::lifecycle::platform::file_identity(&file)?
    };
    let mut bytes = Vec::new();
    file.read_to_end(&mut bytes)?;
    Ok(Snapshot {
        metadata,
        bytes,
        #[cfg(windows)]
        identity,
    })
}

pub(crate) fn validate_private_target(path: &Path, exists: bool) -> Result<(), Error> {
    if !exists {
        return Ok(());
    }
    let snapshot = read_regular(path, "private client config")?
        .ok_or_else(|| Error::message("private client config disappeared"))?;
    validate_private_snapshot(path, Some(&snapshot))
}

pub(crate) fn validate_private_snapshot(
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
        let current = read_regular(path, "private client config")?
            .ok_or_else(|| Error::message("private client config disappeared"))?;
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

pub(crate) fn lock_path(path: &Path, kind: &str) -> Result<Lock, Error> {
    let parent = path
        .parent()
        .ok_or_else(|| Error::message("config patch lock path has no parent"))?;
    crate::lifecycle::platform::directory(parent)?;
    let file = crate::lifecycle::platform::open(path, true, false)?;
    try_lock(file, kind)
}

pub(crate) fn lock_resource(resource: &Path) -> Result<(Lock, PathBuf), Error> {
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

fn try_lock(file: fs::File, kind: &str) -> Result<Lock, Error> {
    match Lock::acquire(file) {
        Ok(lock) => Ok(lock),
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
    // Recovery may contain a native credential: reject widened access before
    // creating a candidate, not only before publishing it.
    if require_user_private {
        validate_private_snapshot(path, expected)?;
    }
    let temporary = temporary_path(path, "candidate");
    let result = (|| -> Result<(), Error> {
        let mut file = crate::lifecycle::platform::open(&temporary, true, true)?;
        file.write_all(bytes)?;
        file.sync_all()?;
        drop(file);
        if let Some(expected) = expected {
            prepare_existing_replacement(path, &temporary, expected, kind, require_user_private)?;
            if require_user_private {
                // Perform the access check after candidate metadata has been
                // prepared, immediately before the final identity/byte check.
                // An external editor can still race the final check and rename;
                // ConfigPatch does not claim filesystem CAS against it.
                validate_private_snapshot(path, Some(expected))?;
                let candidate = read_regular(&temporary, "private client candidate")?
                    .ok_or_else(|| Error::message("private client candidate disappeared"))?;
                validate_private_snapshot(&temporary, Some(&candidate))?;
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
        crate::lifecycle::platform::directory_tree(parent)?;
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
        current.identity == expected.identity
            && current.metadata.file_attributes() == expected.metadata.file_attributes()
    }
}

#[cfg(unix)]
fn prepare_existing_replacement(
    path: &Path,
    temporary: &Path,
    expected: &Snapshot,
    kind: &str,
    require_user_private: bool,
) -> Result<(), Error> {
    use std::os::unix::fs::MetadataExt;
    #[cfg(not(target_os = "macos"))]
    let _ = require_user_private;
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
        if require_user_private && !acl.is_empty() {
            return Err(Error::message("private client config has an extended ACL"));
        }
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
    fs::OpenOptions::new()
        .write(true)
        .open(temporary)?
        .sync_all()?;
    Ok(())
}

#[cfg(windows)]
fn prepare_existing_replacement(
    _path: &Path,
    _temporary: &Path,
    _expected: &Snapshot,
    _kind: &str,
    _require_user_private: bool,
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
    let expected = match crate::lifecycle::platform::read(path) {
        Ok(file) => Some(read_snapshot(file, "protected patch journal")?),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => None,
        Err(error) => return Err(error.into()),
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

#[cfg(test)]
mod tests {
    use super::*;

    struct Fixture(PathBuf);

    impl Fixture {
        fn new() -> Self {
            let path = std::env::temp_dir().join(format!(
                "llmgw-snapshot-{}-{:016x}",
                std::process::id(),
                rand::random::<u64>()
            ));
            fs::create_dir(&path).unwrap();
            Self(path)
        }
    }

    impl Drop for Fixture {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    #[cfg(unix)]
    #[test]
    fn completed_transaction_releases_lock_with_duplicate_handle_open() {
        let fixture = Fixture::new();
        let directory = fixture.0.join("state");
        crate::lifecycle::platform::directory(&directory).unwrap();
        let path = directory.join("transaction.lock");
        let file = crate::lifecycle::platform::open(&path, true, false).unwrap();
        let duplicate = file.try_clone().unwrap();
        let guard = try_lock(file, "transaction").unwrap();
        assert!(lock_path(&path, "transaction").is_err());

        drop(guard);
        let next = lock_path(&path, "transaction");
        assert!(next.is_ok(), "completed owner must release its lock");
        drop(next);
        drop(duplicate);
    }

    #[test]
    fn equal_bytes_replacement_has_different_snapshot_identity() {
        let fixture = Fixture::new();
        let path = fixture.0.join("config.json");
        let bytes = b"{\"same\":true}";
        fs::write(&path, bytes).unwrap();
        let expected = read_regular(&path, "test config").unwrap().unwrap();
        let unchanged = read_regular(&path, "test config").unwrap().unwrap();
        assert!(same_snapshot(&unchanged, &expected));

        // Retain the original file so its ID cannot be recycled for the replacement.
        fs::rename(&path, fixture.0.join("original.json")).unwrap();
        fs::write(&path, bytes).unwrap();
        let replacement = read_regular(&path, "test config").unwrap().unwrap();
        assert_eq!(replacement.bytes, expected.bytes);
        assert_eq!(replacement.metadata.len(), expected.metadata.len());
        assert!(!same_snapshot(&replacement, &expected));
        let error =
            atomic_write_client(&path, b"{}", Some(&expected), "test config", false).unwrap_err();
        assert!(error.to_string().contains("changed"));
        assert_eq!(fs::read(&path).unwrap(), bytes);
    }

    #[test]
    fn opened_snapshot_rejects_a_directory() {
        let fixture = Fixture::new();
        assert!(read_regular(&fixture.0, "test config").is_err());
        let mut options = OpenOptions::new();
        options.read(true);
        #[cfg(windows)]
        {
            use std::os::windows::fs::OpenOptionsExt;
            use windows_sys::Win32::Storage::FileSystem::FILE_FLAG_BACKUP_SEMANTICS;
            options.custom_flags(FILE_FLAG_BACKUP_SEMANTICS);
        }
        let directory = options.open(&fixture.0).unwrap();
        assert!(read_snapshot(directory, "test config").is_err());
    }

    #[cfg(windows)]
    #[test]
    #[ignore = "requires Windows Developer Mode or symbolic-link privilege"]
    fn windows_snapshot_rejects_a_file_reparse_point() {
        use std::os::windows::fs::{OpenOptionsExt, symlink_file};
        use windows_sys::Win32::Storage::FileSystem::{
            FILE_FLAG_BACKUP_SEMANTICS, FILE_FLAG_OPEN_REPARSE_POINT,
        };
        let fixture = Fixture::new();
        let path = fixture.0.join("config.json");
        let link = fixture.0.join("link.json");
        fs::write(&path, b"{}").unwrap();
        symlink_file(&path, &link).unwrap();
        assert!(read_regular(&link, "test config").is_err());
        let file = OpenOptions::new()
            .read(true)
            .custom_flags(FILE_FLAG_OPEN_REPARSE_POINT | FILE_FLAG_BACKUP_SEMANTICS)
            .open(&link)
            .unwrap();
        assert!(read_snapshot(file, "test config").is_err());
    }
}
