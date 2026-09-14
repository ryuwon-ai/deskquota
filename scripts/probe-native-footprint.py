#!/usr/bin/env python3
"""Bounded macOS native CLI/idle observation; no data or provider requests."""
import argparse
from datetime import datetime, timezone
import hashlib
import json
import os
from pathlib import Path
import platform
import shutil
import statistics
import subprocess
import tempfile
import time


def digest(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--binary", type=Path, required=True)
    parser.add_argument("--baseline-binary", type=Path, required=True)
    parser.add_argument("--output", type=Path, required=True)
    args = parser.parse_args()
    if platform.system() != "Darwin":
        raise SystemExit("This observation currently requires macOS ps RSS units.")
    binary = args.binary.resolve(strict=True)
    baseline = args.baseline_binary.resolve(strict=True)
    before = digest(binary)
    result = {
        "started_at": datetime.now(timezone.utc).isoformat(),
        "probe_sha256": digest(Path(__file__).resolve()),
        "platform": platform.platform(), "machine": platform.machine(),
        "binary": str(binary), "binary_sha256": before,
        "binary_bytes": binary.stat().st_size,
        "baseline_binary_sha256": digest(baseline),
        "baseline_binary_bytes": baseline.stat().st_size,
        "data_ingress_requests": 0, "upstream_requests": 0,
        "runs": [], "passed": False,
        "limitations": [
            "Three sequential cold CLI starts; not latency percentiles or performance superiority",
            "No inference workload; empty ledger only; known RPM includes startup hold",
            "RSS is sampled process resident size, not peak allocation or aggregate CLI footprint",
            "Only current macOS arm64 host; no Linux, Windows or low-end runtime evidence",
            "Baseline binary is used for file size only, not a lifecycle timing comparison",
        ],
    }
    try:
        for index in range(3):
            temporary = Path(tempfile.mkdtemp(prefix="llmgw-native-footprint-"))
            config = temporary / "fixture.toml"
            config.write_text('''listen = "127.0.0.1:0"
concurrency = 1
[upstream]
api_base = "http://127.0.0.1:9/v1"
[upstream.auth]
mode = "none"
[quota.rpm]
kind = "known"
value = 60
[quota.tpm]
kind = "unknown"
[[models]]
id = "fixture"
max_output_tokens = 32
[[roots]]
id = "fixture"
endpoints = ["models", "chat/completions"]
models = ["fixture"]
''')
            row = {"index": index, "temporary_path": str(temporary),
                   "cleanup_confirmed": False}
            result["runs"].append(row)

            def call(*parts):
                started = time.perf_counter_ns()
                process = subprocess.run(
                    [str(binary), "--config", str(config), *parts],
                    capture_output=True, timeout=20,
                )
                elapsed = (time.perf_counter_ns() - started) / 1_000_000
                if process.returncode != 0:
                    raise RuntimeError(f"{parts[0]} exited {process.returncode}")
                return process, elapsed

            try:
                _, row["cold_on_ms"] = call("on")
                status, _ = call("status", "--json")
                state = json.loads(status.stdout)
                assert state["state"] == "running" and not state["pending_restart"]
                pid = state["identity"]["pid"]
                row["worker_pid"] = pid
                row["startup_hold_ms"] = state["runtime"]["admission"]["startup_hold_ms"]
                row["queue"] = state["runtime"]["admission"]["queue_length"]
                row["active"] = state["runtime"]["admission"]["active"]
                assert row["queue"] == 0 and row["active"] == 0
                assert row["startup_hold_ms"] > 0
                time.sleep(1)
                rss = []
                for _ in range(10):
                    observed = subprocess.run(
                        ["/bin/ps", "-p", str(pid), "-o", "rss="],
                        capture_output=True, text=True, check=True, timeout=2,
                    )
                    rss.append(int(observed.stdout.strip()))
                    time.sleep(0.1)
                row["idle_rss_kib_samples"] = rss
                _, row["idempotent_on_ms"] = call("on")
                repeated, _ = call("status", "--json")
                assert json.loads(repeated.stdout)["identity"] == state["identity"]
                _, row["empty_off_ms"] = call("off")
            finally:
                # Ownership remains available through the intact fixture path. Never
                # kill by saved PID or delete recovery state on a cleanup failure.
                _, row["cleanup_off_ms"] = call("off")
                stopped, _ = call("status", "--json")
                assert json.loads(stopped.stdout)["state"] == "stopped"
                pid = row.get("worker_pid")
                if pid is not None:
                    deadline = time.monotonic() + 2
                    while True:
                        try:
                            os.kill(pid, 0)
                        except ProcessLookupError:
                            row["worker_pid_absent"] = True
                            break
                        if time.monotonic() >= deadline:
                            raise RuntimeError("Owned worker PID still exists; fixture retained")
                        time.sleep(0.05)
                shutil.rmtree(temporary)
                row["cleanup_confirmed"] = not temporary.exists()
        result["unchanged_binary"] = digest(binary) == before
        assert result["unchanged_binary"]
        result["summary"] = {
            "binary_size_increase_bytes": result["binary_bytes"] - result["baseline_binary_bytes"],
            "cold_on_ms_samples": [r["cold_on_ms"] for r in result["runs"]],
            "empty_off_ms_samples": [r["empty_off_ms"] for r in result["runs"]],
            "idle_rss_mib_per_run_median": [statistics.median(r["idle_rss_kib_samples"]) / 1024 for r in result["runs"]],
        }
        result["passed"] = True
    except BaseException as error:
        result["failure_type"] = type(error).__name__
        raise
    finally:
        result["finished_at"] = datetime.now(timezone.utc).isoformat()
        args.output.parent.mkdir(parents=True, exist_ok=True)
        args.output.write_text(json.dumps(result, indent=2) + "\n")
        print(json.dumps({"passed": result["passed"], "summary": result.get("summary"),
                          "cleanup": [r["cleanup_confirmed"] for r in result["runs"]]}))


if __name__ == "__main__":
    main()
