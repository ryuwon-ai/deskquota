#!/usr/bin/env python3
import datetime
import hashlib
import json
import pathlib
import platform
import re
import subprocess
import tarfile
import tempfile


OUT = pathlib.Path(__file__).resolve().parent
PRODUCT = OUT.parents[1]
RESEARCH = PRODUCT.parent
BASE = PRODUCT / "artifacts/native-task2/quality-fix"
PROTECTED_BASE = RESEARCH / "evidence/native-task2-quality-review/rereview/after.json"
BENCHMARK_RAW_BASE = (
    RESEARCH / "evidence/native-task1-quality-review/pilot-preservation.json"
)
ACCOUNTING_RAW_BASE = RESEARCH / "evidence/accounting-task2-quality-review/hashes-after.json"


def digest(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def identity(path, shown=None):
    return {
        "path": shown or str(path.relative_to(PRODUCT)),
        "bytes": path.stat().st_size,
        "sha256": digest(path),
    }


capture = json.loads((OUT / "capture-summary.json").read_text())
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

source_paths = [item["path"] for item in manifest["files"]]
with tarfile.open(OUT / "source-hold.tar.gz", "r:gz") as archive:
    members = archive.getmembers()
    if [member.name for member in members] != source_paths:
        raise SystemExit("source archive member list differs from manifest")
    for member, item in zip(members, manifest["files"]):
        archived = archive.extractfile(member).read()
        if len(archived) != item["bytes"] or hashlib.sha256(archived).hexdigest() != item["sha256"]:
            raise SystemExit(f"source archive content mismatch: {member.name}")

expected_changed = {
    "product/Cargo.lock",
    "product/Cargo.toml",
    "product/src/config_patch/apply.rs",
    "product/src/config_patch/document.rs",
    "product/src/config_patch/journal.rs",
    "product/src/config_patch/mod.rs",
    "product/src/config_patch/restore.rs",
    "product/src/config_patch/storage.rs",
    "product/src/file_replace.rs",
    "product/src/lib.rs",
    "product/src/setup/persist.rs",
    "product/tests/patch_contract.rs",
}
changed = json.loads((OUT / "changed-files.json").read_text())["changed_files"]
if {item["path"] for item in changed} != expected_changed:
    raise SystemExit(f"unexpected changed paths: {changed}")

protected = json.loads(PROTECTED_BASE.read_text())["protected"]
protected_drift = []
protected_paths = set()
for item in protected:
    protected_paths.add(item["path"])
    path = RESEARCH / item["path"]
    if not path.is_file():
        protected_drift.append({"path": item["path"], "reason": "missing"})
    elif path.stat().st_size != item["bytes"] or digest(path) != item["sha256"]:
        protected_drift.append({"path": item["path"], "reason": "content changed"})
if protected_drift:
    raise SystemExit(f"protected artifact drift: {protected_drift}")

benchmark_raws = sorted((PRODUCT / "artifacts/pilot-runs").glob("*.json"))
accounting_raws = sorted(
    (PRODUCT / "artifacts/accounting-ablation/pilot-runs").glob("*.json")
)
if len(benchmark_raws) != 40 or len(accounting_raws) != 10:
    raise SystemExit(
        f"raw fixture count changed: benchmark={len(benchmark_raws)}, accounting={len(accounting_raws)}"
    )
benchmark_raw_hashes = {
    item["path"]: item["sha256"] for item in json.loads(BENCHMARK_RAW_BASE.read_text())
}
accounting_raw_hashes = json.loads(ACCOUNTING_RAW_BASE.read_text())
for path, hashes in [
    *((path, benchmark_raw_hashes) for path in benchmark_raws),
    *((path, accounting_raw_hashes) for path in accounting_raws),
]:
    relative = str(path.relative_to(RESEARCH))
    if relative not in hashes or digest(path) != hashes[relative]:
        raise SystemExit(f"raw fixture drift: {relative}")

(OUT / "protected-check.json").write_text(
    json.dumps(
        {
            "baseline": str(PROTECTED_BASE.relative_to(RESEARCH)),
            "checked": len(protected),
            "drift": protected_drift,
            "benchmark_raws": len(benchmark_raws),
            "accounting_raws": len(accounting_raws),
            "benchmark_raw_baseline": str(BENCHMARK_RAW_BASE.relative_to(RESEARCH)),
            "accounting_raw_baseline": str(ACCOUNTING_RAW_BASE.relative_to(RESEARCH)),
        },
        indent=2,
    )
    + "\n"
)


def test_rows(name):
    return [
        int(match.group(1))
        for match in re.finditer(
            r"test result: ok\. (\d+) passed; 0 failed;", (OUT / name).read_text()
        )
    ]


full_rows = test_rows("cargo-test-full-final.log")
if len(full_rows) != 15 or sum(full_rows) != 283:
    raise SystemExit(f"unexpected final full-test totals: {full_rows}")
focused_rows = test_rows("patch-contract-final.log")
release_rows = test_rows("patch-contract-release-final.log")
if focused_rows != [19] or release_rows != [19]:
    raise SystemExit(
        f"unexpected patch-test totals: debug={focused_rows}, release={release_rows}"
    )
for name in (
    "cargo-check-all-targets-final.log",
    "cargo-clippy-all-targets-final.log",
    "cargo-build-release-final.log",
):
    if "Finished" not in (OUT / name).read_text():
        raise SystemExit(f"final command log is incomplete: {name}")
if (OUT / "cargo-fmt-final.log").read_text():
    raise SystemExit("final rustfmt check produced unexpected output")

sentinels = (
    "".join(("upstream-", "synthetic-", "8f2f5a47")),
    "".join(("local-data-", "synthetic-", "3dcb99a1")),
    "".join(("control-", "synthetic-", "6a01c442")),
)
sentinel_hits = []
for path in OUT.glob("*.log"):
    text = path.read_text(errors="replace")
    for sentinel in sentinels:
        if sentinel in text:
            sentinel_hits.append({"path": path.name, "sentinel": sentinel})
if sentinel_hits:
    raise SystemExit(f"synthetic credential leaked into logs: {sentinel_hits}")

process_text = subprocess.run(
    ["ps", "-axo", "pid=,ppid=,command="], capture_output=True, text=True, check=True
).stdout
owned_processes = [
    line.strip()
    for line in process_text.splitlines()
    if "target/native-task3/" in line
    and ("/llmgw" in line or "patch_contract-" in line)
]
temp_root = pathlib.Path(tempfile.gettempdir())
temp_paths = sorted(
    {
        str(path)
        for pattern in (
            "llmgw-patch-*",
            "llmgw-windows-*",
            "llmgw-config-patch-locks-*",
        )
        for path in temp_root.glob(pattern)
    }
)
cleanup = {
    "captured_at": datetime.datetime.now(datetime.timezone.utc).isoformat(),
    "owned_processes": owned_processes,
    "matching_temp_paths": temp_paths,
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

release = PRODUCT / "target/native-task3/release/llmgw"
debug = PRODUCT / "target/native-task3/debug/llmgw"
rustc = subprocess.run(
    ["rustc", "--version", "--verbose"], capture_output=True, text=True, check=True
).stdout.strip()
cargo = subprocess.run(
    ["cargo", "--version", "--verbose"], capture_output=True, text=True, check=True
).stdout.strip()

final = {
    "phase": "FINAL_HOLD",
    "execution_finished": True,
    "writer_runtime_ownership_released": True,
    "source_stable_after_capture": True,
    "source_drift": source_drift,
    **capture,
    "binaries": {
        "release": identity(release),
        "debug": identity(debug),
    },
    "lockfile": identity(PRODUCT / "Cargo.lock"),
    "toolchain_file": identity(PRODUCT / "rust-toolchain.toml"),
    "verification": {
        "full_debug": {
            "passed": 283,
            "failed": 0,
            "rows": full_rows,
            "log": "artifacts/native-task3/cargo-test-full-final.log",
        },
        "patch_debug": {"passed": 19, "failed": 0},
        "patch_release": {"passed": 19, "failed": 0},
        "setup_contract": {"passed": 42, "failed": 0},
        "setup_persist_shared_helper": {"passed": 5, "failed": 0},
        "static": {
            "rustfmt": "passed",
            "check_all_targets": "passed",
            "clippy_all_targets_deny_warnings": "passed",
            "release_build": "passed",
        },
    },
    "red_green": {
        "initial_red": "artifacts/native-task3/patch-contract-red.log",
        "nested_duplicate_red": "artifacts/native-task3/nested-duplicate-red.log",
        "restore_partial_red": "artifacts/native-task3/restore-partial-red.log",
        "final_green": "artifacts/native-task3/patch-contract-final.log",
        "release_green": "artifacts/native-task3/patch-contract-release-final.log",
        "misleading_failed_intermediate_names": [
            "artifacts/native-task3/json-boundary-green.log",
            "artifacts/native-task3/patch-contract-final-focused.log",
            "artifacts/native-task3/restore-partial-green.log",
        ],
    },
    "platform_evidence": {
        "macos_runtime": [
            "0600 token target enforcement without chmod",
            "ordinary mode and extended ACL preservation",
            "partial apply and partial restore recovery",
            "bounded child-process interruption and journal recovery",
        ],
        "windows": [
            "static cfg review of current-user DACL validation and ReplaceFileW path",
            "three shared replacement-helper failure-model tests executed on macOS",
            "no Windows host runtime claim",
        ],
        "linux": "static cfg review only; no Linux host runtime claim",
    },
    "protected": {
        "baseline": str(PROTECTED_BASE.relative_to(RESEARCH)),
        "checked": len(protected),
        "drift": protected_drift,
        "benchmark_raws": len(benchmark_raws),
        "accounting_raws": len(accounting_raws),
        "task2_source_manifest_sha256": digest(BASE / "source-manifest.json"),
        "task2_source_archive_sha256": digest(BASE / "source-hold.tar.gz"),
        "task2_final_hold_sha256": digest(BASE / "final-hold.json"),
        "task2_release": identity(PRODUCT / "target/native-task2-quality-fix/release/llmgw"),
        "task2_debug": identity(PRODUCT / "target/native-task2-quality-fix/debug/llmgw"),
        "accounting_held": identity(PRODUCT / "artifacts/accounting-ablation/task2/held-llmgw"),
        "native_held": identity(PRODUCT / "target/native/release/llmgw"),
    },
    "dependency": {
        "added": "jsonc-parser 0.33.1",
        "features": ["cst", "serde_json"],
        "default_features": False,
        "lock_change": "one direct package added; no retained package version changed",
    },
    "host": platform.platform(),
    "rustc": rustc,
    "cargo": cargo,
    "cleanup": "artifacts/native-task3/cleanup-final.json",
    "limits": [
        "ConfigPatch is a library foundation; Task4 client adapters and CLI connect/disconnect wiring were not implemented",
        "no real user client/auth file, real credential, external message, login, service registration, paid endpoint, model start/download, Git action, benchmark, or accounting rerun",
        "external editors may still race the final metadata/hash check and atomic rename; no filesystem CAS is claimed",
    ],
}
(OUT / "final-hold.json").write_text(json.dumps(final, indent=2) + "\n")

readme = f"""# Native Task 3 ConfigPatch FINAL_HOLD

Status: **FINAL_HOLD**. `execution_finished=true`; writer/runtime ownership is released for fresh SPEC review.

ConfigPatch now provides reviewed JSON, comments-only JSON, and TOML edits with protected before-images, stageful raw-free journals, per-resource locks, same-directory replacement, immediate snapshot and permission/ACL rechecks, exact preview-hash enforcement, and value-aware disconnect restoration. Repeated connect preserves the first pre-ownership value. Partial apply, interrupted apply, and partial restore retain explicit recovery state. Existing setup behavior is unchanged; only its Windows ambiguous-replacement helper moved into the shared narrow storage module.

Direct final verification: **19/19** focused debug contracts, **19/19** optimized contracts, and **283/283** full debug tests. Rustfmt, `cargo check --all-targets --locked`, `cargo clippy --all-targets --locked -- -D warnings`, and `cargo build --release --locked` passed. macOS runtime fixtures covered permissions, ACLs, partial writes/restores, and child interruption. Windows evidence is static plus three shared replacement failure-model tests executed on macOS; there is no Windows runtime claim.

Source: {capture['source_files']} files; {capture['changed_files']} changed from the accepted 71-file Task 2 baseline. Manifest `{capture['source_manifest_sha256']}`; archive `{capture['source_archive_sha256']}` ({capture['source_archive_bytes']} bytes). Release `{digest(release)}` ({release.stat().st_size} bytes); debug `{digest(debug)}` ({debug.stat().st_size} bytes); Cargo.lock `{digest(PRODUCT / 'Cargo.lock')}`.

All {len(protected)} prior protected artifacts matched the accepted rereview manifest, including 40 benchmark raw event files and 10 accounting raw event files. No owned Task 3 process or temporary test/lock path remains. Synthetic upstream, local-data, and control-token sentinels were absent from every Task 3 log.

Earlier failed attempts are preserved. In particular, `json-boundary-green.log`, `patch-contract-final-focused.log`, and `restore-partial-green.log` are failed intermediate runs despite their filenames. Canonical final evidence is `patch-contract-final.log`, `cargo-test-full-final.log`, `patch-contract-release-final.log`, and `final-hold.json`.
"""
(OUT / "README.md").write_text(readme)
(OUT / "FINAL_HOLD").write_text(
    "FINAL_HOLD\nexecution_finished=true\nwriter_runtime_ownership_released=true\n"
)
print(json.dumps(final, indent=2))
