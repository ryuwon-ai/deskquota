#!/usr/bin/env python3
"""Bounded I1 release-binary regression: owned PTY, isolated HOME, loopback only."""
import fcntl
import hashlib
import json
import os
import pathlib
import pty
import re
import select
import shutil
import signal
import socket
import subprocess
import termios
import threading
import time

RESEARCH = pathlib.Path(__file__).resolve().parents[3]
OUT = pathlib.Path(__file__).resolve().parent
BIN = RESEARCH / "product/artifacts/native-final-integration-fix/llmgw-macos-arm64"
SCRATCH = OUT / "scratch"
RESULT_PATH = OUT / "direct-runtime.json"
ANSI = re.compile(rb"\x1b(?:\[[0-?]*[ -/]*[@-~]|\][^\x07]*(?:\x07|\x1b\\))")


def sha(data):
    return hashlib.sha256(data).hexdigest()


def private(path, data, mode=0o600):
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_bytes(data)
    path.chmod(mode)


def isolated(home):
    return {
        "PATH": "/usr/bin:/bin",
        "HOME": str(home),
        "USERPROFILE": str(home),
        "XDG_CONFIG_HOME": str(home / ".config"),
        "TERM": "xterm-256color",
        "LC_ALL": "C",
    }


class PTY:
    def __init__(self, config, home):
        self.master, slave = pty.openpty()
        self.buf = b""
        self.pos = 0

        def attach():
            os.setsid()
            fcntl.ioctl(slave, termios.TIOCSCTTY, 0)

        self.process = subprocess.Popen(
            [str(BIN), "setup", "--config", str(config)],
            stdin=slave,
            stdout=slave,
            stderr=slave,
            env=isolated(home),
            preexec_fn=attach,
            close_fds=True,
        )
        os.close(slave)

    def expect(self, marker, timeout=7):
        needle = marker.encode()
        end = time.monotonic() + timeout
        while True:
            index = self.buf.find(needle, self.pos)
            if index >= 0:
                self.pos = index + len(needle)
                return
            if time.monotonic() >= end:
                raise AssertionError(f"PTY timeout waiting for {marker!r}")
            if select.select([self.master], [], [], 0.1)[0]:
                try:
                    chunk = os.read(self.master, 65536)
                except OSError:
                    chunk = b""
                if not chunk:
                    raise AssertionError(f"PTY closed waiting for {marker!r}")
                self.buf += chunk

    def choose(self, marker, value=b"\r"):
        self.expect(marker)
        os.write(self.master, value)

    def finish(self, timeout=12):
        end = time.monotonic() + timeout
        while self.process.poll() is None and time.monotonic() < end:
            if select.select([self.master], [], [], 0.05)[0]:
                try:
                    self.buf += os.read(self.master, 65536)
                except OSError:
                    pass
        code = self.process.wait(timeout=1)
        while select.select([self.master], [], [], 0)[0]:
            try:
                chunk = os.read(self.master, 65536)
            except OSError:
                break
            if not chunk:
                break
            self.buf += chunk
        return code

    def close(self):
        if self.process.poll() is None:
            os.killpg(self.process.pid, signal.SIGTERM)
            try:
                self.process.wait(timeout=2)
            except subprocess.TimeoutExpired:
                os.killpg(self.process.pid, signal.SIGKILL)
                self.process.wait(timeout=2)
        os.close(self.master)


def through_save_only(run):
    for marker in [
        "Setup: Environment",
        "Environment preset",
        "Setup: Connection",
        "Upstream API base",
        "Actually supported endpoint",
        "Upstream auth",
        "Setup: Models",
        "Keep all existing models",
        "Setup: Quota",
        "RPM",
        "TPM",
        "same quota shared",
        "separate input/output",
        "Concurrency",
        "Setup: Run",
        "Local loopback port",
        "Start at next user login",
        "Setup: Tools",
        "Client intents",
        "Setup: Apply",
        "Apply",
    ]:
        run.choose(marker)


def call(home, *args, timeout=25):
    return subprocess.run(
        [str(BIN), *map(str, args)],
        capture_output=True,
        text=True,
        env=isolated(home),
        timeout=timeout,
    )


def status(home, config):
    result = call(home, "status", "--json", "--config", config)
    if result.returncode:
        raise AssertionError(f"status failed: {result.stderr}")
    return json.loads(result.stdout)


def exact_workers(config):
    lines = subprocess.run(
        ["/bin/ps", "-axo", "pid=,command="],
        capture_output=True,
        text=True,
        check=True,
    ).stdout.splitlines()
    target = str(config)
    return [line for line in lines if " worker " in line and target in line and str(BIN) in line]


def find_token(value):
    if isinstance(value, dict):
        for key, item in value.items():
            if key == "X-LLMGW-Token":
                return item
            found = find_token(item)
            if found is not None:
                return found
    if isinstance(value, list):
        for item in value:
            found = find_token(item)
            if found is not None:
                return found
    return None


def main():
    if SCRATCH.exists():
        raise SystemExit("review scratch already exists")
    SCRATCH.mkdir()
    home = SCRATCH / "home"
    home.mkdir(mode=0o700)
    config_dir = SCRATCH / "gateway"
    config_dir.mkdir(mode=0o700)
    config = config_dir / "config.toml"
    upstream = socket.socket()
    upstream.bind(("127.0.0.1", 0))
    upstream.listen()
    upstream.settimeout(0.1)
    upstream_port = upstream.getsockname()[1]
    gateway_probe = socket.socket()
    gateway_probe.bind(("127.0.0.1", 0))
    gateway_port = gateway_probe.getsockname()[1]
    gateway_probe.close()
    calls = []
    stop_listener = threading.Event()

    def listen():
        while not stop_listener.is_set():
            try:
                client, _ = upstream.accept()
            except socket.timeout:
                continue
            calls.append(True)
            client.close()

    listener_thread = threading.Thread(target=listen, daemon=True)
    listener_thread.start()
    config.write_text(
        f'''listen = "127.0.0.1:{gateway_port}"
concurrency = 1
accounting = "actual"
cancel_policy = "close"
retry_transient_429 = true

[upstream]
api_base = "http://127.0.0.1:{upstream_port}/v1"
auth = {{ mode = "none" }}

[quota]
rpm = {{ kind = "unlimited" }}
tpm = {{ kind = "unknown" }}

[[models]]
id = "example-model"
max_output_tokens = 64

[[roots]]
id = "pi-work"
endpoints = ["chat/completions"]
models = ["example-model"]
'''
    )
    config.chmod(0o600)
    setup = PTY(config, home)
    cleanup_off = None
    result = {"passed": False, "scope": "actual PTY SaveOnly -> Pi preview/hash restart/apply -> disconnect/off"}
    try:
        through_save_only(setup)
        setup_exit = setup.finish()
        if setup_exit != 0:
            raise AssertionError(f"setup exit {setup_exit}")
        stopped = status(home, config)
        state_dir = pathlib.Path(stopped["state_directory"])
        data_path = state_dir / "data-token"
        control_path = state_dir / "control-token"
        data = data_path.read_bytes()
        control = control_path.read_bytes()
        token_identity = (sha(data), sha(control))
        if len(data) != 64 or len(control) != 64 or data == control:
            raise AssertionError("invalid provisioned token shape")
        if (state_dir.stat().st_mode & 0o777) != 0o700:
            raise AssertionError("state directory is not 0700")
        if any((path.stat().st_mode & 0o777) != 0o600 for path in [data_path, control_path]):
            raise AssertionError("token file is not 0600")
        if stopped["state"] != "stopped" or exact_workers(config):
            raise AssertionError("SaveOnly started a worker")

        fake_pi = SCRATCH / "bin/pi"
        private(fake_pi, b"#!/bin/sh\nprintf '%s\\n' '0.84.2'\n", 0o700)
        client_home = SCRATCH / "client-home"
        target = client_home / ".pi/agent/models.json"
        original = b'{"providers":{"other":{"baseUrl":"https://example.invalid","models":[]}},"keep":true}\n'
        private(target, original)
        preview = call(
            home,
            "connect", "pi", "--config", config,
            "--client-executable", fake_pi,
            "--client-home", client_home,
            "--root", "pi-work", "--model", "example-model",
        )
        if preview.returncode != 0:
            raise AssertionError(f"preview failed: {preview.stderr}")
        match = re.search(r"^preview hash: ([0-9a-f]{64})$", preview.stdout, re.MULTILINE)
        if not match:
            raise AssertionError("preview hash absent")
        preview_hash = match.group(1)
        if target.read_bytes() != original:
            raise AssertionError("preview changed client bytes")
        if (sha(data_path.read_bytes()), sha(control_path.read_bytes())) != token_identity:
            raise AssertionError("preview changed token identity")

        applied = call(
            home,
            "connect", "pi", "--config", config,
            "--client-executable", fake_pi,
            "--client-home", client_home,
            "--root", "pi-work", "--model", "example-model",
            "--apply-hash", preview_hash, "--restart",
        )
        cleanup_off = True
        running = status(home, config)
        applied_document = json.loads(target.read_text())
        configured_token = find_token(applied_document)
        if applied.returncode != 0 or "client patch: applied" not in applied.stdout:
            raise AssertionError(f"apply failed: {applied.stderr}")
        if running["state"] != "running" or running["pending_restart"]:
            raise AssertionError("authenticated desired worker not running")
        if running["identity"]["fingerprint"] != sha(config.read_bytes()):
            raise AssertionError("running fingerprint differs from reviewed config")
        if configured_token != data.decode():
            raise AssertionError("client token differs from protected data token")
        if '"other"' not in target.read_text() or '"llmgw"' not in target.read_text():
            raise AssertionError("client patch did not preserve unrelated provider")
        if (sha(data_path.read_bytes()), sha(control_path.read_bytes())) != token_identity:
            raise AssertionError("activation or patch changed token identity")

        disconnected = call(home, "disconnect", "pi", "--config", config)
        restored_document = json.loads(target.read_text())
        if disconnected.returncode != 0 or restored_document != json.loads(original):
            raise AssertionError(f"disconnect failed: {disconnected.stderr}")
        off = call(home, "off", "--config", config)
        cleanup_off = None
        final_status = status(home, config)
        if off.returncode != 0 or final_status["state"] != "stopped" or exact_workers(config):
            raise AssertionError(f"off failed: {off.stderr}")
        if (sha(data_path.read_bytes()), sha(control_path.read_bytes())) != token_identity:
            raise AssertionError("disconnect/off changed token identity")
        logs = [setup.buf, preview.stdout.encode(), preview.stderr.encode(), applied.stdout.encode(), applied.stderr.encode(), disconnected.stdout.encode(), disconnected.stderr.encode(), off.stdout.encode(), off.stderr.encode()]
        if any(data in log or control in log for log in logs):
            raise AssertionError("protected token appeared in command output")
        (OUT / "setup.pty.txt").write_bytes(ANSI.sub(b"", setup.buf))
        (OUT / "connect-preview.txt").write_text(preview.stdout + preview.stderr)
        (OUT / "connect-apply.txt").write_text(applied.stdout + applied.stderr)
        (OUT / "disconnect.txt").write_text(disconnected.stdout + disconnected.stderr)
        result.update(
            passed=True,
            setup={"exit": setup_exit, "state": stopped["state"], "worker_count": 0, "tokens": {"data_bytes": 64, "control_bytes": 64, "distinct": True, "directory_mode": "0700", "file_modes": ["0600", "0600"]}},
            preview={"exit": preview.returncode, "hash_present": True, "client_bytes_unchanged": True, "tokens_unchanged": True},
            apply={"exit": applied.returncode, "reviewed_hash_reused": True, "restart_requested": True, "state": running["state"], "desired_fingerprint": True, "pending_restart": False, "client_patch_applied": True, "client_credential_matches_setup_token": True, "unrelated_client_value_preserved": True},
            cleanup={"disconnect_exit": disconnected.returncode, "client_user_values_restored": True, "owned_llmgw_provider_removed": find_token(restored_document) is None, "off_exit": off.returncode, "final_state": final_status["state"], "worker_count": 0, "tokens_unchanged": True},
            security={"protected_token_in_outputs": False},
            upstream_calls=len(calls),
            client_execution_scope="actual llmgw CLI with a synthetic Pi --version executable; installed Pi was not executed",
        )
    finally:
        if cleanup_off:
            cleanup = call(home, "off", "--config", config)
            if cleanup.returncode:
                result["emergency_cleanup_error"] = cleanup.stderr
        setup.close()
        stop_listener.set()
        listener_thread.join(timeout=2)
        upstream.close()
        result["all_pty_waited"] = setup.process.poll() is not None
        result["owned_workers_remaining"] = len(exact_workers(config))
        RESULT_PATH.write_text(json.dumps(result, indent=2) + "\n")
        if result["owned_workers_remaining"] == 0:
            shutil.rmtree(SCRATCH)
        result["scratch_removed"] = not SCRATCH.exists()
        RESULT_PATH.write_text(json.dumps(result, indent=2) + "\n")
    if not result["passed"]:
        raise SystemExit("I1 direct regression failed")


if __name__ == "__main__":
    main()
