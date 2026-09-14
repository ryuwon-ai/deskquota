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
BASE = PRODUCT / "artifacts/native-task2/f8-fix"
REVIEW = RESEARCH / "evidence/native-task2-quality-review"


def digest(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def identity(path, shown=None):
    return {
        "path": shown or str(path.relative_to(PRODUCT)),
        "bytes": path.stat().st_size,
        "sha256": digest(path),
    }


manifest = json.loads((OUT / "source-manifest.json").read_text())
source_drift = []
for item in manifest["files"]:
    path = RESEARCH / item["path"]
    if not path.is_file():
        source_drift.append({"path": item["path"], "reason": "missing"})
    elif path.stat().st_size != item["bytes"] or digest(path) != item["sha256"]:
        source_drift.append({"path": item["path"], "reason": "content changed"})
if source_drift:
    raise SystemExit(f"source drift after capture: {source_drift}")

review_before = json.loads((REVIEW / "before.json").read_text())
protected_drift = []
for item in review_before["protected"]:
    path = RESEARCH / item["path"]
    if not path.is_file():
        protected_drift.append({"path": item["path"], "reason": "missing"})
    elif path.stat().st_size != item["bytes"] or digest(path) != item["sha256"]:
        protected_drift.append({"path": item["path"], "reason": "content changed"})
if protected_drift:
    raise SystemExit(f"protected artifact drift: {protected_drift}")
(OUT / "protected-check.json").write_text(
    json.dumps(
        {
            "checked": len(review_before["protected"]),
            "drift": protected_drift,
            "baseline": "evidence/native-task2-quality-review/before.json",
        },
        indent=2,
    )
    + "\n"
)


def result_rows(name, expected_rows, expected_sum):
    rows = []
    for line in (OUT / name).read_text().splitlines():
        match = re.match(r"test result: ok\. (\d+) passed; 0 failed;", line)
        if match:
            rows.append(int(match.group(1)))
    if len(rows) != expected_rows or sum(rows) != expected_sum:
        raise SystemExit(f"unexpected totals in {name}: {rows}")
    return rows


full_rows = result_rows("full-tests-final.log", 13, 264)
commands = json.loads((OUT / "final-command-results.json").read_text())
if any(commands.values()):
    raise SystemExit(f"final command failed: {commands}")
pty = json.loads((OUT / "pty-final-results.json").read_text())
pty_cleanup = json.loads((OUT / "pty-final-cleanup.json").read_text())
if not (
    pty["q1_endpoint_edit"]["exit"] == 0
    and pty["q1_endpoint_edit"]["heading_preserved"]
    and pty["q1_endpoint_edit"]["port_comment_preserved"]
    and pty["q1_endpoint_edit"]["second_endpoint_note_preserved"]
    and pty["q1_endpoint_edit"]["second_models_note_preserved"]
    and pty["q1_model_edit"]["exit"] == 0
    and pty["q1_model_edit"]["model_comment_preserved"]
    and pty["q1_model_edit"]["route_comment_preserved"]
):
    raise SystemExit(f"Q1 PTY failed: {pty}")
for name in ("q2_missing_ca", "q2_malformed_ca"):
    result = pty[name]
    if not (
        result["exit"] == 1
        and result["after_state"] == "running"
        and result["identity_preserved"]
        and result["pending_restart"]
        and result["actionable_error"]
        and result["generic_child_error_absent"]
        and result["upstream_attempts"] == 0
    ):
        raise SystemExit(f"Q2 PTY failed: {name}: {result}")
if not (
    pty["q2_valid_ca_startup"]["exit"] == 0
    and pty["q2_valid_ca_startup"]["state"] == "running"
    and pty["q2_valid_ca_startup"]["authenticated_fingerprint"]
    == pty["q2_valid_ca_startup"]["desired_fingerprint"]
    and pty["q2_valid_ca_startup"]["upstream_attempts"] == 0
):
    raise SystemExit(f"valid CA PTY failed: {pty['q2_valid_ca_startup']}")
if not (
    pty["f8_saved_pending"]["save_only_identity_preserved"]
    and pty["f8_saved_pending"]["save_only_pending_restart"]
    and pty["f8_saved_pending"]["save_and_start_exit"] == 0
    and pty["f8_saved_pending"]["applied_fingerprint"]
    == pty["f8_saved_pending"]["desired_fingerprint"]
    and not pty["f8_saved_pending"]["pending_restart_after"]
    and pty["f6_running_noop"]["same_config_bytes"]
    and pty["f6_running_noop"]["same_identity"]
    and pty["f6_running_noop"]["idempotent_disclosed"]
    and pty["f6_running_noop"]["restart_side_effect_absent"]
):
    raise SystemExit(f"F8/F6 PTY failed: {pty}")
if not (
    pty_cleanup["all_pty_waited"]
    and pty_cleanup["scratch_removed"]
    and all(item["authenticated_stopped"] for item in pty_cleanup["workers"])
):
    raise SystemExit(f"PTY cleanup failed: {pty_cleanup}")

process_text = subprocess.run(
    ["ps", "-axo", "pid=,ppid=,command="], capture_output=True, text=True, check=True
).stdout
owned_processes = []
for line in process_text.splitlines():
    if "target/native-task2-quality-fix" in line and "llmgw" in line:
        owned_processes.append(line.strip())
temp_root = pathlib.Path(tempfile.gettempdir())
patterns = (
    "llmgw-setup-*",
    "llmgw-windows-*",
    "llmgw-native-*",
    "llmgw-cleanup-failure-preserve-*",
)
temp_paths = sorted({str(path) for pattern in patterns for path in temp_root.glob(pattern)})
cleanup = {
    "captured_at": datetime.datetime.now(datetime.timezone.utc).isoformat(),
    "owned_processes": owned_processes,
    "matching_temp_paths": temp_paths,
    "pty": pty_cleanup,
    "execution_finished": not owned_processes and not temp_paths,
}
(OUT / "cleanup-final.json").write_text(json.dumps(cleanup, indent=2) + "\n")
(OUT / "owned-processes-final.txt").write_text(
    "none\n" if not owned_processes else "\n".join(owned_processes) + "\n"
)
(OUT / "owned-temp-paths-final.txt").write_text(
    "none\n" if not temp_paths else "\n".join(temp_paths) + "\n"
)
if not cleanup["execution_finished"]:
    raise SystemExit(f"owned cleanup incomplete: {cleanup}")

source = json.loads((OUT / "capture-summary.json").read_text())
release = PRODUCT / "target/native-task2-quality-fix/release/llmgw"
debug = PRODUCT / "target/native-task2-quality-fix/debug/llmgw"
red = json.loads((OUT / "probe-results.json").read_text())
contract_red = json.loads((OUT / "contract-red-results.json").read_text())
if not (
    red["endpoint_edit"]["unrelated_heading_preserved"] is False
    and red["missing_ca_restart"]["after_state"] == "stopped"
    and contract_red == {"q1_exit": 101, "q2_exit": 101}
):
    raise SystemExit("RED evidence does not demonstrate both findings")

final = {
    "phase": "FINAL_HOLD",
    "execution_finished": True,
    "source_stable_after_capture": True,
    "source_drift": source_drift,
    "protected_files_checked": len(review_before["protected"]),
    "protected_drift": protected_drift,
    **source,
    "quality_findings": {
        "Q1": {
            "result": "fixed",
            "behavior": "existing validated TOML is edited by changed field using toml_edit; unchanged document fields, route arrays, comments, inline tables, and no-op bytes are retained",
            "pty_endpoint": pty["q1_endpoint_edit"],
            "pty_model": pty["q1_model_edit"],
        },
        "Q2": {
            "result": "fixed",
            "behavior": "restart builds the configured proxy/TLS client under the operation lock before authenticated stop; local failures retain the old worker and expose sanitized BuildError text",
            "missing_ca": pty["q2_missing_ca"],
            "malformed_ca": pty["q2_malformed_ca"],
            "valid_ca_startup": pty["q2_valid_ca_startup"],
        },
    },
    "regressions": {
        "F8_saved_pending": pty["f8_saved_pending"],
        "F6_running_noop": pty["f6_running_noop"],
    },
    "binaries": {
        "current_release": identity(release),
        "current_debug": identity(debug),
        "f8_release": identity(PRODUCT / "target/native-task2-f8-fix/release/llmgw"),
        "preserved_native_release": identity(PRODUCT / "target/native/release/llmgw"),
        "preserved_held_release": identity(
            PRODUCT / "artifacts/accounting-ablation/task2/held-llmgw"
        ),
    },
    "preserved": {
        "f8_source_manifest_sha256": digest(BASE / "source-manifest.json"),
        "f8_source_archive_sha256": digest(BASE / "source-hold.tar.gz"),
        "f8_final_hold_sha256": digest(BASE / "final-hold.json"),
        "quality_review_readme_sha256": digest(REVIEW / "README.md"),
        "quality_review_final_hold_sha256": digest(REVIEW / "final-hold.json"),
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
        "all_targets": {
            "passed": 264,
            "failed": 0,
            "rows": full_rows,
            "test_binaries": 13,
            "log": "artifacts/native-task2/quality-fix/full-tests-final.log",
        },
        "focused": {
            "setup": {"passed": 42, "failed": 0},
            "lifecycle": {"passed": 26, "failed": 0},
        },
        "static": {
            "rustfmt_direct_check": "passed",
            "check_all_targets": "passed",
            "clippy_all_targets_deny_warnings": "passed",
            "release_build": "passed",
        },
        "pty": {
            "sessions": len(pty_cleanup["owned_pty_pids"]),
            "release_binary": identity(release),
            "results": "artifacts/native-task2/quality-fix/pty-final-results.json",
            "cleanup": "artifacts/native-task2/quality-fix/pty-final-cleanup.json",
        },
        "red": {
            "held_f8_release": "artifacts/native-task2/quality-fix/probe-results.json",
            "contracts": contract_red,
        },
    },
    "drivers": {
        name: identity(OUT / name, f"artifacts/native-task2/quality-fix/{name}")
        for name in (
            "support_red.py",
            "probe_quality_red.py",
            "support_green.py",
            "probe_quality_green.py",
        )
    },
    "host": platform.platform(),
    "rustc": subprocess.run(
        ["rustc", "--version"], capture_output=True, text=True, check=True
    ).stdout.strip(),
    "cleanup": "artifacts/native-task2/quality-fix/cleanup-final.json",
    "limits": [
        "macOS owned PTY and loopback lifecycle runtime only",
        "Linux and Windows setup runtime remain unverified",
        "no real or paid API, inference call, model download/start, current-user client file, login registration, OS integration, Git action, benchmark, or accounting rerun",
        "Q2 preflight proves deterministic local client construction only; it does not probe upstream availability",
    ],
}
(OUT / "final-hold.json").write_text(json.dumps(final, indent=2) + "\n")

readme = f'''# Native Task 2 QUALITY Q1/Q2 Fix FINAL_HOLD

Status: FINAL_HOLD. `execution_finished=true`; product source and isolated binaries are frozen for the same fresh QUALITY reviewer.

## Fixed findings

- **Q1:** Existing valid TOML now stays in the `toml_edit` path for endpoint and model changes. The editor compares the original and desired typed config, mutates only changed fields, supports array-of-tables and inline table collections, and validates the rendered document equals the desired config. Actual PTYs retained the unrelated heading/port comments and untouched second-route internal array comments while adding an endpoint, and retained model/route comments while changing the model ID and reservation cap.
- **Q2:** Restart now constructs and drops the same configured reqwest proxy/TLS client under the existing operation lock before stopping an authenticated worker. Missing, malformed, and empty CA errors use the existing sanitized transport errors. Actual missing and malformed CA PTYs returned exit 1, kept the old PID/nonce/fingerprint running, saved pending config, and made zero upstream requests. A valid explicit CA path reached authenticated startup.

F8 SaveOnly followed by SaveAndStart applied the saved desired fingerprint; F6 unchanged SaveAndStart preserved exact config bytes and the entire worker identity. Idempotent `on`, `status`, and `off` do not reread the pending CA file.

## Direct verification

- Focused after implementation: setup **42/42**, lifecycle **26/26**.
- Final `cargo test --locked --all-targets`: **264 passed, 0 failed** across 13 result rows.
- `cargo check --locked --all-targets`, `cargo clippy --locked --all-targets -- -D warnings`, direct Rustfmt check, and `cargo build --locked --release` passed in the isolated quality-fix target.
- Exact final release PTYs: **8 sessions** — endpoint edit, model edit, missing CA restart, malformed CA restart, valid CA start, F8 SaveOnly and SaveAndStart, F6 no-op. All PTYs were waited; all seven owned configs reached authenticated stopped state before scratch deletion.
- RED is retained from the held F8 release and two focused contract failures. The initial final clippy attempt found only a new test `format!` style lint; its failed log is retained, the test was formatted, and final clippy passed.

`cargo fmt --locked` is unsupported by cargo-fmt and its exact failure is retained. Because every Cargo invocation was required to include `--locked`, formatting was applied and checked directly with the pinned Rustfmt binary instead of issuing an unlocked Cargo command.

## Identity

- Source files: {final['source_files']}; changed paths relative to F8: {final['changed_files']}.
- Source manifest: `{final['source_manifest_sha256']}`.
- Source archive: `{final['source_archive_sha256']}` ({final['source_archive_bytes']} bytes).
- Changed-files manifest: `{final['changed_files_manifest_sha256']}`.
- Final release: `{final['binaries']['current_release']['sha256']}` ({final['binaries']['current_release']['bytes']} bytes).
- Final debug: `{final['binaries']['current_debug']['sha256']}` ({final['binaries']['current_debug']['bytes']} bytes).
- F8 release/source manifest/archive/final-hold remain `{final['binaries']['f8_release']['sha256']}`, `{final['preserved']['f8_source_manifest_sha256']}`, `{final['preserved']['f8_source_archive_sha256']}`, `{final['preserved']['f8_final_hold_sha256']}`.
- Fresh review README remains `{final['preserved']['quality_review_readme_sha256']}`. All {final['protected_files_checked']} reviewer-listed protected files had zero drift.
- Protected ordinary/held binaries remain `{final['binaries']['preserved_native_release']['sha256']}` at {final['binaries']['preserved_native_release']['bytes']} bytes.

## Limits

Runtime proof is owned macOS PTY and loopback only. Linux/Windows runtime, real providers, model startup/download, user client files, login registration, OS integration, Git actions, benchmark, and accounting runs remain unverified or untouched. Q2 performs no upstream availability request.
'''
(OUT / "README.md").write_text(readme)

hash_targets = [
    OUT / "source-manifest.json",
    OUT / "source-hold.tar.gz",
    OUT / "changed-files.json",
    OUT / "final-hold.json",
    OUT / "README.md",
    OUT / "cleanup-final.json",
    OUT / "protected-check.json",
    OUT / "full-tests-final.log",
    OUT / "pty-final-results.json",
    OUT / "pty-final-cleanup.json",
    OUT / "probe-results.json",
    release,
    debug,
]
(OUT / "hashes.sha256").write_text(
    "\n".join(f"{digest(path)}  {path.relative_to(PRODUCT)}" for path in hash_targets)
    + "\n"
)
final_hash = digest(OUT / "final-hold.json")
(OUT / "FINAL_HOLD").write_text(
    f"execution_finished=true\nfinal_hold_sha256={final_hash}\nsource_manifest_sha256={final['source_manifest_sha256']}\n"
)
print(json.dumps({"execution_finished": True, "final_hold_sha256": final_hash}, indent=2))
