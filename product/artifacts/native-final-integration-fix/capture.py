#!/usr/bin/env python3
import datetime
import gzip
import hashlib
import json
import pathlib
import re
import shutil
import subprocess
import tarfile

PRODUCT = pathlib.Path(__file__).resolve().parents[2]
RESEARCH = PRODUCT.parent
ARTIFACT = PRODUCT / "artifacts/native-final-integration-fix"
BASE = PRODUCT / "artifacts/native-task6/final-doc-fix/source-manifest.json"
RELEASE = PRODUCT / "target/native-final-integration-fix/release/llmgw"
DEBUG = PRODUCT / "target/native-final-integration-fix/debug/llmgw"
PHASE = "native-final-integration-fix"


def sha(path):
    digest = hashlib.sha256()
    with path.open("rb") as source:
        for chunk in iter(lambda: source.read(1024 * 1024), b""):
            digest.update(chunk)
    return digest.hexdigest()


def record(path):
    return {
        "path": path.relative_to(RESEARCH).as_posix(),
        "bytes": path.stat().st_size,
        "sha256": sha(path),
    }


def write_json(path, value):
    path.write_text(json.dumps(value, indent=2, ensure_ascii=False) + "\n")


captured_at = datetime.datetime.now(datetime.timezone.utc).isoformat()
base = json.loads(BASE.read_text())
base_by_path = {entry["path"]: entry for entry in base["files"]}
source_files = []
for entry in base["files"]:
    source = RESEARCH / entry["path"]
    source_files.append(record(source))

changed = []
for entry in source_files:
    before = base_by_path[entry["path"]]
    if entry["sha256"] != before["sha256"] or entry["bytes"] != before["bytes"]:
        changed.append(
            {
                "path": entry["path"],
                "before_bytes": before["bytes"],
                "before_sha256": before["sha256"],
                "after_bytes": entry["bytes"],
                "after_sha256": entry["sha256"],
            }
        )

expected_changed = [
    "product/docs/client-compatibility.md",
    "product/docs/installation.md",
    "product/docs/runtime-contract.md",
    "product/src/lifecycle/mod.rs",
    "product/src/setup/persist.rs",
    "product/src/setup/summary.rs",
    "product/tests/client_profiles.rs",
    "product/tests/setup_contract.rs",
]
if [entry["path"] for entry in changed] != expected_changed:
    raise SystemExit("unexpected source drift")

shutil.copy2(RELEASE, ARTIFACT / "llmgw-macos-arm64")
shutil.copy2(DEBUG, ARTIFACT / "llmgw-macos-arm64-debug")
release_record = record(RELEASE)
debug_record = record(DEBUG)

source_manifest = {
    "phase": PHASE,
    "captured_at": captured_at,
    "base_manifest": record(BASE),
    "base_source_archive_sha256": "2ac55075bcc246bdbd114aa654f1114098d13bd4976050575cefa3d08253ebe5",
    "files": source_files,
    "changed_paths": expected_changed,
    "release_binary": release_record,
    "debug_binary": debug_record,
    "stable_during_capture": False,
}
manifest_path = ARTIFACT / "source-manifest.json"
write_json(manifest_path, source_manifest)

archive_path = ARTIFACT / "source.tar.gz"
with archive_path.open("wb") as raw:
    with gzip.GzipFile(filename="source.tar", mode="wb", fileobj=raw, mtime=0) as compressed:
        with tarfile.open(fileobj=compressed, mode="w", format=tarfile.PAX_FORMAT) as archive:
            for entry in source_files:
                source = RESEARCH / entry["path"]
                info = archive.gettarinfo(str(source), arcname=entry["path"])
                info.uid = 0
                info.gid = 0
                info.uname = ""
                info.gname = ""
                info.mtime = 0
                with source.open("rb") as contents:
                    archive.addfile(info, contents)

after_files = [record(RESEARCH / entry["path"]) for entry in source_files]
if after_files != source_files:
    raise SystemExit("source changed during capture")
source_manifest["stable_during_capture"] = True
write_json(manifest_path, source_manifest)
write_json(ARTIFACT / "changed-files.json", {"base": record(BASE), "changed": changed})

full_log = ARTIFACT / "final-full-test.log"
rows = []
pattern = re.compile(
    r"test result: (\w+)\. (\d+) passed; (\d+) failed; (\d+) ignored; "
    r"(\d+) measured; (\d+) filtered out"
)
for line in full_log.read_text(errors="replace").splitlines():
    match = pattern.search(line)
    if match:
        rows.append(
            {
                "result": match.group(1),
                "passed": int(match.group(2)),
                "failed": int(match.group(3)),
                "ignored": int(match.group(4)),
                "measured": int(match.group(5)),
                "filtered_out": int(match.group(6)),
            }
        )
test_summary = {
    "command": "CARGO_TARGET_DIR=/Users/ryuwon/Desktop/ryuwon-project/llm-gateway-research/product/target/native-final-integration-fix cargo test --locked",
    "result_rows": len(rows),
    "passed": sum(row["passed"] for row in rows),
    "failed": sum(row["failed"] for row in rows),
    "ignored": sum(row["ignored"] for row in rows),
    "measured": sum(row["measured"] for row in rows),
    "rows": rows,
}
if test_summary["result_rows"] != 17 or test_summary["passed"] != 370 or test_summary["failed"]:
    raise SystemExit("unexpected final Rust test totals")
write_json(ARTIFACT / "test-summary.json", test_summary)

protected_expected = {
    "product/artifacts/native-task6/final-doc-fix/source-manifest.json": "90527f2339e426cfd2e290c84e214f7ff58e8035dc1cc7ea473181dcc8d57108",
    "product/artifacts/native-task6/final-doc-fix/source.tar.gz": "2ac55075bcc246bdbd114aa654f1114098d13bd4976050575cefa3d08253ebe5",
    "product/artifacts/native-task6/final-v2/llmgw-macos-arm64": "cf7c436c442c426a6e9a1d485a5261fe5e5a05c8b131b130e679e00eec2c646b",
    "product/artifacts/native-task6/final-doc-fix/llmgw-macos-arm64.tar.gz": "823b6f6a36668bd20bc6c7754c4c68158971c85e1bd4c73a831fcdd1185d0df1",
    "evidence/native-final-integration-review/FINAL_HOLD.json": "9e1c4a30220fd447f708989dcf00dec25c1053a2ceb0f70a8cd504aceba053f4",
    "evidence/native-final-integration-review/HOLD-size-correction.json": "ae72208fa3c192c97afe2e675ba336ef8fe256fc4a10352030d1fbe384effd52",
    "evidence/native-final-integration-review/README.md": "e816962a2d2c895a345e59a0af84d6e0c754a147fdbd6daf0f3f50b677da4898",
    "evidence/native-final-integration-review/result.json": "49017ea5ccd39ae2d73ed7a2e2138a2e7bff1901b025fef90e0305f9e8de8b3c",
    "evidence/native-final-integration-review/source-evidence-checks.json": "d185c54ee7bea64579ca81af96e979f54b08508a96d4c156410e3ef5866dbeca",
    "evidence/native-task6-final-doc-parent-check.json": "68d2673d0e2f1ab9948077ca3af8ba24b12a951080caeba120795d7a11d960b3",
}
protected = []
for name, expected in protected_expected.items():
    item = record(RESEARCH / name)
    item["expected_sha256"] = expected
    item["unchanged"] = item["sha256"] == expected
    protected.append(item)
if not all(item["unchanged"] for item in protected):
    raise SystemExit("protected baseline or review evidence drifted")
write_json(
    ARTIFACT / "preservation-check.json",
    {
        "checked": len(protected),
        "unchanged": sum(item["unchanged"] for item in protected),
        "historical_parent_audit_rows_retained": 1680,
        "files": protected,
    },
)

processes = subprocess.run(
    ["ps", "-axo", "pid=,ppid=,command="], capture_output=True, text=True, check=True
).stdout.splitlines()
target_fragment = "/target/native-final-integration-fix/"
owned_workers = [line.strip() for line in processes if target_fragment in line and " worker " in line]
temporary = pathlib.Path("/private/var/folders")
leftovers = []
if temporary.exists():
    leftovers.extend(str(path) for path in temporary.glob("*/*/T/llmgw-setup-*") if path.exists())
    leftovers.extend(
        str(path)
        for path in temporary.glob("*/*/T/llmgw-client-profiles-fresh-save-only-*")
        if path.exists()
    )
cleanup = {
    "captured_at": datetime.datetime.now(datetime.timezone.utc).isoformat(),
    "target_worker_processes": owned_workers,
    "owned_workers_remaining": len(owned_workers),
    "owned_fixture_paths_remaining": sorted(leftovers),
    "test_harness_incident": {
        "owned_pid": 41127,
        "cause": "first post-fix test used the wrong expected Codex target path and panicked after activation",
        "cleanup": "proven-owned exact target/config argv received bounded SIGTERM after its Fixture had already removed authentication state",
        "evidence": "product/artifacts/native-final-integration-fix/owned-orphan-cleanup.log",
        "production_lifecycle_failure": False,
    },
}
if owned_workers or leftovers:
    raise SystemExit("owned runtime or fixture cleanup incomplete")
write_json(ARTIFACT / "cleanup-audit.json", cleanup)

result = {
    "phase": PHASE,
    "captured_at": captured_at,
    "status": "FINAL_HOLD",
    "passed": True,
    "finding": "I1",
    "behavior": {
        "final_setup_apply": "publishes config and pending metadata, then initializes protected config-scoped data/control tokens",
        "save_only": "starts no worker, changes no client or registration, preserves running workers and existing token bytes",
        "preview_cancel": "write-free",
        "partial_failure": "reports config applied/state initialization failed before worker start or client patch",
        "fresh_clients": "Pi, Claude Code, and Codex preview and reviewed restart/apply are reachable after actual setup SaveOnly without manual token seeding",
    },
    "source_files": len(source_files),
    "changed_paths": expected_changed,
    "tests": test_summary,
    "checks": {
        "fmt": "passed",
        "check_all_targets_locked": "passed",
        "clippy_all_targets_locked_deny_warnings": "passed",
        "release_locked": "passed",
    },
    "source_manifest": record(manifest_path),
    "source_archive": record(archive_path),
    "release_binary": record(ARTIFACT / "llmgw-macos-arm64"),
    "debug_binary": record(ARTIFACT / "llmgw-macos-arm64-debug"),
    "cleanup": cleanup,
    "limitations": [
        "final packaged same-binary real client observations are coordinated by the parent after fix review",
        "Linux and Windows runtime are unverified in this macOS-only fix run",
        "no real or paid upstream API, model process, user profile, client config, or OS registration was used",
    ],
    "execution_finished": True,
    "runtime_ownership_released": True,
}
write_json(ARTIFACT / "result.json", result)

readme = f"""# Native final integration I1 fix — FINAL_HOLD

Fresh setup SaveOnly now initializes the existing protected, config-scoped data
and control token files under lifecycle operation and worker locks. It starts no
worker and writes no client or OS registration. Preview and cancellation remain
write-free. Existing tokens are validated and remain byte-identical; missing or
invalid state beneath a running worker is never repaired.

The final Rust suite has {test_summary['passed']} passed, 0 failed across
{test_summary['result_rows']} result rows. Focused actual-binary tests exercised
Pi 0.84.2, Claude Code 2.1.63, and Codex 0.154.0 from fresh setup SaveOnly through
preview hash, reviewed restart, authenticated desired-fingerprint readiness,
client apply, disconnect, and authenticated off using isolated temporary homes.

One retained harness failure expected the Codex profile at `.codex/config.toml`
instead of its verified `.codex/llmgw.config.toml` path. The panic orphaned one
proven-owned debug worker after the old Fixture removed state; the exact process
received bounded SIGTERM and cleanup is recorded. The final harness preserves
state from activation until authenticated off plus parsed stopped status.

No performance/accounting matrix, real upstream, real client profile, login,
model process, OS registration, VCS, or prior artifact was changed. Final
packaged-client observations remain for the parent-coordinated rereview.
"""
(ARTIFACT / "README.md").write_text(readme)

identity = {
    "phase": PHASE,
    "captured_at": captured_at,
    "source_manifest": record(manifest_path),
    "source_archive": record(archive_path),
    "release_binary": record(ARTIFACT / "llmgw-macos-arm64"),
    "debug_binary": record(ARTIFACT / "llmgw-macos-arm64-debug"),
    "result": record(ARTIFACT / "result.json"),
    "readme": record(ARTIFACT / "README.md"),
    "capture_script": record(pathlib.Path(__file__).resolve()),
    "execution_finished": True,
    "runtime_ownership_released": True,
    "owned_workers_remaining": 0,
}
write_json(ARTIFACT / "identity.json", identity)

files = []
for path in sorted(ARTIFACT.iterdir()):
    if path.is_file() and path.name != "FINAL_HOLD.json":
        files.append(record(path))
hold = {
    "phase": PHASE,
    "captured_at": datetime.datetime.now(datetime.timezone.utc).isoformat(),
    "status": "FINAL_HOLD",
    "verdict": "I1_FIXED_READY_FOR_SAME_FINAL_INTEGRATION_REREVIEW",
    "passed": True,
    "do_not_overwrite": True,
    "source_files": len(source_files),
    "changed_paths": expected_changed,
    "source_manifest_sha256": sha(manifest_path),
    "source_archive_sha256": sha(archive_path),
    "release_binary_sha256": sha(ARTIFACT / "llmgw-macos-arm64"),
    "release_binary_bytes": (ARTIFACT / "llmgw-macos-arm64").stat().st_size,
    "debug_binary_sha256": sha(ARTIFACT / "llmgw-macos-arm64-debug"),
    "debug_binary_bytes": (ARTIFACT / "llmgw-macos-arm64-debug").stat().st_size,
    "rust_test_result_rows": test_summary["result_rows"],
    "rust_tests_passed": test_summary["passed"],
    "rust_tests_failed": test_summary["failed"],
    "protected_checks": len(protected),
    "protected_drift": 0,
    "owned_workers_remaining": 0,
    "owned_fixture_paths_remaining": 0,
    "execution_finished": True,
    "runtime_ownership_released": True,
    "files": files,
}
write_json(ARTIFACT / "FINAL_HOLD.json", hold)
