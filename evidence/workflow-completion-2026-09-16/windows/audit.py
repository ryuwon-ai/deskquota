"""Audit the saved native Windows results without contacting the host."""
from datetime import datetime, timezone
import hashlib
import json
from pathlib import Path
import re

ROOT = Path(__file__).resolve().parent

def read(name):
    return json.loads((ROOT / name).read_text(encoding="utf-8-sig"))

def digest(name):
    return hashlib.sha256((ROOT / name).read_bytes()).hexdigest()

build = read("build-result.json")
final = read("final-state.json")
lifecycle = read("native-modes.json")
probes = {mode: read(mode + "-final-retry-directives.json") for mode in ("default", "bpe")}
assert build["source_archive_sha256"] == "0aff89243340bbf4b9f30c9f2477ac1691657f47a3c7730a873461ff975f63c0"
assert build["source_manifest_sha256"] == "7861a7fc39e830a444d87bf8db9ff89a8149a7b24fb4e3c91904c5e44850e86a"
assert build["verified_source_files"] == final["verified_rust_source_files"] == 94
assert final["unchanged_probe_sources"] == 2
assert not final["owned_processes"] and not final["remaining_runtime_temp_roots"]
assert lifecycle["passed"] and all(lifecycle["checks"].values())
assert lifecycle["owned_lifecycle_temp_removed"]
assert digest("windows-retry-wrapper.py") == final["wrapper_sha256"]
assert digest("windows-native-modes.py") == final["lifecycle_probe_sha256"] == lifecycle["script_sha256"]
expected_counts = {"default": {"passed": 409, "failed": 0, "ignored": 1},
                   "bpe": {"passed": 236, "failed": 0, "ignored": 1}}
modes = []
for row in build["modes"]:
    mode = row["mode"]
    assert row["tests_exit"] == row["release_exit"] == 0
    text = (ROOT / (mode + "-tests.stdout.log")).read_text(encoding="utf-8")
    lines = [line for line in text.splitlines() if "test result:" in line]
    counts = {name: sum(int(re.search(r"(\d+) " + name, line).group(1)) for line in lines)
              for name in ("passed", "failed", "ignored")}
    assert counts == expected_counts[mode]
    assert all(name + " ... ok" in text for name in row["new_mode_regressions_executed"])
    recorded = next(binary for binary in final["binaries"] if binary["mode"] == mode)
    probe = probes[mode]
    assert row["binary_sha256"] == recorded["sha256"] == lifecycle["binaries"][mode]["sha256"] == probe["binary_sha256"]
    assert row["binary_bytes"] == recorded["bytes"] == lifecycle["binaries"][mode]["bytes"]
    assert probe["passed"] and probe["binary_unchanged"] and len(probe["cases"]) == 11
    assert all(case["passed"] and all(case["checks"].values()) for case in probe["cases"])
    assert probe["native_windows_adapter"]["wrapper_sha256"] == final["wrapper_sha256"]
    assert probe["probe_sha256"] == "884445428df4a38a0545c600a614919064260c7e7387e254090fea128a5c9a8e"
    assert probe["fixture_sha256"] == "cfc7a8f14717a70576f28db854cef0899318b477524eadd282f6d6e9bcf67f7c"
    assert sum(case["ingress"] for case in probe["cases"]) == 17
    assert sum(case["upstream_attempts"] for case in probe["cases"]) == 18
    modes.append({"mode": mode, "tests": counts,
        "test_scope": "default cargo test" if mode == "default" else "focused eight targets with --features bpe",
        "new_regressions_executed": row["new_mode_regressions_executed"],
        "binary_sha256": row["binary_sha256"], "binary_bytes": row["binary_bytes"],
        "retry_probe": {"cases": 11, "passed": 11, "ingress": 17, "upstream_attempts": 18}})
failures = []
for name in ("default-retry-directives.json", "default-diagnostic-retry-directives.json"):
    failed = read(name)
    assert not failed["passed"] and len(failed["cases"]) == 11
    assert all(not case["passed"] and not any(case["observation"].values()) for case in failed["cases"])
    failures.append({"artifact": name, "harness_setup_errors": 11, "submitted_ingress": 0,
        "upstream_attempts": 0, "classification": "Windows parent Python SystemRoot removed before socket creation"})
summary = {"phase": "windows-default-off-bpe-and-retry-veto-final", "passed": True,
    "audited_at": datetime.now(timezone.utc).isoformat(), "platform": final["platform"],
    "toolchain": build["toolchain"], "source_archive_sha256": build["source_archive_sha256"],
    "source_manifest_sha256": build["source_manifest_sha256"], "verified_rust_source_files": 94,
    "modes": modes, "commands": build["commands"],
    "build_environment": {"build_jobs": 2, "incremental": False,
        "linker": "C:/mingw64/bin/gcc.exe", "rustflags": "-C link-self-contained=no -C dlltool=C:/mingw64/bin/dlltool.exe",
        "cargo_target": "ownedlab/target outside source", "source_mtimes_refreshed_after_verification": True},
    "native_lifecycle": {"passed": True, "check_flags": len(lifecycle["checks"]),
        "cli_invocations": len(lifecycle["commands"]), "restricted_path": lifecycle["restricted_path"],
        "cross_capability": {key: lifecycle["checks"][key] for key in (
            "default_can_inspect_bpe_worker", "default_restart_reports_missing_bpe",
            "default_restart_preserves_healthy_bpe_identity", "default_can_stop_bpe_worker",
            "default_new_on_rejects_bpe_config", "rejected_start_created_no_worker")}},
    "retained_failed_trials": failures,
    "adapter": {"sha256": final["wrapper_sha256"], "source_probes_unchanged": True,
        "fixture_semantics_changed": False,
        "changes": probes["default"]["native_windows_adapter"]["changes"]},
    "cleanup": {"owned_processes": 0, "runtime_temp_roots": 0, "initial_failure_empty_dirs_removed": 2},
    "limitations": ["One Windows 10 x64 host with existing GNU toolchain",
        "BPE focused tests overlap default suite; do not sum as unique tests",
        "No Windows performance comparison, client compaction rerun, installation or global settings change",
        "No real provider calls, Docker, WSL or MSVC requirement introduced"],
    "raw_hashes": {name: digest(name) for name in ("build-result.json", "default-tests.stdout.log",
        "bpe-tests.stdout.log", "default-release.stderr.log", "bpe-release.stderr.log",
        "native-modes.json", "default-final-retry-directives.json", "bpe-final-retry-directives.json",
        "default-retry-directives.json", "default-diagnostic-retry-directives.json",
        "windows-retry-wrapper.py", "windows-native-modes.py", "final-state.json")}}
(ROOT / "final-validation.json").write_text(json.dumps(summary, indent=2) + "\n", encoding="utf-8")
print(json.dumps({"passed": True, "modes": modes, "native_lifecycle_flags": len(lifecycle["checks"]),
    "retained_harness_setup_errors": 22, "cleanup": summary["cleanup"]}, indent=2))
