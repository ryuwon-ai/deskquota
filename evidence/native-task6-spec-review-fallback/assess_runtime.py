#!/usr/bin/env python3
"""Assess preserved runtime outputs after the review wrapper schema mistake."""

from __future__ import annotations

from datetime import datetime, timezone
import hashlib
import json
from pathlib import Path


OUT = Path(__file__).resolve().parent
EXPECTED = "cf7c436c442c426a6e9a1d485a5261fe5e5a05c8b131b130e679e00eec2c646b"


def sha256(path: Path) -> str:
    return hashlib.sha256(path.read_bytes()).hexdigest()


commands = json.loads((OUT / "runtime-commands.json").read_text(encoding="utf-8"))
clients = {}
for name in ("pi", "claude", "codex"):
    path = OUT / f"runtime-{name}.json"
    data = json.loads(path.read_text(encoding="utf-8"))
    clients[name] = {
        "sha256": sha256(path),
        "passed": data["passed"],
        "binary_sha256": data["binary_sha256"],
        "version": data["installed_version"],
        "protocol": data["protocol"],
        "listing": data["listing"]["status"],
        "selection": data["selection"]["status"],
        "tools": data["tools"]["status"],
        "gateway_off": data["gateway_off"]["status"],
        "gateway_off_upstream_attempts": data["gateway_off"]["upstream_attempts"],
        "disconnect": data["disconnect"]["status"],
        "cleanup": data["cleanup"],
    }

native_path = OUT / "runtime-native.json"
native = json.loads(native_path.read_text(encoding="utf-8"))
native_command = next(record for record in commands["records"] if record["name"] == "native")
native_stdout = json.loads(native_command["stdout"])
native_ok = (
    native_command["exit_code"] == 0
    and native_stdout["passed"] is True
    and native["binary_sha256"] == EXPECTED
    and native["wizard"]["invocation_count"] == 2
    and native["wizard"]["client_version_query_children"] == 0
    and native["wizard"]["invocations"]["direct_pid_sampled"]["completed_save_only"] is True
    and native["wizard"]["invocations"]["timed_command_tree"]["completed_save_only"] is True
    and len(native["idle_worker"]["samples_bytes"]) == 10
    and len(native["lifecycle"]["on_raw_nanoseconds"]) == 3
    and len(native["lifecycle"]["off_raw_nanoseconds"]) == 3
    and native["lifecycle"]["final_authenticated_state"] == "stopped"
    and native["cleanup"]["authenticated_off_and_stopped_confirmed"] is True
    and native["cleanup"]["temporary_home_removed"] is True
)
all_command_exits_zero = len(commands["records"]) == 4 and all(record["exit_code"] == 0 for record in commands["records"])
clients_ok = all(
    item["passed"] is True
    and item["binary_sha256"] == EXPECTED
    and item["selection"] == "verified"
    and item["tools"] == "verified"
    and item["gateway_off"] == "expected_failure_verified"
    and item["gateway_off_upstream_attempts"] == 0
    and item["disconnect"] == "verified"
    for item in clients.values()
)
payload = {
    "at": datetime.now(timezone.utc).isoformat(),
    "preserved_wrapper_result": {
        "path": "runtime-commands.json",
        "sha256": sha256(OUT / "runtime-commands.json"),
        "reported_passed": commands["passed"],
        "classification_error": "measure_native output JSON intentionally has no top-level passed; the command stdout carries the driver verdict",
    },
    "all_command_exits_zero": all_command_exits_zero,
    "clients": clients,
    "native": {
        "sha256": sha256(native_path),
        "driver_stdout_passed": native_stdout["passed"],
        "binary_sha256": native["binary_sha256"],
        "direct_pid_sample_count": native["wizard"]["invocations"]["direct_pid_sampled"]["sample_count"],
        "direct_pid_sampled_max_rss_bytes": native["wizard"]["invocations"]["direct_pid_sampled"]["sampled_pid_max_rss_bytes"],
        "command_tree_peak_rss_bytes": native["wizard"]["invocations"]["timed_command_tree"]["os_command_tree_peak_rss_bytes"],
        "command_tree_raw_line": native["wizard"]["invocations"]["timed_command_tree"]["raw_peak_line"],
        "idle_samples_bytes": native["idle_worker"]["samples_bytes"],
        "on_raw_nanoseconds": native["lifecycle"]["on_raw_nanoseconds"],
        "off_raw_nanoseconds": native["lifecycle"]["off_raw_nanoseconds"],
        "passed": native_ok,
    },
    "passed": all_command_exits_zero and clients_ok and native_ok,
}
with (OUT / "runtime-assessment.json").open("x", encoding="utf-8") as stream:
    json.dump(payload, stream, indent=2)
    stream.write("\n")
print(json.dumps({"passed": payload["passed"]}))
raise SystemExit(0 if payload["passed"] else 1)
