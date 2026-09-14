#!/usr/bin/env python3
import datetime
import hashlib
import json
import pathlib
import platform
import re
import subprocess
import tempfile


PRODUCT = pathlib.Path(__file__).resolve().parents[3]
RESEARCH = PRODUCT.parent
OUT = pathlib.Path(__file__).resolve().parent


def digest(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def identity(path, shown=None):
    return {
        "path": shown or str(path.relative_to(PRODUCT)),
        "bytes": path.stat().st_size,
        "sha256": digest(path),
    }


manifest = json.loads((OUT / "source-manifest.json").read_text())
drift = []
for item in manifest["files"]:
    path = RESEARCH / item["path"]
    if not path.is_file():
        drift.append({"path": item["path"], "reason": "missing"})
    elif path.stat().st_size != item["bytes"] or digest(path) != item["sha256"]:
        drift.append({"path": item["path"], "reason": "content changed"})
if drift:
    raise SystemExit(f"source drift after capture: {drift}")


def result_rows(name, expected_rows, expected_sum):
    rows = []
    for line in (OUT / name).read_text().splitlines():
        match = re.match(r"test result: ok\. (\d+) passed; 0 failed;", line)
        if match:
            rows.append(int(match.group(1)))
    if len(rows) != expected_rows or sum(rows) != expected_sum:
        raise SystemExit(f"unexpected totals in {name}: {rows}")
    return rows


full_rows = result_rows("full-tests-final.log", 13, 261)
focused_rows = result_rows("focused-tests.log", 4, 95)
saved = json.loads((OUT / "saved-pending-final-results.json").read_text())
noop = json.loads((OUT / "running-noop-final-results.json").read_text())
external = json.loads((OUT / "external-pending-final-results.json").read_text())
if not (
    saved["save_only_exit"] == 0
    and saved["save_only_pending_restart"]
    and saved["save_only_preserved_worker_identity"]
    and saved["save_and_start_exit"] == 0
    and saved["saved_config_applied"]
    and not saved["pending_restart_after_save_and_start"]
    and not saved["restart_required_error"]
):
    raise SystemExit(f"saved-pending PTY failed: {saved}")
if not (
    noop["setup_exit"] == 0
    and noop["same_fingerprint"]
    and not noop["identity_changed"]
    and noop["summary_says_no_restart_required"]
    and noop["summary_says_idempotent_on"]
    and not noop["summary_mentions_cancel_or_drain"]
):
    raise SystemExit(f"noop PTY failed: {noop}")
if not (
    external["setup_exit"] == 0
    and external["saved_config_applied"]
    and not external["pending_restart"]
    and external["identity_changed"]
    and external["preview_disclosed_restart"]
    and external["preview_disclosed_queue_cancel"]
    and external["preview_disclosed_drain"]
):
    raise SystemExit(f"external-pending PTY failed: {external}")
for name in (
    "saved-pending-final-cleanup.json",
    "running-noop-final-cleanup.json",
    "external-pending-final-cleanup.json",
):
    cleanup = json.loads((OUT / name).read_text())
    if not cleanup["all_pty_waited"] or not cleanup["scratch_removed"]:
        raise SystemExit(f"PTY cleanup failed: {name}: {cleanup}")

processes = {}
for name in ("llmgw", "cargo", "rustc"):
    result = subprocess.run(["pgrep", "-x", name], capture_output=True, text=True)
    processes[name] = [int(value) for value in result.stdout.split()]
temp_root = pathlib.Path(tempfile.gettempdir())
patterns = (
    "llmgw-setup-*",
    "llmgw-windows-*",
    "llmgw-native-task2*",
    "llmgw-cleanup-failure-preserve-*",
)
temp_paths = sorted({str(path) for pattern in patterns for path in temp_root.glob(pattern)})
cleanup = {
    "captured_at": datetime.datetime.now(datetime.timezone.utc).isoformat(),
    "processes": processes,
    "matching_temp_paths": temp_paths,
    "saved_pending": json.loads((OUT / "saved-pending-final-cleanup.json").read_text()),
    "unchanged_worker": json.loads((OUT / "running-noop-final-cleanup.json").read_text()),
    "external_pending": json.loads((OUT / "external-pending-final-cleanup.json").read_text()),
    "execution_finished": not any(processes.values()) and not temp_paths,
}
(OUT / "cleanup-final.json").write_text(json.dumps(cleanup, indent=2) + "\n")
(OUT / "owned-processes-final.txt").write_text(
    "none\n"
    if not any(processes.values())
    else "\n".join(f"{name}: {pid}" for name, pids in processes.items() for pid in pids) + "\n"
)
(OUT / "owned-temp-paths-final.txt").write_text(
    "none\n" if not temp_paths else "\n".join(temp_paths) + "\n"
)
if not cleanup["execution_finished"]:
    raise SystemExit(f"owned cleanup incomplete: {cleanup}")

source = json.loads((OUT / "capture-summary.json").read_text())
release = PRODUCT / "target/native-task2-f8-fix/release/llmgw"
debug = PRODUCT / "target/native-task2-f8-fix/debug/llmgw"
prior = PRODUCT / "artifacts/native-task2/spec-fix"
final = {
    "phase": "FINAL_HOLD",
    "execution_finished": True,
    "source_stable_after_capture": True,
    "source_drift": drift,
    **source,
    "f8": {
        "result": "fixed",
        "behavior": "typed desired-versus-authenticated-worker fingerprint impact drives idempotent on, explicit restart, or pre-write refusal",
        "restart_disclosure": "desired and worker fingerprints, queue cancellation, active drain up to 10 seconds, and desired-fingerprint readiness",
        "saved_pending_pty": saved,
        "external_pending_pty": external,
        "unchanged_worker_pty": noop,
    },
    "binaries": {
        "current_release": identity(release),
        "current_debug": identity(debug),
        "prior_spec_fix_release": identity(
            PRODUCT / "target/native-task2-spec-fix/release/llmgw"
        ),
        "initial_native_task2_release": identity(
            PRODUCT / "target/native-task2/release/llmgw"
        ),
        "preserved_native_release": identity(PRODUCT / "target/native/release/llmgw"),
        "preserved_held_release": identity(
            PRODUCT / "artifacts/accounting-ablation/task2/held-llmgw"
        ),
    },
    "preserved": {
        "prior_spec_fix_manifest_sha256": digest(prior / "source-manifest.json"),
        "prior_spec_fix_archive_sha256": digest(prior / "source-hold.tar.gz"),
        "prior_spec_fix_final_hold_sha256": digest(prior / "final-hold.json"),
        "rereview_readme_sha256": digest(
            RESEARCH / "evidence/native-task2-spec-review/rereview/README.md"
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
    },
    "verification": {
        "focused": {
            "passed": 95,
            "failed": 0,
            "rows": focused_rows,
            "log": "artifacts/native-task2/f8-fix/focused-tests.log",
        },
        "all_targets": {
            "passed": 261,
            "failed": 0,
            "rows": full_rows,
            "test_binaries": 13,
            "log": "artifacts/native-task2/f8-fix/full-tests-final.log",
        },
        "static": {
            "fmt": "passed",
            "check_all_targets": "passed",
            "clippy_all_targets_deny_warnings": "passed",
            "release_build": "passed",
        },
        "pty": {
            "sessions": 4,
            "saved_pending": "two setup PTYs: Save only, then Save and start",
            "external_pending": "one setup PTY",
            "unchanged_worker": "one setup PTY",
            "release_binary": identity(release),
        },
        "red": {
            "held_release_result": "artifacts/native-task2/f8-fix/saved-pending-red-results.json",
            "typed_contract_compile_red": "artifacts/native-task2/f8-fix/setup-contract-red.log",
        },
        "green": {
            "saved_pending": "artifacts/native-task2/f8-fix/saved-pending-final-results.json",
            "external_pending": "artifacts/native-task2/f8-fix/external-pending-final-results.json",
            "unchanged_worker": "artifacts/native-task2/f8-fix/running-noop-final-results.json",
        },
    },
    "drivers": {
        name: identity(OUT / name, f"artifacts/native-task2/f8-fix/{name}")
        for name in (
            "probe_red_base.py",
            "probe_saved_pending_red.py",
            "probe_green_base.py",
            "probe_saved_pending_green.py",
            "probe_running_green.py",
            "probe_external_pending_green.py",
        )
    },
    "host": platform.platform(),
    "rustc": subprocess.run(["rustc", "--version"], capture_output=True, text=True, check=True).stdout.strip(),
    "cleanup": "artifacts/native-task2/f8-fix/cleanup-final.json",
    "limits": [
        "macOS owned PTY and loopback lifecycle runtime only",
        "Linux and Windows setup runtime remain unverified",
        "no real or paid API, inference call, model download/start, current-user client file, login registration, OS integration, Git action, benchmark, or accounting rerun",
        "no external-editor filesystem CAS claim; expected fingerprints are rechecked under the lifecycle operation lock and the child retains its existing fingerprint guard",
    ],
}
(OUT / "final-hold.json").write_text(json.dumps(final, indent=2, ensure_ascii=False) + "\n")

readme = f'''# Native Task 2 F8 Fix FINAL_HOLD

Status: FINAL_HOLD. `execution_finished=true`; product source and isolated binaries are frozen for the same fresh SPEC reviewer.

## F8 result

Setup now builds a typed `RuntimeImpact` from the desired rendered config fingerprint and authenticated worker identity. A saved pending or external edit is disclosed before the final choice with both fingerprints, queue cancellation, active drain up to ten seconds, and desired-fingerprint readiness. Selecting Save and start carries explicit restart intent into apply. Apply rechecks the decision and expected fingerprint under the setup and lifecycle operation locks; unverified state or a mismatch whose restart impact was not previewed is refused before writing or restarting.

If desired config already matches the worker, setup uses authenticated idempotent `on` even when the prior disk state had been pending. Save only remains non-disruptive. Public Native1 on/restart behavior is unchanged; setup uses narrow expected-fingerprint variants.

## Direct verification

- Focused Rust: 95 passed, 0 failed — lib 11, config 19, lifecycle 25, setup 40 (`focused-tests.log`).
- Full `cargo test --locked --all-targets`: 261 passed, 0 failed across 13 result rows (`full-tests-final.log`).
- fmt, check, clippy with warnings denied, and release build passed in `target/native-task2-f8-fix`.
- Exact final release PTYs: four sessions. SaveOnly→SaveAndStart applies the saved fingerprint and clears pending restart; an external edit predating setup does the same; unchanged F6 preserves PID, nonce, fingerprint, address, and start time. Every worker was authenticated-stopped and every PTY waited.
- RED: held release SaveAndStart exited 1 with `restart_required` and retained the old fingerprint (`saved-pending-red-results.json`). The new typed API first failed compilation in `setup-contract-red.log`.

The intermediate `setup-contract-green-first.log` is intentionally retained: direct restart inside an integration-test executable starts `current_exe()` as the test binary and produced `worker_start_failed`. Final restart/readiness evidence therefore uses the actual `llmgw` PTY without adding a test-only product override. `runtime-decision-green-first.log` is an invocation-argument error; its corrected run is `runtime-decision-green.log`.

## Identity

- Source files: {final['source_files']}.
- Source manifest: `{final['source_manifest_sha256']}`.
- Source archive: `{final['source_archive_sha256']}` ({final['source_archive_bytes']} bytes).
- Changed-files manifest: `{final['changed_files_manifest_sha256']}` (8 paths relative to the prior SPEC-fix HOLD).
- Final release: `{final['binaries']['current_release']['sha256']}` ({final['binaries']['current_release']['bytes']} bytes).
- Final debug: `{final['binaries']['current_debug']['sha256']}` ({final['binaries']['current_debug']['bytes']} bytes).
- Prior SPEC-fix source/archive/final-hold remain `{final['preserved']['prior_spec_fix_manifest_sha256']}`, `{final['preserved']['prior_spec_fix_archive_sha256']}`, and `{final['preserved']['prior_spec_fix_final_hold_sha256']}`. Its release remains `{final['binaries']['prior_spec_fix_release']['sha256']}`.
- Re-review README remains `{final['preserved']['rereview_readme_sha256']}`.
- Protected ordinary/held binaries remain `{final['binaries']['preserved_native_release']['sha256']}` at {final['binaries']['preserved_native_release']['bytes']} bytes.

## Cleanup and limits

No owned llmgw/Cargo/rustc process or matching temporary path remains (`cleanup-final.json`). Runtime proof is macOS only. Linux/Windows runtime, real providers, model startup/download, user client files, login registration, OS integration, Git actions, benchmark, and accounting runs remain unverified or untouched.
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
    OUT / "full-tests-final.log",
    OUT / "focused-tests.log",
    OUT / "saved-pending-red-results.json",
    OUT / "saved-pending-final-results.json",
    OUT / "external-pending-final-results.json",
    OUT / "running-noop-final-results.json",
]
(OUT / "hashes.sha256").write_text(
    "\n".join(
        f"{digest(path)}  {path.relative_to(PRODUCT)}" for path in hash_targets
    )
    + "\n"
)
final_hash = digest(OUT / "final-hold.json")
(OUT / "FINAL_HOLD").write_text(
    f"execution_finished=true\nfinal_hold_sha256={final_hash}\nsource_manifest_sha256={final['source_manifest_sha256']}\n"
)
print(json.dumps({"execution_finished": True, "final_hold_sha256": final_hash}, indent=2))
