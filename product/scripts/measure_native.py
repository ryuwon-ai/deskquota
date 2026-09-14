#!/usr/bin/env python3
"""Measure one isolated macOS wizard and repeated native on/off cycles."""

from __future__ import annotations

import argparse
import hashlib
import json
import os
from pathlib import Path
import platform
import re
import shlex
import shutil
import socket
import subprocess
import sys
import tempfile
import threading
import time
import tomllib
from urllib.parse import urlsplit

SCRIPT_DIR = Path(__file__).resolve().parent
if str(SCRIPT_DIR) not in sys.path:
    sys.path.insert(0, str(SCRIPT_DIR))
import probe_native_setup


def safe_environment(home: Path, _source: dict[str, str] | None = None) -> dict[str, str]:
    home = home.resolve()
    temporary = home / "tmp"
    config_home = home / "xdg"
    temporary.mkdir(parents=True, exist_ok=True)
    config_home.mkdir(parents=True, exist_ok=True)
    return {
        "HOME": str(home),
        "USERPROFILE": str(home),
        "XDG_CONFIG_HOME": str(config_home),
        "TMPDIR": str(temporary),
        "LC_ALL": "C",
        "PATH": "/usr/bin:/bin:/usr/sbin:/sbin",
    }


def parse_macos_peak(output: str) -> int:
    matches = re.findall(r"^\s*(\d+)\s+maximum resident set size\s*$", output, re.MULTILINE)
    if len(matches) != 1:
        raise ValueError("expected one macOS maximum resident set size")
    return int(matches[0])


def macos_peak_line(output: str) -> str:
    matches = [line for line in output.splitlines() if "maximum resident set size" in line]
    if len(matches) != 1:
        raise ValueError("expected one macOS maximum resident set size line")
    return matches[0]


def free_port() -> int:
    with socket.socket() as listener:
        listener.bind(("127.0.0.1", 0))
        return int(listener.getsockname()[1])


def saved_upstream_summary(text: str) -> dict[str, str]:
    upstream = tomllib.loads(text)["upstream"]
    api_base = upstream["api_base"]
    parsed = urlsplit(api_base)
    if parsed.hostname not in {"127.0.0.1", "::1", "localhost"}:
        raise ValueError("measurement config must use an owned loopback upstream")
    auth = upstream["auth"]["mode"]
    if auth != "none":
        raise ValueError("measurement config must not read an environment credential")
    return {"api_base": api_base, "auth": auth}


def run(binary: Path, config: Path, env: dict[str, str], *arguments: str) -> tuple[subprocess.CompletedProcess[str], int]:
    started = time.monotonic_ns()
    completed = subprocess.run(
        [str(binary), *arguments, "--config", str(config)],
        env=env,
        stdin=subprocess.DEVNULL,
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
        text=True,
        timeout=20,
        check=False,
    )
    return completed, time.monotonic_ns() - started


def process_rss_bytes(pid: int, env: dict[str, str]) -> int:
    completed = subprocess.run(
        ["/bin/ps", "-o", "rss=", "-p", str(pid)],
        env=env,
        capture_output=True,
        text=True,
        timeout=2,
        check=False,
    )
    if completed.returncode != 0 or not completed.stdout.strip():
        raise RuntimeError(f"ps_failed_for_pid:{pid}")
    return int(completed.stdout.strip()) * 1024


def sample_owned_popen(
    stop: threading.Event,
    registry: list[probe_native_setup.PtyRun],
    env: dict[str, str],
    samples: list[dict[str, int]],
) -> None:
    started = time.monotonic_ns()
    while not stop.wait(0.02):
        if not registry:
            continue
        process = registry[-1].process
        if process.poll() is not None:
            continue
        try:
            rss = process_rss_bytes(process.pid, env)
        except RuntimeError:
            continue
        samples.append(
            {
                "elapsed_nanoseconds": time.monotonic_ns() - started,
                "pid": process.pid,
                "rss_bytes": rss,
            }
        )


def drive_current_wizard(
    run: probe_native_setup.PtyRun,
    port: int,
) -> str:
    choose = probe_native_setup.choose
    choose(run, "Setup: Environment")
    choose(run, "Environment preset")
    choose(run, "Setup: Connection")
    choose(run, "Upstream API base")
    choose(run, "Actually supported endpoint", b" \r")
    choose(run, "Upstream auth")
    choose(run, "Setup: Models")
    choose(run, "Try one bounded GET")
    choose(run, "Manual model ID", "fixture-model\r".encode())
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
    choose(run, "Start at next user login")
    choose(run, "Setup: Tools")
    choose(run, "Client intents")
    choose(run, "Setup: Apply")
    run.expect("fingerprint impact:")
    run.expect("Apply")
    run.send(b"\r")
    return run.finish(0)


def complete_current_wizard(
    executable: Path,
    config: Path,
    port: int,
    registry: list[probe_native_setup.PtyRun],
    environment: dict[str, str],
) -> str:
    run = probe_native_setup.PtyRun(executable, config, registry, environment)
    return drive_current_wizard(run, port)


def measure(binary: Path, cycles: int) -> dict[str, object]:
    if sys.platform != "darwin":
        raise RuntimeError("macOS_measurement_only")
    root = Path(tempfile.mkdtemp(prefix="llmgw-native-task6-measure-한글-space-"))
    home = root / "home"
    home.mkdir(mode=0o700)
    env = safe_environment(home)
    config = root / "config with space" / "gateway 설정.toml"
    config.parent.mkdir()
    timed_config = root / "timed config" / "gateway 설정.toml"
    timed_config.parent.mkdir()
    runs: list[probe_native_setup.PtyRun] = []
    cleanup_confirmed = False
    on_attempted = False
    original_error: BaseException | None = None
    result: dict[str, object] | None = None
    try:
        wizard_samples: list[dict[str, int]] = []
        stop_sampler = threading.Event()
        sampler = threading.Thread(
            target=sample_owned_popen,
            args=(stop_sampler, runs, env, wizard_samples),
            daemon=True,
        )
        direct_run = probe_native_setup.PtyRun(binary, config, runs, env)
        wizard_samples.append(
            {"elapsed_nanoseconds": 0, "pid": direct_run.process.pid, "rss_bytes": process_rss_bytes(direct_run.process.pid, env)}
        )
        sampler.start()
        try:
            drive_current_wizard(direct_run, free_port())
        finally:
            stop_sampler.set()
            sampler.join(timeout=3)

        wrapper = root / "timed-llmgw"
        wrapper.write_text(
            "#!/bin/sh\nexec /usr/bin/time -l " + shlex.quote(str(binary)) + ' "$@"\n',
            encoding="utf-8",
        )
        wrapper.chmod(0o700)
        wizard_log = complete_current_wizard(wrapper, timed_config, free_port(), runs, env)
        command_tree_peak = parse_macos_peak(wizard_log)
        upstream = saved_upstream_summary(config.read_text(encoding="utf-8"))
        if not wizard_samples:
            raise RuntimeError("no_direct_wizard_pid_rss_samples")

        on_raw: list[int] = []
        off_raw: list[int] = []
        idle_samples: list[int] = []
        for index in range(cycles):
            on_attempted = True
            on, elapsed = run(binary, config, env, "on")
            if on.returncode != 0:
                raise RuntimeError(f"on_failed_cycle_{index}:{on.stderr}")
            on_raw.append(elapsed)
            status, _ = run(binary, config, env, "status", "--json")
            if status.returncode != 0:
                raise RuntimeError(f"status_failed_cycle_{index}:{status.stderr}")
            state = json.loads(status.stdout)
            if state.get("state") != "running":
                raise RuntimeError(f"worker_not_running_cycle_{index}")
            if index == 0:
                pid = int(state["identity"]["pid"])
                for _ in range(10):
                    idle_samples.append(process_rss_bytes(pid, env))
                    time.sleep(0.1)
            off, elapsed = run(binary, config, env, "off")
            if off.returncode != 0:
                raise RuntimeError(f"off_failed_cycle_{index}:{off.stderr}")
            off_raw.append(elapsed)
            on_attempted = False
        final_status, _ = run(binary, config, env, "status", "--json")
        cleanup_confirmed = (
            final_status.returncode == 0 and json.loads(final_status.stdout).get("state") == "stopped"
        )
        if not cleanup_confirmed:
            raise RuntimeError("final_authenticated_stopped_status_unconfirmed")
        result = {
            "binary": str(binary),
            "binary_sha256": hashlib.sha256(binary.read_bytes()).hexdigest(),
            "host": {
                "platform": platform.platform(),
                "machine": platform.machine(),
                "development_host": "Apple M4 32 GiB; not a low-end or clean OS account",
            },
            "environment_keys": sorted(env),
            "wizard": {
                "client_selection": "manual",
                "client_version_query_children": 0,
                "invocations": {
                    "direct_pid_sampled": {
                        "completed_save_only": True,
                        "sample_interval_ms": 20,
                        "sample_count": len(wizard_samples),
                        "raw_pid_samples": wizard_samples,
                        "sampled_pid_max_rss_bytes": max(
                            (row["rss_bytes"] for row in wizard_samples), default=None
                        ),
                        "scope": "exact direct llmgw setup PID from the owned PTY process",
                        "limit": "sample maximum; peaks between observations may be missed",
                    },
                    "timed_command_tree": {
                        "completed_save_only": True,
                        "os_command_tree_peak_rss_bytes": command_tree_peak,
                        "raw_peak_line": macos_peak_line(wizard_log),
                        "scope": "macOS wait4 value for the timed command and its descendants",
                    },
                },
                "invocation_count": 2,
                "reason": "one direct invocation permits exact PID sampling; a separate timed invocation records the OS command-tree peak",
            },
            "idle_worker": {
                "sample_interval_ms": 100,
                "samples_bytes": idle_samples,
                "sampled_max_rss_bytes": max(idle_samples),
                "scope": "owned worker PID only",
            },
            "lifecycle": {
                "cycles": cycles,
                "on_raw_nanoseconds": on_raw,
                "off_raw_nanoseconds": off_raw,
                "final_authenticated_state": "stopped",
            },
            "network": {
                "upstream_attempts_observed": "not instrumented",
                "saved_upstream": upstream,
                "wizard_model_listing_selected": False,
                "wizard_inference_selected": False,
            },
            "cleanup": {},
            "limitations": [
                "macOS development host observation only",
                "no clean OS account, low-end PC, Windows, Linux, login, signing, or quarantine proof",
                "raw samples are descriptive and are not percentiles or superiority evidence",
            ],
        }
    except BaseException as error:
        original_error = error

    cleanup_error: BaseException | None = None
    try:
        for pty_run in runs:
            pty_run.close()
        if on_attempted:
            off, _ = run(binary, config, env, "off")
            status, _ = run(binary, config, env, "status", "--json")
            cleanup_confirmed = off.returncode == 0 and status.returncode == 0 and json.loads(status.stdout).get("state") == "stopped"
        else:
            cleanup_confirmed = True
        if cleanup_confirmed:
            shutil.rmtree(root)
        else:
            cleanup_error = RuntimeError(f"cleanup_unconfirmed_recovery_retained:{root}")
    except BaseException as error:
        cleanup_error = error

    if original_error is not None:
        if cleanup_error is not None:
            raise RuntimeError(
                f"measurement_failed:{original_error}; cleanup_failed:{cleanup_error}; recovery_retained:{root}"
            ) from original_error
        raise original_error
    if cleanup_error is not None:
        raise cleanup_error
    assert result is not None
    result["cleanup"] = {
        "authenticated_off_and_stopped_confirmed": True,
        "temporary_home_removed": True,
    }
    return result


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--binary", type=Path, required=True)
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument("--cycles", type=int, default=5)
    args = parser.parse_args()
    if args.cycles < 1 or args.cycles > 20:
        parser.error("--cycles must be between 1 and 20")
    if args.output.exists():
        parser.error("--output already exists")
    binary = args.binary.resolve(strict=True)
    result = measure(binary, args.cycles)
    args.output.parent.mkdir(parents=True, exist_ok=True)
    with args.output.open("x", encoding="utf-8") as stream:
        json.dump(result, stream, indent=2)
        stream.write("\n")
    print(json.dumps({"passed": True, "binary_sha256": result["binary_sha256"], "output": str(args.output)}))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
