#!/usr/bin/env python3
import datetime
import hashlib
import json
import pathlib
import platform
import re
import subprocess


PRODUCT = pathlib.Path(__file__).resolve().parents[3]
OUT = pathlib.Path(__file__).resolve().parent
RESEARCH = PRODUCT.parent


def digest(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def identity(path, shown=None):
    return {
        "path": shown or str(path.relative_to(PRODUCT)),
        "bytes": path.stat().st_size,
        "sha256": digest(path),
    }


manifest_path = OUT / "source-manifest.json"
manifest = json.loads(manifest_path.read_text())
drift = []
for item in manifest["files"]:
    path = RESEARCH / item["path"]
    if not path.is_file():
        drift.append({"path": item["path"], "reason": "missing"})
    elif path.stat().st_size != item["bytes"] or digest(path) != item["sha256"]:
        drift.append({"path": item["path"], "reason": "content changed"})
if drift:
    raise SystemExit(f"source drift after capture: {drift}")

test_rows = []
for line in (OUT / "full-tests-final.log").read_text().splitlines():
    match = re.match(r"test result: ok\. (\d+) passed; 0 failed;", line)
    if match:
        test_rows.append(int(match.group(1)))
if len(test_rows) != 13 or sum(test_rows) != 255:
    raise SystemExit(f"unexpected final test totals: {test_rows}")

pty = json.loads((OUT / "pty-final-exact.json").read_text())
release = PRODUCT / "target/native-task2-spec-fix/release/llmgw"
debug = PRODUCT / "target/native-task2-spec-fix/debug/llmgw"
if pty.get("result") != "pass" or pathlib.Path(pty["binary"]).resolve() != release.resolve():
    raise SystemExit("final PTY did not pass against the held release path")
if len(pty["cases"]) != 7:
    raise SystemExit("final PTY did not report seven cases")

processes = {}
for name in ("llmgw", "cargo", "rustc"):
    result = subprocess.run(["pgrep", "-x", name], capture_output=True, text=True)
    processes[name] = [int(value) for value in result.stdout.split()]
temp_root = pathlib.Path(__import__("tempfile").gettempdir())
patterns = (
    "llmgw-setup-*",
    "llmgw-windows-*",
    "llmgw-native-task2*",
    "llmgw-pty-*",
    "llmgw-cleanup-failure-preserve-*",
)
temp_paths = sorted({str(path) for pattern in patterns for path in temp_root.glob(pattern)})
(OUT / "owned-processes-final.txt").write_text(
    "none\n"
    if not any(processes.values())
    else "\n".join(f"{name}: {pid}" for name, pids in processes.items() for pid in pids) + "\n"
)
(OUT / "owned-temp-paths-final.txt").write_text(
    "none\n" if not temp_paths else "\n".join(temp_paths) + "\n"
)
cleanup = {
    "captured_at": datetime.datetime.now(datetime.timezone.utc).isoformat(),
    "processes": processes,
    "matching_temp_paths": temp_paths,
    "green_probe_cleanup": json.loads((OUT / "green-probe-cleanup.json").read_text()),
    "green_running_cleanup": json.loads((OUT / "green-running-cleanup.json").read_text()),
    "final_pty_result": pty["result"],
    "final_pty_cases": len(pty["cases"]),
    "execution_finished": not any(processes.values()) and not temp_paths,
}
(OUT / "cleanup-final.json").write_text(json.dumps(cleanup, indent=2) + "\n")
if not cleanup["execution_finished"]:
    raise SystemExit(f"owned cleanup incomplete: {cleanup}")

source_summary = json.loads((OUT / "capture-summary.json").read_text())
final = {
    "phase": "FINAL_HOLD",
    "execution_finished": True,
    "source_stable_after_capture": True,
    "source_drift": drift,
    **source_summary,
    "binaries": {
        "current_release": identity(release),
        "current_debug": identity(debug),
        "initial_native_task2_release": identity(
            PRODUCT / "target/native-task2/release/llmgw"
        ),
        "preserved_native_release": identity(PRODUCT / "target/native/release/llmgw"),
        "preserved_held_release": identity(
            PRODUCT / "artifacts/accounting-ablation/task2/held-llmgw"
        ),
    },
    "preserved": {
        "initial_native_task2_source_manifest_sha256": digest(
            PRODUCT / "artifacts/native-task2/source-manifest.json"
        ),
        "initial_native_task2_source_archive_sha256": digest(
            PRODUCT / "artifacts/native-task2/source-hold.tar.gz"
        ),
        "accounting_baseline_manifest_sha256": digest(
            PRODUCT / "artifacts/accounting-ablation/quality-fix/source-manifest.json"
        ),
        "accounting_baseline_archive_sha256": digest(
            PRODUCT / "artifacts/accounting-ablation/quality-fix/source-hold.tar.gz"
        ),
        "accounting_pilot_sha256": digest(
            PRODUCT / "artifacts/accounting-ablation/pilot.json"
        ),
        "spec_review_readme_sha256": digest(
            RESEARCH / "evidence/native-task2-spec-review/README.md"
        ),
    },
    "f1_f7": {
        "F1": "same-directory full write/sync publication; SIGXFSZ preserves 417-byte existing file; Windows ambiguous replacement retains protected original and candidate recovery paths",
        "F2": "persistent setup writer lock plus exact draft snapshot carried to the immediate pre-publication check; concurrent edit rejected and preserved",
        "F3": "loaded Env header/name and Models/CountTokens selections remain defaults; metadata-only root is accepted without inventing generation support",
        "F4": "loaded login and client pending intents remain selected and persisted",
        "F5": "request-bounded model without a configured fallback is accepted; summary distinguishes request cap from provider limit",
        "F6": "unchanged SaveAndStart uses authenticated on and preserves PID, nonce, fingerprint, address, and start time",
        "F7": "invalid config and missing inference arguments exit 2 while live malformed-edit restart-required behavior and off/status identity recovery remain intact",
    },
    "verification": {
        "all_targets": {
            "passed": 255,
            "failed": 0,
            "test_result_rows": test_rows,
            "test_binaries": 13,
            "log": "artifacts/native-task2/spec-fix/full-tests-final.log",
        },
        "setup_contract": {
            "passed": 36,
            "failed": 0,
            "log": "artifacts/native-task2/spec-fix/focused-green-setup-final.log",
        },
        "lifecycle_contract": {
            "passed": 25,
            "failed": 0,
            "log": "artifacts/native-task2/spec-fix/lifecycle-green.log",
        },
        "windows_failure_model": {
            "passed": 3,
            "failed": 0,
            "red_log": "artifacts/native-task2/spec-fix/windows-recovery-red.log",
            "green_log": "artifacts/native-task2/spec-fix/windows-recovery-final.log",
            "actual_windows_runtime": "unverified",
        },
        "static": {
            "fmt": "passed",
            "check_all_targets": "passed",
            "clippy_all_targets_deny_warnings": "passed",
            "release_build": "passed",
        },
        "pty": {
            "result": pty["result"],
            "cases": len(pty["cases"]),
            "binary": identity(release),
            "script": identity(PRODUCT / "scripts/probe_native_setup.py"),
            "result_file": identity(OUT / "pty-final-exact.json", "artifacts/native-task2/spec-fix/pty-final-exact.json"),
            "stderr_bytes": (OUT / "pty-final-exact.stderr").stat().st_size,
        },
        "red_green": {
            "combined_red": "artifacts/native-task2/spec-fix/probe-results.json",
            "combined_green": "artifacts/native-task2/spec-fix/green-probe-results.json",
            "running_red": "artifacts/native-task2/spec-fix/running-noop-results.json",
            "running_green": "artifacts/native-task2/spec-fix/green-running-noop-results.json",
            "metadata_only_red": "artifacts/native-task2/spec-fix/metadata-only-red-result.json",
            "metadata_only_green": "artifacts/native-task2/spec-fix/metadata-only-result.json",
        },
    },
    "host": platform.platform(),
    "rustc": subprocess.run(["rustc", "--version"], capture_output=True, text=True, check=True).stdout.strip(),
    "cleanup": "artifacts/native-task2/spec-fix/cleanup-final.json",
    "limits": [
        "macOS PTY and owned loopback runtime only",
        "Linux and Windows setup runtime unverified; the Windows failure model is host-side simulation only",
        "no Windows cross-build rerun because the previously recorded SDK/aws-lc headers blocker did not change",
        "no real or paid API, model download/start, current-user client file, login registration, installer, commit, release, benchmark, or accounting matrix exercised",
        "the last snapshot check plus rename/ReplaceFileW is not an external-editor filesystem CAS primitive",
    ],
}
(OUT / "final-hold.json").write_text(json.dumps(final, indent=2, ensure_ascii=False) + "\n")

readme = f'''# Native Task 2 SPEC Fix FINAL_HOLD

Status: FINAL_HOLD. `execution_finished=true`; product source and the isolated binaries are frozen for the same fresh SPEC reviewer.

## F1-F7 result

- F1: Existing macOS/Linux files are fully written and synced before publication. The actual SIGXFSZ repro now preserves all 417 original bytes. Windows keeps protected original and replacement recovery copies across ambiguous `ReplaceFileW` or post-write verification failure; the host-side 1176 model proves both contents remain recoverable.
- F2: Final apply holds a persistent setup writer lock and compares the exact draft snapshot again immediately before publication. The concurrent-edit repro exits 1 and keeps the external edit.
- F3: The dialog keeps the loaded Env header/name and Models/CountTokens endpoints. A Models-only root completes without a fabricated generation protocol.
- F4: Loaded login and client pending intents remain selected and persist through re-setup.
- F5: A model bounded by each request is valid without a configured fallback. The summary calls any configured fallback a reservation estimate, not an upstream limit.
- F6: Unchanged SaveAndStart uses authenticated `on`; PID, nonce, fingerprint, address, and start time remain identical.
- F7: Invalid configuration and missing inference arguments exit 2. The lifecycle suite preserves the distinct live-worker restart-required exit 1 and usable off/status identity path.

## Direct verification

- Final `cargo test --locked --all-targets`: 255 passed, 0 failed across 13 result rows (`full-tests-final.log`). This supersedes the pre-Windows-safety 252 count and the earlier intentionally retained failed `integration-green.log` run with three stale exit expectations.
- Setup contract: 36/36; lifecycle contract: 25/25.
- Windows recovery decision model: 3/3 after `windows-recovery-red.log` failed before the helper existed.
- `cargo fmt --all -- --check`, `cargo check --locked --all-targets`, `cargo clippy --locked --all-targets -- -D warnings`, and `cargo build --locked --release`: passed.
- Final macOS PTY: 7/7 against the exact held release path, including cancel, Back, Input Ctrl-C, Unicode/space save-only, authenticated save-and-start, injected post-start cleanup, and cleanup-failure preservation (`pty-final-exact.json`).
- Combined F1-F7 RED/GREEN observations are in `probe-results.json` / `green-probe-results.json`; running identity in `running-noop-results.json` / `green-running-noop-results.json`; metadata-only prompt behavior in `metadata-only-red-result.json` / `metadata-only-result.json`.

## Identity

- Source files: {final['source_files']}.
- Source manifest SHA-256: `{final['source_manifest_sha256']}`.
- Source archive SHA-256: `{final['source_archive_sha256']}` ({final['source_archive_bytes']} bytes).
- Changed-files manifest SHA-256: `{final['changed_files_manifest_sha256']}` (12 paths relative to initial Native Task 2 HOLD).
- Final isolated release: `{final['binaries']['current_release']['sha256']}` ({final['binaries']['current_release']['bytes']} bytes).
- Final isolated debug: `{final['binaries']['current_debug']['sha256']}` ({final['binaries']['current_debug']['bytes']} bytes).
- PTY script: `{final['verification']['pty']['script']['sha256']}`; exact PTY result: `{final['verification']['pty']['result_file']['sha256']}`.

Initial Native Task 2 source/archive/release remain `{final['preserved']['initial_native_task2_source_manifest_sha256']}`, `{final['preserved']['initial_native_task2_source_archive_sha256']}`, and `{final['binaries']['initial_native_task2_release']['sha256']}`. Protected ordinary/held binaries remain `{final['binaries']['preserved_native_release']['sha256']}` at {final['binaries']['preserved_native_release']['bytes']} bytes. Accounting baseline manifest/archive and pilot remain `{final['preserved']['accounting_baseline_manifest_sha256']}`, `{final['preserved']['accounting_baseline_archive_sha256']}`, and `{final['preserved']['accounting_pilot_sha256']}`.

## Cleanup and limits

All owned `llmgw`, Cargo, and rustc processes are absent; matching owned temp paths are absent. Driver-specific cleanup records show all PTYs waited, authenticated worker stop, and scratch removal (`cleanup-final.json`).

Runtime PTY/network evidence is macOS only. Linux and Windows runtime remain unverified. The Windows 1176 behavior is a failure-model test around the safe helper, not native Windows execution. No real/paid API, model, user client file, login registration, OS integration, Git action, release, benchmark, or accounting rerun occurred.
'''
(OUT / "README.md").write_text(readme)

hash_targets = [
    OUT / "source-manifest.json",
    OUT / "source-hold.tar.gz",
    OUT / "changed-files.json",
    OUT / "final-hold.json",
    OUT / "README.md",
    OUT / "cleanup-final.json",
    release,
    debug,
    PRODUCT / "scripts/probe_native_setup.py",
    OUT / "pty-final-exact.json",
    OUT / "full-tests-final.log",
    OUT / "windows-recovery-red.log",
    OUT / "windows-recovery-final.log",
]
lines = []
for path in hash_targets:
    shown = path.relative_to(PRODUCT) if path.is_relative_to(PRODUCT) else path
    lines.append(f"{digest(path)}  {shown}")
(OUT / "hashes.sha256").write_text("\n".join(lines) + "\n")
final_hash = digest(OUT / "final-hold.json")
(OUT / "FINAL_HOLD").write_text(
    f"execution_finished=true\nfinal_hold_sha256={final_hash}\nsource_manifest_sha256={final['source_manifest_sha256']}\n"
)
print(json.dumps({"execution_finished": True, "final_hold_sha256": final_hash}, indent=2))
