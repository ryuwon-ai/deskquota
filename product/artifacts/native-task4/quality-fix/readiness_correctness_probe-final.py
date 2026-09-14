#!/usr/bin/env python3
"""Owned PTY regression for readiness changing during client confirmation."""
from __future__ import annotations

import argparse
import hashlib
import importlib.util
import json
import os
import pathlib
import select
import shutil
import subprocess
import sys
import tempfile
import time

sys.dont_write_bytecode = True
RESEARCH = pathlib.Path(__file__).resolve().parents[4]
SETUP_DRIVER = RESEARCH / "product/scripts/probe_native_setup.py"
spec = importlib.util.spec_from_file_location("owned_setup_pty", SETUP_DRIVER)
setup = importlib.util.module_from_spec(spec)
assert spec.loader is not None
spec.loader.exec_module(setup)


def finish_any(run: setup.PtyRun) -> tuple[int, str]:
    deadline = time.monotonic() + 12
    while run.process.poll() is None and time.monotonic() < deadline:
        ready, _, _ = select.select([run.master], [], [], 0.1)
        if ready:
            try:
                run.buffer += os.read(run.master, 4096)
            except OSError:
                pass
    code = run.process.wait(timeout=1)
    text = run.buffer.decode("utf-8", errors="replace")
    run.close()
    return code, text


def drive(
    binary: pathlib.Path,
    config: pathlib.Path,
    port: int,
    native: pathlib.Path,
    environment: dict[str, str],
    runs: list[setup.PtyRun],
    result: dict[str, object],
    stop_during_confirmation: bool,
) -> tuple[int, str]:
    class ReviewPty(setup.PtyRun):
        def send(self, value: bytes) -> None:
            if (
                value == b"y"
                and b"Apply the exact pi client preview with hash" in self.buffer
                and not result.get("confirmation_observed")
            ):
                before = json.loads(cli(binary, config, environment, "status", "--json").stdout)
                assert before["state"] == "running"
                result["confirmation_observed"] = True
                result["reviewed_runtime_fingerprint"] = before["identity"]["fingerprint"]
                result["config_sha256_before_confirmation"] = hashlib.sha256(
                    config.read_bytes()
                ).hexdigest()
                result["client_existed_before_confirmation"] = (native / "models.json").exists()
                if stop_during_confirmation:
                    off = cli(binary, config, environment, "off")
                    assert off.returncode == 0
                    stopped = json.loads(
                        cli(binary, config, environment, "status", "--json").stdout
                    )
                    assert stopped["state"] == "stopped"
                    result["state_after_off_before_yes"] = "stopped"
            super().send(value)

    run = ReviewPty(binary, config, runs, environment)
    choose = setup.choose
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
    choose(run, "Client intents", b" \r")
    choose(run, "Setup: Apply")
    run.expect("fingerprint impact:")
    run.expect("Apply")
    run.expect("Cancel")
    run.wait_raw()
    run.send(b"\x1b[A")
    run.expect("❯ Save and start")
    run.send(b"\r")
    run.expect("Native config directory for pi")
    run.send(b"\r")
    run.expect("preview hash:")
    run.expect("Apply the exact pi client preview with hash")
    run.send(b"y")
    return finish_any(run)


def cli(
    binary: pathlib.Path,
    config: pathlib.Path,
    environment: dict[str, str],
    *args: str,
) -> subprocess.CompletedProcess[str]:
    return subprocess.run(
        [str(binary), *args, "--config", str(config)],
        cwd=config.parent,
        env=environment,
        capture_output=True,
        text=True,
        timeout=15,
    )


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--binary", type=pathlib.Path, required=True)
    parser.add_argument("--output", type=pathlib.Path, required=True)
    parser.add_argument("--leave-running", action="store_true")
    args = parser.parse_args()
    binary = args.binary.resolve()
    scratch = pathlib.Path(tempfile.mkdtemp(prefix="llmgw-quality-fix-ready-"))
    config = scratch / "gateway.toml"
    home = scratch / "home"
    home.mkdir(mode=0o700)
    native = home / ".pi/agent"
    native.mkdir(parents=True, mode=0o700)
    bindir = scratch / "bin"
    bindir.mkdir()
    pi = bindir / "pi"
    pi.write_text('#!/bin/sh\n[ "$1" = "--version" ] || exit 99\nprintf "0.84.2\\n"\n')
    pi.chmod(0o700)
    environment = {
        "PATH": f"{bindir}:/usr/bin:/bin",
        "HOME": str(home),
        "USERPROFILE": str(home),
        "PI_CODING_AGENT_DIR": str(native),
        "XDG_CONFIG_HOME": str(home / "config"),
        "XDG_CACHE_HOME": str(home / "cache"),
        "XDG_DATA_HOME": str(home / "data"),
        "TMPDIR": str(scratch),
        "LANG": "en_US.UTF-8",
        "LC_ALL": "en_US.UTF-8",
        "TERM": "xterm-256color",
        "NO_COLOR": "1",
    }
    runs: list[setup.PtyRun] = []
    result: dict[str, object] = {
        "binary_sha256": hashlib.sha256(binary.read_bytes()).hexdigest(),
        "environment_keys": sorted(environment),
        "synthetic_version_only_client": True,
        "upstream_inference_requested": False,
        "stop_during_confirmation": not args.leave_running,
    }
    try:
        code, log = drive(
            binary,
            config,
            setup.free_port(),
            native,
            environment,
            runs,
            result,
            not args.leave_running,
        )
        status = json.loads(cli(binary, config, environment, "status", "--json").stdout)
        client_written = (native / "models.json").exists()
        desired = result["reviewed_runtime_fingerprint"]
        ready = (
            status["state"] == "running"
            and status.get("identity", {}).get("fingerprint") == desired
        )
        result.update(
            setup_exit=code,
            worker_state_after_confirmation=status["state"],
            client_config_written=client_written,
            client_journal_state=status["clients"]["pi"]["state"],
            reported_client_applied="client patch: applied" in log,
            desired_fingerprint_ready_before_result=ready,
            config_sha256_after_result=hashlib.sha256(config.read_bytes()).hexdigest(),
        )
        result["safe_result"] = not client_written or ready
        assert result["config_sha256_before_confirmation"] == result["config_sha256_after_result"]
        assert result["safe_result"], "client config was written after reviewed readiness became stale"
        if not client_written:
            assert result["client_journal_state"] != "applied"
    except Exception as error:
        result["driver_error_type"] = type(error).__name__
        result["driver_error_message"] = str(error)[:300]
        raise
    finally:
        for run in runs:
            run.close()
        if config.exists():
            off = cli(binary, config, environment, "off")
            assert off.returncode == 0
        shutil.rmtree(scratch)
        result["owned_temporary_paths_remaining"] = []
        args.output.write_text(json.dumps(result, indent=2, sort_keys=True) + "\n")
        print(json.dumps(result, indent=2, sort_keys=True))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
