//! Native CLI contract: only synthetic loopback configs and owned children.
use serde_json::Value;
use std::{
    fs,
    net::TcpListener,
    path::PathBuf,
    process::{Command, Output},
    sync::atomic::{AtomicU64, Ordering},
};
static NEXT: AtomicU64 = AtomicU64::new(0);
struct Fixture {
    dir: PathBuf,
    config: PathBuf,
    original: String,
}
impl Fixture {
    fn new(port: u16) -> Self {
        let dir = std::env::temp_dir().join(format!(
            "llmgw-native-{}-{} 한글",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir_all(&dir).unwrap();
        let config = dir.join("gateway config.toml");
        let original = format!(
            r#"listen = "127.0.0.1:{port}"
[upstream]
api_base = "http://127.0.0.1:9/v1"
[upstream.auth]
mode = "none"
[quota.rpm]
kind = "unlimited"
[quota.tpm]
kind = "unknown"
[[models]]
id = "fixture"
max_output_tokens = 32
[[roots]]
id = "fixture"
endpoints = ["models", "chat/completions"]
models = ["fixture"]
"#
        );
        fs::write(&config, &original).unwrap();
        Self {
            dir,
            config,
            original,
        }
    }
    fn command(&self, op: &str) -> Command {
        let mut c = Command::new(env!("CARGO_BIN_EXE_llmgw"));
        c.args([op, "--config"]).arg(&self.config);
        c
    }
    fn call(&self, op: &str) -> Output {
        self.command(op).output().unwrap()
    }
    fn ok(&self, op: &str) -> Value {
        let out = self.command(op).arg("--json").output().unwrap();
        assert!(
            out.status.success(),
            "{op} failed: {}",
            String::from_utf8_lossy(&out.stderr)
        );
        serde_json::from_slice(&out.stdout).expect("machine status")
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = fs::write(&self.config, &self.original);
        let _ = self.call("off");
        let _ = fs::remove_dir_all(&self.dir);
    }
}
#[test]
fn stopped_is_success_and_off_is_idempotent() {
    let f = Fixture::new(0);
    assert_eq!(f.ok("status")["state"], "stopped");
    assert!(f.call("off").status.success());
    assert!(f.call("off").status.success());
}
#[test]
fn concurrent_on_has_one_identity() {
    let f = Fixture::new(0);
    let mut a = f
        .command("on")
        .stdout(std::process::Stdio::null())
        .spawn()
        .unwrap();
    let mut b = f
        .command("on")
        .stdout(std::process::Stdio::null())
        .spawn()
        .unwrap();
    let a = a.wait().unwrap();
    let b = b.wait().unwrap();
    assert!(a.success() && b.success(), "both on must succeed");
    let first = f.ok("status");
    assert_eq!(first["state"], "running");
    assert!(f.call("on").status.success());
    assert_eq!(
        first["identity"]["nonce"],
        f.ok("status")["identity"]["nonce"]
    );
    assert!(f.call("off").status.success());
    assert_eq!(f.ok("status")["state"], "stopped");
}
#[test]
fn occupied_port_does_not_stop_unrelated_listener() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let f = Fixture::new(listener.local_addr().unwrap().port());
    let out = f.call("on");
    assert_eq!(out.status.code(), Some(1));
    assert!(String::from_utf8_lossy(&out.stderr).contains("port_in_use"));
    assert!(std::net::TcpStream::connect(listener.local_addr().unwrap()).is_ok());
}
#[test]
fn changed_config_requires_restart_and_invalid_edit_can_stop_old_port() {
    let f = Fixture::new(0);
    assert!(f.call("on").status.success());
    let old = f.ok("status");
    fs::write(&f.config, format!("{}\n# changed", f.original)).unwrap();
    let out = f.call("on");
    assert_eq!(out.status.code(), Some(1));
    assert!(String::from_utf8_lossy(&out.stderr).contains("restart_required"));
    assert_eq!(f.ok("status")["pending_restart"], true);
    assert!(f.call("restart").status.success());
    assert_ne!(
        old["identity"]["nonce"],
        f.ok("status")["identity"]["nonce"]
    );
    fs::write(&f.config, "invalid = [").unwrap();
    assert!(!f.call("restart").status.success());
    assert!(f.call("off").status.success());
    assert_eq!(f.ok("status")["state"], "stopped");
}
#[test]
fn malformed_running_config_requires_restart_without_replacing_worker() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    listener.set_nonblocking(true).unwrap();
    let f = Fixture::new(0);
    fs::write(
        &f.config,
        f.original
            .replace("127.0.0.1:9", &listener.local_addr().unwrap().to_string()),
    )
    .unwrap();
    assert!(f.call("on").status.success());
    let old = f.ok("status");
    fs::write(&f.config, "invalid = [").unwrap();
    let result = f.call("on");
    assert_eq!(result.status.code(), Some(1));
    assert!(String::from_utf8_lossy(&result.stderr).contains("restart_required"));
    assert!(!f.call("restart").status.success());
    let current = f.ok("status");
    assert_eq!(current["state"], "running");
    assert_eq!(current["pending_restart"], true);
    assert_eq!(current["identity"], old["identity"]);
    assert_eq!(
        listener.accept().unwrap_err().kind(),
        std::io::ErrorKind::WouldBlock
    );
    assert!(f.call("off").status.success());
}

#[test]
fn restart_preflights_ca_without_stopping_authenticated_worker_or_contacting_upstream() {
    let upstream = TcpListener::bind("127.0.0.1:0").unwrap();
    upstream.set_nonblocking(true).unwrap();
    let f = Fixture::new(0);
    fs::write(
        &f.config,
        f.original
            .replace("127.0.0.1:9", &upstream.local_addr().unwrap().to_string()),
    )
    .unwrap();
    assert!(f.call("on").status.success());
    let before = f.ok("status");

    let missing = f.dir.join("missing-ca.pem");
    let configured = fs::read_to_string(&f.config).unwrap().replace(
        "api_base = \"",
        &format!("ca_bundle = {missing:?}\napi_base = \""),
    );
    fs::write(&f.config, configured).unwrap();
    let result = f.call("restart");
    assert_eq!(result.status.code(), Some(1));
    assert!(
        String::from_utf8_lossy(&result.stderr).contains("failed to read configured CA bundle")
    );
    let after = f.ok("status");
    assert_eq!(after["state"], "running");
    assert_eq!(after["identity"], before["identity"]);
    assert_eq!(after["pending_restart"], true);
    assert_eq!(
        upstream.accept().unwrap_err().kind(),
        std::io::ErrorKind::WouldBlock
    );

    fs::write(
        &missing,
        b"-----BEGIN CERTIFICATE-----\nnot-valid-base64!\n-----END CERTIFICATE-----\n",
    )
    .unwrap();
    let malformed = f.call("restart");
    assert_eq!(malformed.status.code(), Some(1));
    assert!(
        String::from_utf8_lossy(&malformed.stderr)
            .contains("configured CA bundle is not valid PEM certificates")
    );
    let still_running = f.ok("status");
    assert_eq!(still_running["identity"], before["identity"]);
    assert_eq!(
        upstream.accept().unwrap_err().kind(),
        std::io::ErrorKind::WouldBlock
    );

    fs::write(&missing, b"").unwrap();
    let empty = f.call("restart");
    assert_eq!(empty.status.code(), Some(1));
    assert!(
        String::from_utf8_lossy(&empty.stderr)
            .contains("configured CA bundle contains no certificates")
    );
    assert_eq!(f.ok("status")["identity"], before["identity"]);
}
#[test]
fn malformed_stopped_config_never_provisions_tokens() {
    let f = Fixture::new(0);
    let paths = llmgw::config::LoadedConfig::load(&f.config)
        .unwrap()
        .state_paths;
    fs::write(&f.config, "invalid = [").unwrap();
    for _ in 0..2 {
        assert_eq!(f.call("on").status.code(), Some(2));
        assert!(!paths.data_token.exists());
        assert!(!paths.control_token.exists());
        assert_eq!(f.ok("status")["state"], "stopped");
    }
}
#[test]
fn stale_pid_never_kills_an_unrelated_process() {
    let f = Fixture::new(0);
    assert!(f.call("on").status.success());
    let status = f.ok("status");
    assert!(f.call("off").status.success());
    let dir = PathBuf::from(status["state_directory"].as_str().unwrap());
    // Rewrite the existing protected file; std::fs creation would inherit a
    // Windows DACL and fail protection checks before reaching stale-PID behavior.
    let runtime = dir.join("runtime.json");
    let mut record: Value = serde_json::from_slice(&fs::read(&runtime).unwrap()).unwrap();
    record["identity"]["pid"] = std::process::id().into();
    record["state"] = "running".into();
    fs::write(runtime, serde_json::to_vec(&record).unwrap()).unwrap();
    assert!(f.call("off").status.success());
    let stopped = f.ok("status");
    assert_eq!(stopped["state"], "stopped");
    assert_eq!(stopped["stale_runtime"], true);
    assert!(f.call("on").status.success());
}
#[test]
fn known_quota_is_ready_with_empty_queue_startup_hold() {
    let f = Fixture::new(0);
    fs::write(
        &f.config,
        f.original
            .replace("kind = \"unlimited\"", "kind = \"known\"\nvalue = 60"),
    )
    .unwrap();
    assert!(f.call("on").status.success());
    let s = f.ok("status");
    assert_eq!(s["state"], "running");
    assert_eq!(s["runtime"]["admission"]["queue_length"], 0);
    assert!(
        s["runtime"]["admission"]["startup_hold_ms"]
            .as_u64()
            .unwrap()
            > 0
    );
    assert!(matches!(
        s["autostart"]["registration"].as_str(),
        Some("disabled" | "registered" | "unknown" | "blocked")
    ));
    assert_eq!(s["autostart"]["current_shell_auth_is_login_proof"], false);
}
#[test]
fn same_directory_configs_have_independent_workers() {
    let f = Fixture::new(0);
    let other = f.dir.join("second.toml");
    fs::write(&other, &f.original).unwrap();
    assert!(f.call("on").status.success());
    let mut command = Command::new(env!("CARGO_BIN_EXE_llmgw"));
    let started = command
        .args(["--config"])
        .arg(&other)
        .arg("on")
        .output()
        .unwrap();
    let status = Command::new(env!("CARGO_BIN_EXE_llmgw"))
        .arg("--config")
        .arg(&other)
        .args(["status", "--json"])
        .output()
        .unwrap();
    let stopped = Command::new(env!("CARGO_BIN_EXE_llmgw"))
        .arg("--config")
        .arg(&other)
        .arg("off")
        .output()
        .unwrap();
    assert!(started.status.success() && status.status.success() && stopped.status.success());
    let second: Value = serde_json::from_slice(&status.stdout).unwrap();
    assert_ne!(
        f.ok("status")["identity"]["path_hash"],
        second["identity"]["path_hash"]
    );
    assert_eq!(f.ok("status")["state"], "running");
}
#[test]
fn control_stop_rejects_a_previous_instance_nonce() {
    let f = Fixture::new(0);
    assert!(f.call("on").status.success());
    let s = f.ok("status");
    let dir = PathBuf::from(s["state_directory"].as_str().unwrap());
    let token = fs::read_to_string(dir.join("control-token")).unwrap();
    let rt = tokio::runtime::Runtime::new().unwrap();
    let code = rt.block_on(async {
        reqwest::Client::builder()
            .no_proxy()
            .build()
            .unwrap()
            .post(format!(
                "http://{}/_llmgw/stop",
                s["identity"]["address"].as_str().unwrap()
            ))
            .header("x-llmgw-control-token", token)
            .header("x-llmgw-instance-nonce", "previous-instance")
            .send()
            .await
            .unwrap()
            .status()
    });
    assert_eq!(code, 409);
    assert_eq!(f.ok("status")["state"], "running");
}
#[test]
fn edited_listen_port_off_reaches_recorded_address() {
    let f = Fixture::new(0);
    assert!(f.call("on").status.success());
    let old = f.ok("status")["identity"]["address"]
        .as_str()
        .unwrap()
        .to_owned();
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    fs::write(
        &f.config,
        f.original
            .replace("127.0.0.1:0", &listener.local_addr().unwrap().to_string()),
    )
    .unwrap();
    assert!(f.call("off").status.success());
    assert!(std::net::TcpStream::connect(old).is_err());
    assert!(std::net::TcpStream::connect(listener.local_addr().unwrap()).is_ok());
}
#[test]
fn loopback_control_ignores_proxy_environment() {
    let f = Fixture::new(0);
    assert!(f.call("on").status.success());
    let out = f
        .command("status")
        .env("ALL_PROXY", "http://127.0.0.1:9")
        .env("HTTP_PROXY", "http://127.0.0.1:9")
        .env("NO_PROXY", "")
        .arg("--json")
        .output()
        .unwrap();
    assert!(out.status.success());
}
#[cfg(unix)]
#[test]
fn weak_existing_state_permissions_are_rejected_without_chmod() {
    use std::os::unix::fs::PermissionsExt;
    let f = Fixture::new(0);
    let loaded = llmgw::config::LoadedConfig::load(&f.config).unwrap();
    fs::create_dir(&loaded.state_paths.directory).unwrap();
    fs::set_permissions(
        &loaded.state_paths.directory,
        fs::Permissions::from_mode(0o755),
    )
    .unwrap();
    assert!(!f.call("on").status.success());
    assert_eq!(
        fs::metadata(&loaded.state_paths.directory)
            .unwrap()
            .permissions()
            .mode()
            & 0o777,
        0o755
    );
}
#[test]
fn off_drains_a_live_request_closes_sockets_and_exposes_draining() {
    use std::io::{Read, Write};
    use std::time::{Duration, Instant};
    let upstream = TcpListener::bind("127.0.0.1:0").unwrap();
    upstream.set_nonblocking(true).unwrap();
    let f = Fixture::new(0);
    fs::write(
        &f.config,
        f.original
            .replace("127.0.0.1:9", &upstream.local_addr().unwrap().to_string()),
    )
    .unwrap();
    assert!(f.call("on").status.success());
    let s = f.ok("status");
    let dir = PathBuf::from(s["state_directory"].as_str().unwrap());
    let token = fs::read_to_string(dir.join("data-token")).unwrap();
    let mut downstream =
        std::net::TcpStream::connect(s["identity"]["address"].as_str().unwrap()).unwrap();
    downstream
        .set_read_timeout(Some(Duration::from_secs(13)))
        .unwrap();
    write!(
        downstream,
        "GET /r/fixture/v1/models HTTP/1.1\r\nHost: localhost\r\nx-llmgw-token: {token}\r\n\r\n"
    )
    .unwrap();
    let deadline = Instant::now() + Duration::from_secs(3);
    let mut accepted = loop {
        match upstream.accept() {
            Ok((s, _)) => break s,
            Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                assert!(Instant::now() < deadline);
                std::thread::sleep(Duration::from_millis(20));
            }
            Err(e) => panic!("{e}"),
        }
    };
    accepted
        .set_read_timeout(Some(Duration::from_secs(13)))
        .unwrap();
    let mut bytes = [0; 4096];
    assert!(accepted.read(&mut bytes).unwrap() > 0);
    accepted.write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 10000\r\nContent-Type: application/json\r\n\r\nabc").unwrap();
    assert!(downstream.read(&mut bytes).unwrap() > 0);
    let before = Instant::now();
    let mut off = f
        .command("off")
        .stdout(std::process::Stdio::null())
        .spawn()
        .unwrap();
    std::thread::sleep(Duration::from_millis(300));
    let during = f.command("status").arg("--json").output().unwrap();
    let exit = off.wait().unwrap();
    let elapsed = before.elapsed();
    let upstream_closed = accepted.read(&mut bytes).map_or(true, |n| n == 0);
    let downstream_closed = downstream.read(&mut bytes).map_or(true, |n| n == 0);
    assert!(exit.success());
    assert!(elapsed >= Duration::from_secs(9) && elapsed < Duration::from_secs(13));
    assert!(upstream_closed && downstream_closed);
    assert!(
        during.status.success(),
        "draining status must remain queryable: {}",
        String::from_utf8_lossy(&during.stderr)
    );
    let status: Value = serde_json::from_slice(&during.stdout).unwrap();
    assert_eq!(status["state"], "draining");
    assert_eq!(f.ok("status")["state"], "stopped");
    assert!(TcpListener::bind(s["identity"]["address"].as_str().unwrap()).is_ok());
}
#[test]
fn held_lock_with_unknown_identity_is_failed_json_and_never_killed() {
    let f = Fixture::new(0);
    assert!(f.call("on").status.success());
    let s = f.ok("status");
    let runtime = PathBuf::from(s["state_directory"].as_str().unwrap()).join("runtime.json");
    let original = fs::read(&runtime).unwrap();
    let mut record: Value = serde_json::from_slice(&original).unwrap();
    record["identity"]["nonce"] = "0".repeat(64).into();
    fs::write(&runtime, serde_json::to_vec(&record).unwrap()).unwrap();
    let stop = f.call("off");
    let status = f.command("status").arg("--json").output().unwrap();
    fs::write(&runtime, original).unwrap();
    assert_eq!(stop.status.code(), Some(1));
    assert_eq!(f.ok("status")["identity"]["nonce"], s["identity"]["nonce"]);
    assert_eq!(status.status.code(), Some(1));
    let result: Value = serde_json::from_slice(&status.stdout).expect("failed status JSON");
    assert_eq!(result["state"], "failed");
    assert!(result["autostart"]["registration"].as_str().is_some());
    assert_eq!(
        result["autostart"]["current_shell_auth_is_login_proof"],
        false
    );
}
#[cfg(unix)]
#[test]
fn stale_record_of_owned_unrelated_child_preserves_child_and_lock_file() {
    use std::os::unix::fs::MetadataExt;
    struct ChildGuard(std::process::Child);
    impl Drop for ChildGuard {
        fn drop(&mut self) {
            let _ = self.0.kill();
            let _ = self.0.wait();
        }
    }
    let mut child = ChildGuard(Command::new("/bin/sleep").arg("60").spawn().unwrap());
    let f = Fixture::new(0);
    assert!(f.call("on").status.success());
    let s = f.ok("status");
    let dir = PathBuf::from(s["state_directory"].as_str().unwrap());
    assert!(f.call("off").status.success());
    let inode = fs::metadata(dir.join("worker.lock")).unwrap().ino();
    fs::write(
        dir.join("runtime.json"),
        format!(r#"{{"pid":{},"state":"running"}}"#, child.0.id()),
    )
    .unwrap();
    assert!(f.call("off").status.success());
    assert!(child.0.try_wait().unwrap().is_none());
    assert!(f.call("on").status.success());
    assert_eq!(fs::metadata(dir.join("worker.lock")).unwrap().ino(), inode);
    assert!(child.0.try_wait().unwrap().is_none());
}
#[test]
fn empty_queue_reports_shared_cooldown_without_a_new_request() {
    use std::io::{Read, Write};
    use std::time::Duration;
    let upstream = TcpListener::bind("127.0.0.1:0").unwrap();
    let address = upstream.local_addr().unwrap();
    let f = Fixture::new(0);
    fs::write(
        &f.config,
        f.original.replace("127.0.0.1:9", &address.to_string()),
    )
    .unwrap();
    assert!(f.call("on").status.success());
    let s = f.ok("status");
    let token = fs::read_to_string(
        PathBuf::from(s["state_directory"].as_str().unwrap()).join("data-token"),
    )
    .unwrap();
    let handle = std::thread::spawn(move || {
        upstream.set_nonblocking(true).unwrap();
        let deadline = std::time::Instant::now() + Duration::from_secs(3);
        let mut socket = loop {
            match upstream.accept() {
                Ok((s, _)) => break s,
                Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                    assert!(std::time::Instant::now() < deadline);
                    std::thread::sleep(Duration::from_millis(10));
                }
                Err(e) => panic!("{e}"),
            }
        };
        socket
            .set_read_timeout(Some(Duration::from_secs(3)))
            .unwrap();
        assert!(socket.read(&mut [0; 4096]).unwrap() > 0);
        socket.write_all(b"HTTP/1.1 429 Too Many Requests\r\nRetry-After: 120\r\nContent-Length: 2\r\nConnection: close\r\n\r\n{}").unwrap();
    });
    let rt = tokio::runtime::Runtime::new().unwrap();
    let code = rt.block_on(async {
        let response = reqwest::Client::builder()
            .no_proxy()
            .build()
            .unwrap()
            .get(format!(
                "http://{}/r/fixture/v1/models",
                s["identity"]["address"].as_str().unwrap()
            ))
            .header("x-llmgw-token", token)
            .timeout(Duration::from_secs(3))
            .send()
            .await
            .unwrap();
        let code = response.status();
        response.bytes().await.unwrap();
        code
    });
    handle.join().unwrap();
    assert_eq!(code, 429);
    let status = f.ok("status");
    assert_eq!(status["runtime"]["admission"]["queue_length"], 0);
    assert!(
        status["runtime"]["admission"]["shared_cooldown_ms"]
            .as_u64()
            .unwrap()
            > 110000
    );
}
#[cfg(target_os = "macos")]
#[test]
fn mac_extended_acl_is_rejected_before_token_write() {
    use std::os::unix::fs::PermissionsExt;
    let f = Fixture::new(0);
    let paths = llmgw::config::LoadedConfig::load(&f.config)
        .unwrap()
        .state_paths;
    fs::create_dir(&paths.directory).unwrap();
    fs::set_permissions(&paths.directory, fs::Permissions::from_mode(0o700)).unwrap();
    assert!(
        Command::new("/bin/chmod")
            .args([
                "+a",
                "everyone allow read,write,file_inherit,directory_inherit"
            ])
            .arg(&paths.directory)
            .status()
            .unwrap()
            .success()
    );
    let result = f.call("on");
    assert!(
        !result.status.success(),
        "extended ACL must not be accepted as private"
    );
    assert!(
        !paths.data_token.exists(),
        "reject unsafe directory before writing a token"
    );
}
#[cfg(target_os = "macos")]
#[test]
fn mac_existing_token_acl_is_rejected_and_preserved() {
    let f = Fixture::new(0);
    assert!(f.call("on").status.success());
    let s = f.ok("status");
    assert!(f.call("off").status.success());
    let token = PathBuf::from(s["state_directory"].as_str().unwrap()).join("data-token");
    assert!(
        Command::new("/bin/chmod")
            .args(["+a", "everyone allow read"])
            .arg(&token)
            .status()
            .unwrap()
            .success()
    );
    let before = exacl::getfacl(&token, None).unwrap();
    assert!(!f.call("on").status.success());
    assert_eq!(exacl::getfacl(&token, None).unwrap(), before);
}
#[test]
fn deleted_config_never_guesses_a_state_identity() {
    let f = Fixture::new(0);
    assert!(f.call("on").status.success());
    let s = f.ok("status");
    fs::remove_file(&f.config).unwrap();
    let out = f.call("off");
    fs::write(&f.config, &f.original).unwrap();
    assert!(!out.status.success());
    assert_eq!(f.ok("status")["identity"]["nonce"], s["identity"]["nonce"]);
}
#[cfg(unix)]
#[test]
fn existing_read_only_private_tokens_need_no_permission_changes() {
    use std::os::unix::fs::PermissionsExt;
    let f = Fixture::new(0);
    assert!(f.call("on").status.success());
    let s = f.ok("status");
    assert!(f.call("off").status.success());
    let token = PathBuf::from(s["state_directory"].as_str().unwrap()).join("data-token");
    fs::set_permissions(&token, fs::Permissions::from_mode(0o400)).unwrap();
    let result = f.call("on");
    assert_eq!(
        fs::metadata(&token).unwrap().permissions().mode() & 0o777,
        0o400
    );
    assert!(
        result.status.success(),
        "existing private token should only be read"
    );
}
#[test]
fn stale_port_log_cannot_classify_a_new_config_failure() {
    use fs4::FileExt;
    let f = Fixture::new(0);
    assert!(f.call("on").status.success());
    let status = f.ok("status");
    assert!(f.call("off").status.success());
    let dir = PathBuf::from(status["state_directory"].as_str().unwrap());
    fs::write(dir.join("worker.log"), b"port_in_use\n").unwrap();
    let lock = fs::OpenOptions::new()
        .read(true)
        .write(true)
        .open(dir.join("operation.lock"))
        .unwrap();
    FileExt::lock(&lock).unwrap();
    let child = f
        .command("on")
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .unwrap();
    // On now resolves the existing identity under the operation lock before
    // parsing a stopped instance. This edit must fail validation, never reuse
    // the previous worker's port-conflict diagnostic.
    std::thread::sleep(std::time::Duration::from_millis(300));
    fs::write(&f.config, "invalid = [").unwrap();
    drop(lock);
    let result = child.wait_with_output().unwrap();
    fs::write(&f.config, &f.original).unwrap();
    let error = String::from_utf8_lossy(&result.stderr);
    assert_eq!(result.status.code(), Some(2));
    assert!(
        error.contains("invalid configuration"),
        "expected new config validation failure, got {error}"
    );
    assert!(!error.contains("port_in_use"));
    assert_eq!(f.ok("status")["state"], "stopped");
}

fn check_restart_edit_during_operation_wait(valid_edit: bool) {
    use fs4::FileExt;
    let upstream = TcpListener::bind("127.0.0.1:0").unwrap();
    upstream.set_nonblocking(true).unwrap();
    let f = Fixture::new(0);
    let original = f
        .original
        .replace("127.0.0.1:9", &upstream.local_addr().unwrap().to_string());
    fs::write(&f.config, &original).unwrap();
    assert!(f.call("on").status.success());
    let before = f.ok("status");
    let lock = fs::OpenOptions::new()
        .read(true)
        .write(true)
        .open(PathBuf::from(before["state_directory"].as_str().unwrap()).join("operation.lock"))
        .unwrap();
    FileExt::lock(&lock).unwrap();
    let mut restart = f
        .command("restart")
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .unwrap();
    // The held lock prevents stop/start. Give the CLI time to reach that gate;
    // confirm it is still pending before the edit, then release explicitly.
    std::thread::sleep(std::time::Duration::from_millis(500));
    let waiting = restart.try_wait().unwrap().is_none();
    let edit = if valid_edit {
        format!("{original}\n# saved while restart waits\n")
    } else {
        "invalid = [".into()
    };
    fs::write(&f.config, &edit).unwrap();
    drop(lock);
    let result = restart.wait_with_output().unwrap();
    assert!(waiting, "restart must wait for the held operation lock");
    let after = f.ok("status");
    if valid_edit {
        assert!(
            result.status.success(),
            "{}",
            String::from_utf8_lossy(&result.stderr)
        );
        assert_eq!(after["state"], "running");
        assert_ne!(after["identity"]["nonce"], before["identity"]["nonce"]);
        assert_eq!(
            after["identity"]["fingerprint"],
            llmgw::config::ConfigFingerprint::from_bytes(edit.as_bytes()).as_str()
        );
        assert_eq!(after["pending_restart"], false);
    } else {
        assert_eq!(result.status.code(), Some(2));
        assert!(String::from_utf8_lossy(&result.stderr).contains("invalid configuration"));
        assert_eq!(after["state"], "running");
        assert_eq!(after["identity"], before["identity"]);
        assert_eq!(after["pending_restart"], true);
    }
    assert_eq!(
        upstream.accept().unwrap_err().kind(),
        std::io::ErrorKind::WouldBlock
    );
    assert!(f.call("off").status.success());
}
#[test]
fn restart_invalid_edit_while_waiting_preserves_existing_identity() {
    check_restart_edit_during_operation_wait(false);
}
#[test]
fn restart_valid_edit_while_waiting_uses_latest_fingerprint() {
    check_restart_edit_during_operation_wait(true);
}

#[cfg(unix)]
#[test]
fn restart_path_retarget_while_waiting_never_switches_instance() {
    use fs4::FileExt;
    let f = Fixture::new(0);
    let other = Fixture::new(0);
    assert!(f.call("on").status.success());
    assert!(other.call("on").status.success());
    let before = f.ok("status");
    let other_before = other.ok("status");
    let lock = fs::OpenOptions::new()
        .read(true)
        .write(true)
        .open(PathBuf::from(before["state_directory"].as_str().unwrap()).join("operation.lock"))
        .unwrap();
    FileExt::lock(&lock).unwrap();
    let mut restart = f
        .command("restart")
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .unwrap();
    std::thread::sleep(std::time::Duration::from_millis(500));
    let waiting = restart.try_wait().unwrap().is_none();
    fs::remove_file(&f.config).unwrap();
    std::os::unix::fs::symlink(&other.config, &f.config).unwrap();
    drop(lock);
    let result = restart.wait_with_output().unwrap();
    // Restore the original name before inspecting identities or fixture cleanup.
    fs::remove_file(&f.config).unwrap();
    fs::write(&f.config, &f.original).unwrap();
    assert!(waiting);
    assert_eq!(result.status.code(), Some(1));
    assert!(String::from_utf8_lossy(&result.stderr).contains("configuration_path_changed"));
    assert_eq!(f.ok("status")["identity"], before["identity"]);
    assert_eq!(other.ok("status")["identity"], other_before["identity"]);
}
