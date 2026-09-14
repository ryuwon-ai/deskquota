#!/usr/bin/env python3
"""Bounded read-only Native Task 6 final SPEC confirmation."""

from __future__ import annotations

from datetime import datetime, timezone
import hashlib
import json
from pathlib import Path
import tarfile


RESEARCH = Path(__file__).resolve().parents[3]
PRODUCT = RESEARCH / "product"
FIX = PRODUCT / "artifacts/native-task6/quality-fix"
DOC_FIX = PRODUCT / "artifacts/native-task6/spec-doc-fix"
FINAL_V2 = PRODUCT / "artifacts/native-task6/final-v2"
OUT = Path(__file__).resolve().parent
EXPECTED = {
    "manifest": "d00ff4d106cd374857d4738441c3406a8085e8344d3c52e845f81a82763a88c7",
    "source_archive": "0d8897fcd9300f433740f3fbd24edc042f8b7706e18715b50c5fc8a90de3ec2b",
    "hold": "79eb12f4184f37597baf0d7897b61fe8d6c0aaf89300acdff75faf4062276c2a",
    "package": "ba03203e9416a4eb75ca972fb0b3e5eba8618c46b2c26c7f65e2b7ffff387a22",
    "binary": "cf7c436c442c426a6e9a1d485a5261fe5e5a05c8b131b130e679e00eec2c646b",
    "quality_rereview_hold": "682f13f1cedb9f88b42e30630c80ea0756a8a13672b5b91a032c78e283aa4fe4",
    "original_quality_hold": "c0b26468b3e94adf6ab801fb81b29ee04f8a250993e3a1f490e899676631205e",
}
PACKAGE_MEMBERS = (
    "llmgw",
    "README.md",
    "docs/installation.md",
    "docs/runtime-contract.md",
    "docs/client-compatibility.md",
)


def sha_bytes(data: bytes) -> str:
    return hashlib.sha256(data).hexdigest()


def sha256(path: Path) -> str:
    return sha_bytes(path.read_bytes())


def compare(rows: list[dict[str, object]], root: Path) -> list[dict[str, object]]:
    drift = []
    for expected in rows:
        path = root / str(expected["path"])
        if not path.is_file():
            drift.append({"path": expected["path"], "error": "missing_or_not_regular"})
            continue
        actual = {"bytes": path.stat().st_size, "sha256": sha256(path)}
        if actual["bytes"] != expected["bytes"] or actual["sha256"] != expected["sha256"]:
            drift.append({"path": expected["path"], "expected": expected, "actual": actual})
    return drift


manifest_path = FIX / "source-manifest.json"
manifest = json.loads(manifest_path.read_text(encoding="utf-8"))
rows = manifest["files"]
live_drift = compare(rows, RESEARCH)
old_manifest = json.loads((DOC_FIX / "source-manifest.json").read_text(encoding="utf-8"))
old = {item["path"]: item for item in old_manifest["files"]}
new = {item["path"]: item for item in rows}
changed = sorted(path for path in old.keys() & new.keys() if old[path]["bytes"] != new[path]["bytes"] or old[path]["sha256"] != new[path]["sha256"])
added = sorted(new.keys() - old.keys())
removed = sorted(old.keys() - new.keys())

source_errors: list[str] = []
source_archive_path = FIX / "source.tar.gz"
with tarfile.open(source_archive_path, "r:gz") as archive:
    members = archive.getmembers()
    names = [member.name for member in members]
    expected_names = [str(item["path"]) for item in rows] + ["source-manifest.json"]
    if len(names) != 102 or len(names) != len(set(names)) or set(names) != set(expected_names):
        source_errors.append("source_member_set_mismatch")
    if any(not member.isfile() for member in members):
        source_errors.append("source_non_regular_member")
    embedded = archive.extractfile("source-manifest.json")
    if embedded is None or embedded.read() != manifest_path.read_bytes():
        source_errors.append("embedded_manifest_mismatch")
    by_name = {member.name: member for member in members}
    for expected_row in rows:
        member = by_name.get(str(expected_row["path"]))
        stream = archive.extractfile(member) if member is not None else None
        data = stream.read() if stream is not None else None
        if data is None or len(data) != expected_row["bytes"] or sha_bytes(data) != expected_row["sha256"]:
            source_errors.append(f"source_content_mismatch:{expected_row['path']}")

package_path = FIX / "llmgw-macos-arm64.tar.gz"
package_errors: list[str] = []
with tarfile.open(package_path, "r:gz") as archive:
    members = archive.getmembers()
    names = [member.name for member in members]
    if names != list(PACKAGE_MEMBERS) or len(names) != len(set(names)):
        package_errors.append("package_member_mismatch")
    if any(not member.isfile() for member in members):
        package_errors.append("package_non_regular_member")
    package_payloads = {}
    for member in members:
        stream = archive.extractfile(member)
        data = stream.read() if stream is not None else b""
        package_payloads[member.name] = data
        expected_path = FINAL_V2 / "llmgw-macos-arm64" if member.name == "llmgw" else PRODUCT / member.name
        if data != expected_path.read_bytes():
            package_errors.append(f"package_content_mismatch:{member.name}")

install_text = (PRODUCT / "packaging/install.ps1").read_text(encoding="utf-8")
catch_start = install_text.index("            $replaceError = $_.Exception.Message")
catch_end = install_text.index("    } else {", catch_start)
catch = install_text[catch_start:catch_end]
package_text = (PRODUCT / "scripts/package_native.py").read_text(encoding="utf-8")
doc_text = " ".join((PRODUCT / "docs/installation.md").read_text(encoding="utf-8").split())
contracts = {
    "q1_hashes_captured_member": '"binary_sha256": hashlib.sha256(contents["llmgw"]).hexdigest()' in package_text,
    "q1_archive_writes_same_captured_member": "contents: dict[str, bytes] = {\"llmgw\": binary.read_bytes()}" in package_text and "archive.addfile(entry, io.BytesIO(contents[name]))" in package_text,
    "q2_reports_original_error_and_three_paths": "replace_failed_recovery_unconfirmed: $replaceError; target=$target; backup=$backup; candidate=$candidate" in catch,
    "q2_catch_has_no_copy": "[System.IO.File]::Copy" not in catch,
    "q2_catch_has_no_move": "[System.IO.File]::Move" not in catch,
    "q2_catch_has_no_remove": "Remove-Item" not in catch,
    "q2_catch_keeps_preserve_flag": "$preserveRecovery = $false" not in catch,
    "q2_script_never_removes_target": "Remove-Item -LiteralPath $target" not in install_text,
    "q2_finally_cleanup_is_guarded": "if (-not $preserveRecovery -and $null -ne $candidate)" in install_text and "if (-not $preserveRecovery -and $null -ne $backup)" in install_text,
    "s1_stop_and_preserve_still_documented": "If the installer reports `replace_failed_recovery_unconfirmed`, stop and keep the reported target, destination-local `.llmgw-backup-*`, and candidate files" in doc_text,
    "s1_manual_identity_control_still_documented": "confirm that the target is still absent, no other installer or process is writing it, and the backup matches the known previous binary by its trusted checksum or release identity" in doc_text,
    "s1_no_blind_overwrite_still_documented": "Never blindly overwrite an existing target" in doc_text,
    "windows_still_unverified": "Windows execution, Authenticode, SmartScreen, and user-PATH behavior remain unverified" in doc_text,
}

parent_path = RESEARCH / "evidence/native-task6-quality-fix-parent-check.json"
parent = json.loads(parent_path.read_text(encoding="utf-8"))
quality_fix_drift = compare(parent["preserve_quality_fix"], RESEARCH)
quality_rereview_dir = RESEARCH / "evidence/native-task6-quality-review/rereview-q1-q2"
quality_rereview_hold_path = quality_rereview_dir / "FINAL_HOLD.json"
quality_rereview_hold = json.loads(quality_rereview_hold_path.read_text(encoding="utf-8"))
quality_rereview_drift = compare(quality_rereview_hold["files"], RESEARCH)
original_quality_hold_path = RESEARCH / "evidence/native-task6-quality-review/FINAL_HOLD.json"
targeted_log = (FIX / "targeted-final-green-v2.log").read_text(encoding="utf-8")

result = {
    "at": datetime.now(timezone.utc).isoformat(),
    "verdict": "SPEC_PASS",
    "source_files": len(rows),
    "source_manifest_sha256": sha256(manifest_path),
    "source_live_drift": live_drift,
    "source_archive_members": 102,
    "source_archive_sha256": sha256(source_archive_path),
    "source_archive_errors": source_errors,
    "changed_from_spec_doc_fix": changed,
    "added_from_spec_doc_fix": added,
    "removed_from_spec_doc_fix": removed,
    "package_members": len(PACKAGE_MEMBERS),
    "package_bytes": package_path.stat().st_size,
    "package_sha256": sha256(package_path),
    "package_errors": package_errors,
    "package_byte_identical_to_spec_doc_fix": package_path.read_bytes() == (DOC_FIX / "llmgw-macos-arm64.tar.gz").read_bytes(),
    "packaged_binary_sha256": sha_bytes(package_payloads["llmgw"]),
    "final_v2_binary_sha256": sha256(FINAL_V2 / "llmgw-macos-arm64"),
    "quality_fix_hold_sha256": sha256(FIX / "FINAL_HOLD.json"),
    "contracts": contracts,
    "quality_fix_hold_rows": len(parent["preserve_quality_fix"]),
    "quality_fix_hold_drift": quality_fix_drift,
    "historical_preservation_from_parent": {
        "rows": parent["prior_files"],
        "drift": parent["drift"],
        "parent_check_sha256": sha256(parent_path),
        "used_without_recollecting_1680_rows": True,
    },
    "quality_rereview": {
        "verdict": quality_rereview_hold["verdict"],
        "remaining_findings": quality_rereview_hold["remaining_findings"],
        "hold_sha256": sha256(quality_rereview_hold_path),
        "own_rows": len(quality_rereview_hold["files"]),
        "own_drift": quality_rereview_drift,
        "runtime_ownership_released": quality_rereview_hold["runtime_ownership_released"],
    },
    "original_quality_hold_sha256": sha256(original_quality_hold_path),
    "focused_test_log_retained": {
        "sha256": sha256(FIX / "targeted-final-green-v2.log"),
        "reports_five_passes": "Ran 5 tests" in targeted_log and "OK" in targeted_log,
        "freshly_rerun_by_this_confirmation": False,
    },
    "runtime_or_cargo_executed": False,
    "windows_runtime_verified": False,
}
result["passed"] = (
    len(rows) == 101 and not live_drift
    and result["source_manifest_sha256"] == EXPECTED["manifest"]
    and result["source_archive_sha256"] == EXPECTED["source_archive"]
    and not source_errors
    and changed == [
        "product/packaging/install.ps1",
        "product/scripts/package_native.py",
        "product/tests/test_installers.py",
        "product/tests/test_package_native.py",
    ]
    and not added and not removed
    and result["package_members"] == 5 and result["package_bytes"] == 4_202_653
    and result["package_sha256"] == EXPECTED["package"] and not package_errors
    and result["package_byte_identical_to_spec_doc_fix"]
    and result["packaged_binary_sha256"] == EXPECTED["binary"]
    and result["final_v2_binary_sha256"] == EXPECTED["binary"]
    and result["quality_fix_hold_sha256"] == EXPECTED["hold"]
    and all(contracts.values())
    and len(parent["preserve_quality_fix"]) == 17 and not quality_fix_drift
    and parent["passed"] is True and parent["prior_files"] == 1680 and parent["drift"] == 0
    and result["quality_rereview"]["verdict"] == "PASS"
    and result["quality_rereview"]["remaining_findings"] == 0
    and result["quality_rereview"]["hold_sha256"] == EXPECTED["quality_rereview_hold"]
    and not quality_rereview_drift and result["quality_rereview"]["runtime_ownership_released"] is True
    and result["original_quality_hold_sha256"] == EXPECTED["original_quality_hold"]
    and result["focused_test_log_retained"]["reports_five_passes"]
)
with (OUT / "result.json").open("x", encoding="utf-8") as stream:
    json.dump(result, stream, indent=2)
    stream.write("\n")
print(json.dumps({"passed": result["passed"], "contracts": len(contracts)}))
raise SystemExit(0 if result["passed"] else 1)
