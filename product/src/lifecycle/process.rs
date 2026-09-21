use super::platform;
use crate::config::StatePaths;
use fs4::FileExt;
use std::{
    fs::File,
    io,
    path::Path,
    process::{Child, Command, Stdio},
    time::Duration,
};
/// Stable file object: never unlink a lock path, including on normal shutdown.
pub struct Lock {
    _file: File,
}
impl Lock {
    pub(crate) fn acquire(file: File) -> Result<Self, fs4::TryLockError> {
        FileExt::try_lock(&file)?;
        Ok(Self { _file: file })
    }
    pub fn attempt(paths: &StatePaths, name: &str) -> io::Result<Option<Self>> {
        let file = platform::open(&paths.directory.join(name), true, false)?;
        match Self::acquire(file) {
            Ok(lock) => Ok(Some(lock)),
            Err(fs4::TryLockError::WouldBlock) => Ok(None),
            Err(fs4::TryLockError::Error(e)) => Err(e),
        }
    }
    pub async fn operation(paths: &StatePaths) -> io::Result<Self> {
        let deadline = tokio::time::Instant::now() + Duration::from_secs(17);
        loop {
            if let Some(lock) = Self::attempt(paths, "operation.lock")? {
                return Ok(lock);
            }
            if tokio::time::Instant::now() >= deadline {
                return Err(io::Error::other("lifecycle_busy"));
            }
            tokio::time::sleep(Duration::from_millis(25)).await;
        }
    }
}
impl Drop for Lock {
    fn drop(&mut self) {
        // Closing alone can retain the lock through a forked or duplicated handle.
        let _ = FileExt::unlock(&self._file);
    }
}
pub fn spawn(config: &Path, fingerprint: &str) -> io::Result<Child> {
    let mut command = Command::new(std::env::current_exe()?);
    command
        .arg("--config")
        .arg(config)
        .arg("worker")
        .arg("--fingerprint")
        .arg(fingerprint)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    platform::configure_detached(&mut command)?;
    command.spawn()
}
