//! Native one-shot commands and one owned server. No PID-based signal or watchdog.
pub mod identity;
pub(crate) mod platform;
mod process;
use crate::{
    config::{ConfigFingerprint, LoadedConfig, StatePaths},
    server::{self, RuntimeCredentials},
};
use identity::{Identity, Record};
pub(crate) use process::Lock;
use serde_json::{Value, json};
use std::{
    io::{self, Write},
    path::Path,
    time::Duration,
};
pub type Error = Box<dyn std::error::Error>;
// Internal child outcome only; public commands continue to report failure as 1.
pub(crate) const WORKER_PORT_IN_USE_EXIT: u8 = 10;
const READY: Duration = Duration::from_secs(5);
const STOP: Duration = Duration::from_secs(12);
fn failure(reason: &'static str) -> Error {
    io::Error::other(reason).into()
}
fn client() -> Result<reqwest::Client, Error> {
    Ok(reqwest::Client::builder()
        .no_proxy()
        .redirect(reqwest::redirect::Policy::none())
        .retry(reqwest::retry::never())
        .connect_timeout(Duration::from_millis(300))
        .timeout(Duration::from_millis(500))
        .build()?)
}
async fn handshake(paths: &StatePaths, id: &Identity, stop: bool) -> Result<Value, Error> {
    let token = server::read_secret_file(&paths.control_token)?;
    let client = client()?;
    let endpoint = if stop { "stop" } else { "health" };
    let request = if stop {
        client.post(format!("http://{}/_llmgw/{endpoint}", id.address))
    } else {
        client.get(format!("http://{}/_llmgw/{endpoint}", id.address))
    };
    let request = if stop {
        request.header("x-llmgw-instance-nonce", &id.nonce)
    } else {
        request
    };
    let mut response = request
        .header(crate::control::CONTROL_TOKEN_HEADER, token)
        .send()
        .await
        .map_err(|_| failure("control_unreachable"))?;
    if !response.status().is_success() {
        return Err(failure("control_authentication_failed"));
    }
    let mut bytes = Vec::new();
    while let Some(chunk) = response
        .chunk()
        .await
        .map_err(|_| failure("control_read_failed"))?
    {
        if bytes.len() + chunk.len() > 65536 {
            return Err(failure("control_response_too_large"));
        }
        bytes.extend_from_slice(&chunk);
    }
    let value: Value =
        serde_json::from_slice(&bytes).map_err(|_| failure("invalid_control_response"))?;
    if !stop {
        let received: Identity = serde_json::from_value(value["identity"].clone())
            .map_err(|_| failure("identity_unknown"))?;
        if &received != id {
            return Err(failure("identity_mismatch"));
        }
    }
    Ok(value)
}
fn stopped(paths: &StatePaths) -> Value {
    let record = identity::read_record(paths).ok();
    let stale = paths.runtime_state.exists()
        && record
            .as_ref()
            .is_none_or(|record| record.state != "stopped");
    json!({"state":"stopped","pending_restart":false,"stale_runtime":stale,"last_worker_state":record.map(|record|record.state),"state_directory":paths.directory,"autostart":"not_implemented","clients":"not_implemented"})
}
pub async fn status(path: &Path) -> Result<Value, Error> {
    let (path, paths) = identity::canonical(path)?;
    if !paths.directory.try_exists()? {
        return Ok(stopped(&paths));
    }
    platform::directory(&paths.directory)?;
    if Lock::attempt(&paths, "worker.lock")?.is_some() {
        return Ok(stopped(&paths));
    }
    let record = identity::read_record(&paths)
        .map_err(|_| failure("failed: worker lock held; identity unknown"))?;
    if !record.identity.valid_for(&path) {
        return Err(failure("failed: worker identity invalid"));
    }
    let runtime = handshake(&paths, &record.identity, false).await?;
    let current = std::fs::read(&path)
        .ok()
        .map(|b| ConfigFingerprint::from_bytes(&b));
    Ok(
        json!({"state":runtime["state"],"identity":record.identity,"pending_restart":current.as_ref().is_none_or(|fp|fp.as_str()!=record.identity.fingerprint),"runtime":runtime,"state_directory":paths.directory,"autostart":"not_implemented","clients":"not_implemented"}),
    )
}

/// Initialize the protected, config-scoped credentials created by a completed
/// setup apply without starting or changing a worker.
pub(crate) async fn initialize_setup_state(
    path: &Path,
    expected_fingerprint: &str,
) -> Result<(), Error> {
    let (path, paths) = identity::canonical(path)?;
    platform::directory(&paths.directory)?;
    let _operation = Lock::operation(&paths).await?;
    let loaded = LoadedConfig::load(&path)?;
    if loaded.source_path != path {
        return Err(failure("configuration_path_changed"));
    }
    if loaded.fingerprint.as_str() != expected_fingerprint {
        return Err(failure("configuration_changed_before_state_initialization"));
    }
    let _worker = Lock::attempt(&paths, "worker.lock")?;
    if _worker.is_some() {
        identity::provision_tokens(&paths)?;
    }
    // A worker may already have loaded these credentials. Validate the
    // existing protected files in both branches, but never repair or rotate
    // one under it. Keep an available worker lock through validation.
    let control = server::read_secret_file(&paths.control_token)?;
    RuntimeCredentials::new(&control, None)?;
    Ok(())
}

pub async fn on(path: &Path) -> Result<Value, Error> {
    on_inner(path, None).await
}
pub(crate) async fn on_expected(path: &Path, expected: &str) -> Result<Value, Error> {
    on_inner(path, Some(expected)).await
}
async fn on_inner(path: &Path, expected: Option<&str>) -> Result<Value, Error> {
    let (path, paths) = identity::canonical(path)?;
    platform::directory(&paths.directory)?;
    let _operation = Lock::operation(&paths).await?;
    // A running worker owns its immutable config even if the saved edit cannot parse.
    let current = status(&path).await?;
    if current["state"] != "stopped" {
        if current["pending_restart"] == true {
            return Err(failure("restart_required"));
        }
        if expected.is_some_and(|value| current["identity"]["fingerprint"] != value) {
            return Err(failure("configuration_changed_before_start"));
        }
        return Ok(current);
    }
    let loaded = LoadedConfig::load(&path)?;
    if expected.is_some_and(|value| loaded.fingerprint.as_str() != value) {
        return Err(failure("configuration_changed_before_start"));
    }
    start(loaded).await
}
async fn start(loaded: LoadedConfig) -> Result<Value, Error> {
    let paths = &loaded.state_paths;
    if Lock::attempt(paths, "worker.lock")?.is_none() {
        let status = status(&loaded.source_path).await?;
        if status["pending_restart"] == true {
            return Err(failure("restart_required"));
        }
        return Ok(status);
    }
    // Validate credentials before launch and before restart can disrupt a healthy worker.
    identity::provision_tokens(paths)?;
    RuntimeCredentials::load(&loaded)?;
    let mut child = process::spawn(&loaded.source_path, loaded.fingerprint.as_str())?;
    let deadline = tokio::time::Instant::now() + READY;
    loop {
        if let Some(exit) = child.try_wait()? {
            if !exit.success() {
                return Err(failure(
                    if exit.code() == Some(i32::from(WORKER_PORT_IN_USE_EXIT)) {
                        "port_in_use"
                    } else {
                        "worker_start_failed"
                    },
                ));
            }
        }
        if let Ok(Ok(status)) = tokio::time::timeout_at(deadline, status(&loaded.source_path)).await
        {
            if status["state"] == "running" {
                if status["identity"]["fingerprint"] != loaded.fingerprint.as_str() {
                    return Err(failure("restart_required"));
                }
                return Ok(status);
            }
        }
        if tokio::time::Instant::now() >= deadline {
            return Err(failure(
                "readiness_timeout: state unknown; no process was killed",
            ));
        }
        tokio::time::sleep(Duration::from_millis(25)).await;
    }
}
pub async fn off(path: &Path) -> Result<Value, Error> {
    let (path, paths) = identity::canonical(path)?;
    if !paths.directory.try_exists()? {
        return Ok(stopped(&paths));
    }
    platform::directory(&paths.directory)?;
    let _operation = Lock::operation(&paths).await?;
    stop(&path, &paths).await
}
async fn stop(path: &Path, paths: &StatePaths) -> Result<Value, Error> {
    if Lock::attempt(paths, "worker.lock")?.is_some() {
        return Ok(stopped(paths));
    }
    let record = identity::read_record(paths)
        .map_err(|_| failure("failed: worker lock held; identity unknown"))?;
    if !record.identity.valid_for(path) {
        return Err(failure("failed: worker identity invalid"));
    }
    let deadline = tokio::time::Instant::now() + STOP;
    handshake(paths, &record.identity, false).await?;
    handshake(paths, &record.identity, true).await?;
    loop {
        if let Some(_lock) = Lock::attempt(paths, "worker.lock")? {
            return Ok(stopped(paths));
        }
        if tokio::time::Instant::now() >= deadline {
            return Err(failure(
                "stop_timeout: state unknown; no process was killed",
            ));
        }
        tokio::time::sleep(Duration::from_millis(25)).await;
    }
}
pub async fn restart(path: &Path) -> Result<Value, Error> {
    restart_inner(path, None).await
}
pub(crate) async fn restart_expected(path: &Path, expected: &str) -> Result<Value, Error> {
    restart_inner(path, Some(expected)).await
}
async fn restart_inner(path: &Path, expected: Option<&str>) -> Result<Value, Error> {
    let (path, paths) = identity::canonical(path)?;
    platform::directory(&paths.directory)?;
    let _operation = Lock::operation(&paths).await?;
    let loaded = LoadedConfig::load(&path)?;
    if loaded.source_path != path {
        return Err(failure("configuration_path_changed"));
    }
    if expected.is_some_and(|value| loaded.fingerprint.as_str() != value) {
        return Err(failure("configuration_changed_before_restart"));
    }
    identity::provision_tokens(&loaded.state_paths)?;
    RuntimeCredentials::load(&loaded)?;
    server::preflight_upstream(&loaded.config)?;
    stop(&loaded.source_path, &loaded.state_paths).await?;
    start(loaded).await
}
pub fn prepare_worker() -> Result<(), Error> {
    Ok(platform::detach_session()?)
}
pub async fn worker(path: &Path, expected: Option<&str>) -> Result<(), Error> {
    let loaded = LoadedConfig::load(path)?;
    if expected.is_some_and(|fp| fp != loaded.fingerprint.as_str()) {
        return Err(failure("configuration_changed_before_worker_start"));
    }
    let paths = loaded.state_paths.clone();
    platform::directory(&paths.directory)?;
    let _lock =
        Lock::attempt(&paths, "worker.lock")?.ok_or_else(|| failure("worker_already_running"))?;
    let mut log = platform::open(&paths.directory.join("worker.log"), true, false)?;
    log.set_len(0)?;
    log.write_all(b"starting\n")?;
    let result = async {
        identity::provision_tokens(&paths)?;
        let mut credentials = RuntimeCredentials::load(&loaded)?;
        credentials.identity = Some(Identity::new(
            &loaded.source_path,
            loaded.fingerprint.as_str().to_owned(),
            loaded.config.listen,
        ));
        let gateway = server::spawn(loaded.config, credentials).await?;
        let identity = gateway.identity().expect("owned identity").clone();
        if let Err(error) = identity::write_record(
            &paths,
            &Record {
                identity: identity.clone(),
                state: "running".into(),
            },
        ) {
            gateway.shutdown().await?;
            return Err(Error::from(error));
        }
        let result = gateway.run_foreground().await;
        identity::write_record(
            &paths,
            &Record {
                identity,
                state: if result.is_ok() { "stopped" } else { "failed" }.into(),
            },
        )?;
        result.map_err(Error::from)
    }
    .await;
    // One bounded sanitized event, never upstream errors, URLs or payloads.
    use std::io::{Seek, SeekFrom};
    log.set_len(0)?;
    log.seek(SeekFrom::Start(0))?;
    let message: &[u8] = match &result {
        Ok(()) => b"stopped\n",
        Err(error) if matches!(error.downcast_ref::<server::StartError>(), Some(server::StartError::Bind(e)) if e.kind() == io::ErrorKind::AddrInUse) => {
            b"port_in_use\n"
        }
        Err(_) => b"worker_start_failed\n",
    };
    log.write_all(message)?;
    log.sync_all()?;
    result
}
