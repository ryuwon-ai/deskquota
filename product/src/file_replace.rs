//! Shared Windows replacement recovery used by setup and ConfigPatch.
//!
//! `ReplaceFileW` may remove or rename the destination on some failures. The
//! caller therefore retains independently written, synced private copies of
//! both sides until the published bytes have been verified.
#[cfg(any(windows, test))]
use std::{
    fs,
    io::{Read, Write},
    path::{Path, PathBuf},
};

#[cfg(any(windows, test))]
pub(crate) fn replace_existing_with_recovery<F>(
    replacement: &Path,
    destination: &Path,
    expected_original: &[u8],
    expected_replacement: &[u8],
    kind: &str,
    replace: F,
) -> std::io::Result<()>
where
    F: FnOnce(&Path, &Path) -> std::io::Result<()>,
{
    fn suffixed(path: &Path, suffix: &str) -> PathBuf {
        let mut name = path.as_os_str().to_owned();
        name.push(suffix);
        PathBuf::from(name)
    }

    fn read_protected(path: &Path) -> std::io::Result<Vec<u8>> {
        let mut file = crate::lifecycle::platform::read(path)?;
        let mut bytes = Vec::new();
        file.read_to_end(&mut bytes)?;
        Ok(bytes)
    }

    fn retained(
        error: std::io::Error,
        kind: &str,
        recovery: &Path,
        candidate: &Path,
    ) -> std::io::Error {
        std::io::Error::new(
            error.kind(),
            format!(
                "{kind} replacement was not verified; original recovery is at {} and replacement candidate is at {}: {error}",
                recovery.display(),
                candidate.display()
            ),
        )
    }

    if read_protected(destination)? != expected_original {
        return Err(std::io::Error::other(format!(
            "existing {kind} changed before replacement recovery was prepared"
        )));
    }
    if read_protected(replacement)? != expected_replacement {
        return Err(std::io::Error::other(format!(
            "temporary {kind} changed before publication"
        )));
    }

    let recovery = suffixed(replacement, ".recovery");
    let candidate = suffixed(replacement, ".candidate");
    let prepared = (|| -> std::io::Result<()> {
        let mut file = crate::lifecycle::platform::open(&recovery, true, true)?;
        file.write_all(expected_original)?;
        file.sync_all()?;
        drop(file);
        let mut file = crate::lifecycle::platform::open(&candidate, true, true)?;
        file.write_all(expected_replacement)?;
        file.sync_all()?;
        drop(file);
        if read_protected(&recovery)? != expected_original
            || read_protected(&candidate)? != expected_replacement
            || read_protected(destination)? != expected_original
        {
            return Err(std::io::Error::other(format!(
                "{kind} changed before replacement"
            )));
        }
        Ok(())
    })();
    if let Err(error) = prepared {
        let _ = fs::remove_file(&candidate);
        let _ = fs::remove_file(&recovery);
        return Err(error);
    }

    if let Err(error) = replace(replacement, destination) {
        return Err(retained(error, kind, &recovery, &candidate));
    }
    match read_protected(destination) {
        Ok(current) if current == expected_replacement => {}
        Ok(_) => {
            return Err(retained(
                std::io::Error::other("published content does not match the reviewed bytes"),
                kind,
                &recovery,
                &candidate,
            ));
        }
        Err(error) => return Err(retained(error, kind, &recovery, &candidate)),
    }

    if let Err(error) = fs::remove_file(&candidate) {
        return Err(retained(error, kind, &recovery, &candidate));
    }
    fs::remove_file(&recovery).map_err(|error| {
        std::io::Error::new(
            error.kind(),
            format!(
                "{kind} was replaced and verified, but original recovery cleanup failed at {}: {error}",
                recovery.display()
            ),
        )
    })
}
