use super::platform;
use crate::config::StatePaths;
use serde::{Deserialize, Serialize};
use std::{
    fs,
    io::{self, Read, Write},
    net::SocketAddr,
    path::{Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
};
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Identity {
    pub path_hash: String,
    pub fingerprint: String,
    pub nonce: String,
    pub started_unix_ms: u64,
    pub address: SocketAddr,
    pub pid: u32,
}
impl Identity {
    pub fn new(path: &Path, fingerprint: String, address: SocketAddr) -> Self {
        Self {
            path_hash: StatePaths::path_hash(path),
            fingerprint,
            nonce: random_token(),
            started_unix_ms: SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default()
                .as_millis()
                .min(u128::from(u64::MAX)) as u64,
            address,
            pid: std::process::id(),
        }
    }
    pub fn valid_for(&self, path: &Path) -> bool {
        self.path_hash == StatePaths::path_hash(path)
            && self.address.ip().is_loopback()
            && self.address.port() != 0
            && self.nonce.len() == 64
            && self.fingerprint.len() == 64
    }
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Record {
    pub identity: Identity,
    pub state: String,
}
pub fn canonical(path: &Path) -> io::Result<(PathBuf, StatePaths)> {
    let path = fs::canonicalize(path)?;
    let paths = StatePaths::from_config_path(&path).map_err(io::Error::other)?;
    Ok((path, paths))
}
pub fn random_token() -> String {
    use std::fmt::Write;
    let mut s = String::with_capacity(64);
    for byte in rand::random::<[u8; 32]>() {
        write!(s, "{byte:02x}").expect("String");
    }
    s
}
pub fn provision_tokens(paths: &StatePaths) -> io::Result<()> {
    for path in [&paths.control_token] {
        match platform::open(path, false, true) {
            Ok(mut file) => {
                file.write_all(random_token().as_bytes())?;
                file.sync_all()?;
            }
            Err(e) if e.kind() == io::ErrorKind::AlreadyExists => {
                platform::read(path)?;
            }
            Err(e) => return Err(e),
        }
    }
    Ok(())
}
pub fn read_record(paths: &StatePaths) -> io::Result<Record> {
    let mut bytes = Vec::new();
    platform::open(&paths.runtime_state, false, false)?
        .take(8193)
        .read_to_end(&mut bytes)?;
    if bytes.len() > 8192 {
        return Err(io::Error::other("runtime record too large"));
    }
    serde_json::from_slice(&bytes).map_err(|_| io::Error::other("invalid runtime record"))
}
pub fn write_record(paths: &StatePaths, record: &Record) -> io::Result<()> {
    // Existing state is never silently replaced to repair its permissions.
    match std::fs::symlink_metadata(&paths.runtime_state) {
        Ok(_) => {
            platform::read(&paths.runtime_state)?;
        }
        Err(error) if error.kind() == io::ErrorKind::NotFound => {}
        Err(error) => return Err(error),
    }
    // One worker owns writes under its lifetime lock; a fresh file avoids partial-reader JSON.
    let temp = paths
        .directory
        .join(format!("runtime-{}.tmp", record.identity.nonce));
    let result = (|| {
        let mut file = platform::open(&temp, false, true)?;
        serde_json::to_writer(&mut file, record)?;
        file.sync_all()?;
        fs::rename(&temp, &paths.runtime_state)
    })(); // destination is never read or followed
    if result.is_err() {
        let _ = fs::remove_file(temp);
    }
    result
}
