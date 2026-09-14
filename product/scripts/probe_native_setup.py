#!/usr/bin/env python3
"""Owned macOS PTY smoke for the first-run wizard. Uses no real upstream."""
from __future__ import annotations

import argparse
import fcntl
import json
import os
import pty
import re
import select
import signal
import shutil
import socket
import subprocess
import tempfile
import termios
import time
from pathlib import Path


def free_port() -> int:
    with socket.socket() as sock:
        sock.bind(("127.0.0.1", 0))
        return int(sock.getsockname()[1])


class PtyRun:
    def __init__(
        self,
        binary: Path,
        config: Path,
        registry: list[PtyRun],
        environment: dict[str, str] | None = None,
    ):
        master, slave = pty.openpty()
        self.master = master
        self.buffer = b""
        self.cursor = 0
        self.closed = False

        def attach_controlling_terminal() -> None:
            os.setsid()
            fcntl.ioctl(slave, termios.TIOCSCTTY, 0)

        self.process = subprocess.Popen(
            [str(binary), "setup", "--config", str(config)],
            stdin=slave,
            stdout=slave,
            stderr=slave,
            close_fds=True,
            preexec_fn=attach_controlling_terminal,
            env=environment,
        )
        os.close(slave)
        registry.append(self)

    def expect(self, marker: str, timeout: float = 5.0) -> None:
        wanted = marker.encode()
        deadline = time.monotonic() + timeout
        while True:
            found = self.buffer.find(wanted, self.cursor)
            if found >= 0:
                self.cursor = found + len(wanted)
                return
            remaining = deadline - time.monotonic()
            if remaining <= 0:
                raise AssertionError(f"PTY timeout waiting for {marker!r}: {self.buffer[-2000:]!r}")
            ready, _, _ = select.select([self.master], [], [], remaining)
            if not ready:
                continue
            try:
                chunk = os.read(self.master, 4096)
            except OSError:
                chunk = b""
            if not chunk:
                raise AssertionError(f"PTY closed waiting for {marker!r}: {self.buffer[-2000:]!r}")
            self.buffer += chunk

    def send(self, value: bytes) -> None:
        os.write(self.master, value)

    def wait_raw(self, timeout: float = 2.0) -> None:
        deadline = time.monotonic() + timeout
        while termios.tcgetattr(self.master)[3] & termios.ICANON:
            if time.monotonic() >= deadline:
                raise AssertionError("PTY did not enter raw input mode")
            time.sleep(0.01)

    def finish(self, expected: int) -> str:
        deadline = time.monotonic() + 10
        while self.process.poll() is None and time.monotonic() < deadline:
            ready, _, _ = select.select([self.master], [], [], 0.1)
            if ready:
                try:
                    self.buffer += os.read(self.master, 4096)
                except OSError:
                    pass
        code = self.process.wait(timeout=1)
        self.close()
        if code != expected:
            raise AssertionError(f"expected exit {expected}, got {code}: {self.buffer[-4000:]!r}")
        return self.buffer.decode("utf-8", errors="replace")

    def close(self) -> None:
        if self.closed:
            return
        if self.process.poll() is None:
            try:
                os.killpg(self.process.pid, signal.SIGTERM)
                self.process.wait(timeout=2)
            except (ProcessLookupError, subprocess.TimeoutExpired):
                if self.process.poll() is None:
                    try:
                        os.killpg(self.process.pid, signal.SIGKILL)
                    except ProcessLookupError:
                        pass
                    self.process.wait(timeout=2)
        os.close(self.master)
        self.closed = True


def choose(run: PtyRun, marker: str, keys: bytes = b"\r") -> None:
    run.expect(marker)
    run.send(keys)


def complete(
    binary: Path,
    config: Path,
    port: int,
    start: bool,
    registry: list[PtyRun],
    client_config_dir: Path | None = None,
    client_environment: dict[str, str] | None = None,
) -> str:
    run = PtyRun(binary, config, registry, client_environment)
    choose(run, "Setup: Environment")
    choose(run, "Environment preset")
    choose(run, "Setup: Connection")
    choose(run, "Upstream API base")
    choose(run, "Actually supported endpoint", b" \r")
    choose(run, "Upstream auth")
    choose(run, "Setup: Models")
    choose(run, "Try one bounded GET")
    choose(run, "Manual model ID", "모델 with space\r".encode())
    choose(run, "Set a user-chosen reservation fallback", b"y")
    choose(run, "Nonzero reservation fallback")
    choose(run, "Setup: Quota")
    choose(run, "RPM")
    choose(run, "TPM")
    choose(run, "same quota shared")
    choose(run, "separate input/output")
    choose(run, "Concurrency")
    choose(run, "Setup: Run")
    choose(run, "Local loopback port", f"{port}\r".encode())
    choose(run, "Request start at next login")
    choose(run, "Setup: Tools")
    choose(run, "Client intents", b" \r" if client_config_dir is not None else b"\r")
    choose(run, "Setup: Apply")
    run.expect("fingerprint impact:")
    run.expect("Apply")
    run.expect("Cancel")
    run.wait_raw()
    if start:
        run.send(b"\x1b[A")
        run.expect("❯ Save and start")
        run.send(b"\r")
    else:
        run.send(b"\r")
    if client_config_dir is not None:
        run.expect("Native config directory for pi")
        run.send(b"\r")
        run.expect("preview hash:")
        run.expect("Apply the exact pi client preview with hash")
        hashes = re.findall(rb"preview hash: ([0-9a-f]{64})", run.buffer)
        if not hashes:
            raise AssertionError("selected Pi setup did not print a preview hash")
        run.send(b"y")
    return run.finish(0)


def stop_owned_configs(binary: Path, configs: list[Path], strict: bool) -> None:
    failures: list[str] = []
    for config in configs:
        if not config.exists():
            continue
        try:
            off = subprocess.run(
                [binary, "off", "--config", config],
                capture_output=True,
                timeout=12,
            )
            status = subprocess.run(
                [binary, "status", "--json", "--config", config],
                capture_output=True,
                text=True,
                timeout=5,
            )
            state = json.loads(status.stdout).get("state") if status.returncode == 0 else None
            if off.returncode != 0 or state != "stopped":
                failures.append(f"{config}: off={off.returncode}, state={state}")
        except (subprocess.SubprocessError, json.JSONDecodeError) as error:
            failures.append(f"{config}: {error}")
    if strict and failures:
        raise AssertionError("owned worker cleanup failed: " + "; ".join(failures))


def cleanup_then_remove(binary: Path, runs: list[PtyRun], configs: list[Path], root: Path) -> None:
    for run in runs:
        run.close()
    stop_owned_configs(binary, configs, strict=True)
    shutil.rmtree(root)


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--binary", type=Path, required=True)
    parser.add_argument("--output", type=Path, required=True)
    args = parser.parse_args()
    root = Path(tempfile.mkdtemp(prefix="llmgw-native-setup-한글 space-"))
    evidence: dict[str, object] = {"binary": str(args.binary.resolve()), "cases": {}}
    runs: list[PtyRun] = []
    possible_configs: list[Path] = []
    try:
        cancel_config = root / "cancel 경로" / "gateway 설정.toml"
        possible_configs.append(cancel_config)
        cancel_config.parent.mkdir()
        run = PtyRun(args.binary, cancel_config, runs)
        run.expect("Setup: Environment")
        run.send(b"\x1b")
        cancel_log = run.finish(130)
        if cancel_config.exists() or list(cancel_config.parent.iterdir()):
            raise AssertionError("cancel created a file")
        evidence["cases"]["cancel"] = {"exit": 130, "files": 0, "log_tail": cancel_log[-800:]}

        back_config = root / "back 경로" / "gateway 설정.toml"
        possible_configs.append(back_config)
        back_config.parent.mkdir()
        run = PtyRun(args.binary, back_config, runs)
        choose(run, "Setup: Environment")
        choose(run, "Environment preset")
        choose(run, "Setup: Connection", b"\x1b[B\r")
        run.expect("Setup: Environment")
        run.send(b"\x1b")
        back_log = run.finish(130)
        if back_log.count("Setup: Environment") < 2:
            raise AssertionError("Back did not produce a fresh Environment screen")
        if back_config.exists() or list(back_config.parent.iterdir()):
            raise AssertionError("back/cancel created a file")
        evidence["cases"]["back_then_cancel"] = {"exit": 130, "returned_to_environment": True, "files": 0, "log_tail": back_log[-1000:]}

        ctrl_config = root / "ctrl-c 경로" / "gateway 설정.toml"
        possible_configs.append(ctrl_config)
        ctrl_config.parent.mkdir()
        run = PtyRun(args.binary, ctrl_config, runs)
        choose(run, "Setup: Environment")
        choose(run, "Environment preset")
        choose(run, "Setup: Connection")
        run.expect("Upstream API base")
        run.send(b"\x03")
        ctrl_log = run.finish(-signal.SIGINT)
        if ctrl_config.exists() or list(ctrl_config.parent.iterdir()):
            raise AssertionError("Ctrl-C created a file")
        evidence["cases"]["input_ctrl_c"] = {"subprocess_signal": "SIGINT", "shell_equivalent_exit": 130, "files": 0, "log_tail": ctrl_log[-800:]}

        save_config = root / "저장 only space" / "gateway 설정.toml"
        possible_configs.append(save_config)
        save_config.parent.mkdir()
        save_log = complete(args.binary, save_config, free_port(), False, runs)
        status = subprocess.run([args.binary, "status", "--json", "--config", save_config], check=True, capture_output=True, text=True)
        state = json.loads(status.stdout)
        if state["state"] != "stopped":
            raise AssertionError(state)
        model_unicode = "모델 with space" in save_config.read_text()
        if not model_unicode:
            raise AssertionError("Unicode model input was not preserved")
        evidence["cases"]["save_only"] = {"exit": 0, "state": "stopped", "model_unicode": True, "log_tail": save_log[-1200:]}

        start_config = root / "저장 and start space" / "gateway 설정.toml"
        possible_configs.append(start_config)
        start_config.parent.mkdir()
        start_log = complete(args.binary, start_config, free_port(), True, runs)
        status = subprocess.run([args.binary, "status", "--json", "--config", start_config], check=True, capture_output=True, text=True)
        state = json.loads(status.stdout)
        if state["state"] != "running" or not state.get("identity", {}).get("fingerprint"):
            raise AssertionError({"state": state, "log_tail": start_log[-4000:]})
        subprocess.run([args.binary, "off", "--config", start_config], check=True, capture_output=True)
        evidence["cases"]["save_and_start"] = {"exit": 0, "state": "running", "authenticated_identity": True, "cleaned": True, "log_tail": start_log[-1200:]}

        client_config = root / "selected pi" / "gateway 설정.toml"
        possible_configs.append(client_config)
        client_config.parent.mkdir()
        client_home = root / "selected pi home"
        client_home.mkdir(mode=0o700)
        client_config_dir = root / "selected pi native override"
        client_config_dir.mkdir(mode=0o700)
        client_environment = {
            "PATH": os.environ.get("PATH", "/opt/homebrew/bin:/usr/local/bin:/usr/bin:/bin"),
            "LANG": "en_US.UTF-8",
            "LC_ALL": "en_US.UTF-8",
            "HOME": str(client_home),
            "USERPROFILE": str(client_home),
            "PI_CODING_AGENT_DIR": str(client_config_dir),
            "NO_COLOR": "1",
        }
        client_log = complete(
            args.binary,
            client_config,
            free_port(),
            True,
            runs,
            client_config_dir=client_config_dir,
            client_environment=client_environment,
        )
        status = subprocess.run(
            [args.binary, "status", "--json", "--config", client_config],
            check=True,
            capture_output=True,
            text=True,
        )
        state = json.loads(status.stdout)
        models = client_config_dir / "models.json"
        journal_state = state.get("clients", {}).get("pi", {}).get("state")
        if state["state"] != "running" or journal_state != "applied" or not models.is_file():
            raise AssertionError({"state": state, "models": models.is_file(), "log_tail": client_log[-4000:]})
        if "llmgw" not in models.read_text(encoding="utf-8"):
            raise AssertionError("selected Pi setup did not write the reviewed managed provider")
        disconnected = subprocess.run(
            [args.binary, "disconnect", "pi", "--config", client_config],
            check=True,
            capture_output=True,
            text=True,
        )
        subprocess.run([args.binary, "off", "--config", client_config], check=True, capture_output=True)
        readiness_index = client_log.find("authenticated readiness: true")
        patch_index = client_log.find("client patch: applied")
        if readiness_index < 0 or patch_index < 0 or readiness_index >= patch_index:
            raise AssertionError("selected client patch did not follow authenticated gateway readiness")
        if str(client_config_dir) not in client_log:
            raise AssertionError("selected client setup did not honor PI_CODING_AGENT_DIR as its default")
        evidence["cases"]["selected_pi_save_and_start"] = {
            "exit": 0,
            "gateway_ready_before_client_apply": True,
            "separate_client_preview": "Apply the exact pi client preview with hash" in client_log,
            "native_environment_default_honored": str(client_config_dir) in client_log,
            "client_journal_applied": journal_state == "applied",
            "disconnect_succeeded": disconnected.returncode == 0,
            "cleaned": True,
        }

        failure_config = root / "intentional failure cleanup" / "gateway 설정.toml"
        possible_configs.append(failure_config)
        failure_config.parent.mkdir()
        try:
            complete(args.binary, failure_config, free_port(), True, runs)
            raise AssertionError("synthetic harness failure after authenticated start")
        except AssertionError as error:
            if str(error) != "synthetic harness failure after authenticated start":
                raise
        finally:
            stop_owned_configs(args.binary, [failure_config], strict=True)
        evidence["cases"]["intentional_failure_cleanup"] = {
            "injected_after_start": True,
            "authenticated_off": True,
            "confirmed_stopped": True,
        }

        preserve_root = Path(tempfile.mkdtemp(prefix="llmgw-cleanup-failure-preserve-"))
        preserve_config = preserve_root / "gateway.toml"
        preserve_config.write_text("synthetic cleanup failure fixture\n")
        failing_binary = preserve_root / "fail-off.sh"
        failing_binary.write_text("#!/bin/sh\nexit 1\n")
        failing_binary.chmod(0o700)
        try:
            cleanup_then_remove(failing_binary, [], [preserve_config], preserve_root)
            raise AssertionError("cleanup failure did not stop temp removal")
        except AssertionError as error:
            if not str(error).startswith("owned worker cleanup failed:"):
                raise
            if not preserve_root.exists() or not preserve_config.exists():
                raise AssertionError("cleanup failure did not preserve recovery state")
        finally:
            shutil.rmtree(preserve_root, ignore_errors=True)
        evidence["cases"]["cleanup_failure_preserves_state"] = {
            "injected_off_failure": True,
            "temp_and_control_state_preserved": True,
        }
        evidence["result"] = "pass"
        args.output.parent.mkdir(parents=True, exist_ok=True)
        args.output.write_text(json.dumps(evidence, ensure_ascii=False, indent=2) + "\n")
        return 0
    finally:
        cleanup_then_remove(args.binary, runs, possible_configs, root)


if __name__ == "__main__":
    raise SystemExit(main())
