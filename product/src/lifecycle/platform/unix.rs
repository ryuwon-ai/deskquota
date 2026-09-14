use std::{
    fs::{self, File, OpenOptions},
    io,
    os::unix::fs::{DirBuilderExt, MetadataExt, OpenOptionsExt},
    path::Path,
    process::Command,
};
pub fn detach_session() -> io::Result<()> {
    rustix::process::setsid().map(|_| ()).map_err(Into::into)
}
pub fn configure_detached(_command: &mut Command) {}
pub fn current_user_identity() -> io::Result<String> {
    Err(io::Error::new(
        io::ErrorKind::Unsupported,
        "Windows current-user identity is unavailable on this platform",
    ))
}
fn validate(metadata: &fs::Metadata, directory: bool) -> io::Result<()> {
    if metadata.uid() != rustix::process::geteuid().as_raw()
        || metadata.mode() & 0o077 != 0
        || (if directory {
            !metadata.is_dir()
        } else {
            !metadata.is_file()
        })
    {
        return Err(io::Error::new(
            io::ErrorKind::PermissionDenied,
            "state must be owned by this user, private, and not a link",
        ));
    }
    Ok(())
}
pub fn directory(path: &Path) -> io::Result<()> {
    match fs::DirBuilder::new().mode(0o700).create(path) {
        Ok(()) => {}
        Err(e) if e.kind() == io::ErrorKind::AlreadyExists => {}
        Err(e) => return Err(e),
    }
    let metadata = fs::symlink_metadata(path)?;
    validate(&metadata, true)?;
    validate_acl(path, &metadata)
}
pub fn open(path: &Path, create: bool, exclusive: bool) -> io::Result<File> {
    open_mode(path, create, exclusive, true)
}
pub fn read(path: &Path) -> io::Result<File> {
    open_mode(path, false, false, false)
}
fn open_mode(path: &Path, create: bool, exclusive: bool, write: bool) -> io::Result<File> {
    let mut options = OpenOptions::new();
    options
        .read(true)
        .write(write)
        .mode(0o600)
        .custom_flags(libc::O_NOFOLLOW | libc::O_CLOEXEC | libc::O_NONBLOCK);
    if exclusive {
        options.create_new(true);
    } else {
        options.create(create);
    }
    let file = options.open(path)?;
    let metadata = file.metadata()?;
    validate(&metadata, false)?;
    validate_acl(path, &metadata)?;
    Ok(file)
}

fn validate_acl(path: &Path, opened: &fs::Metadata) -> io::Result<()> {
    #[cfg(target_os = "macos")]
    {
        // macOS extended ACL grants are independent of mode bits. Reject the
        // whole extended ACL (including inherited entries), never modify it.
        // exacl uses acl_get_link_np here and cannot follow a substituted link.
        let acl = exacl::getfacl(path, exacl::AclOption::SYMLINK_ACL).map_err(|_| {
            io::Error::new(io::ErrorKind::PermissionDenied, "cannot validate state ACL")
        })?;
        let current = fs::symlink_metadata(path)?;
        if !acl.is_empty() || current.dev() != opened.dev() || current.ino() != opened.ino() {
            return Err(io::Error::new(
                io::ErrorKind::PermissionDenied,
                "state extended ACL or concurrent path change is not allowed",
            ));
        }
    }
    // Linux POSIX ACL group bits represent ACL_MASK: 0o077 == 0 denies all
    // named users/groups and other access. Creation mode also masks defaults.
    #[cfg(not(target_os = "macos"))]
    let _ = (path, opened);
    Ok(())
}
