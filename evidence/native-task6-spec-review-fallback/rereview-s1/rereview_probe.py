#!/usr/bin/env python3
"""Read-only Native Task 6 S1 documentation rereview identity probe."""

from __future__ import annotations

import argparse
from datetime import datetime, timezone
import hashlib
import json
from pathlib import Path
import tarfile


RESEARCH = Path(__file__).resolve().parents[3]
PRODUCT = RESEARCH / "product"
FINAL = PRODUCT / "artifacts/native-task6/final-v2"
FIX = PRODUCT / "artifacts/native-task6/spec-doc-fix"
OUT = Path(__file__).resolve().parent
EXPECTED = {
    "hold": "608022e67da4d18ce08e140faf4e9cee79273a636a1d422f00cb666eecfdd3f3",
    "manifest": "f228071555f45c1774b7c2f8e059f2adce6eef77c10674191e04713feb0d4c3f",
    "source_archive": "b8b2843f9dfdbeb18ce4368af9609f9a087bc95d171cc9324d4f968ab26d383f",
    "package": "ba03203e9416a4eb75ca972fb0b3e5eba8618c46b2c26c7f65e2b7ffff387a22",
    "binary": "cf7c436c442c426a6e9a1d485a5261fe5e5a05c8b131b130e679e00eec2c646b",
}
PACKAGE_MEMBERS = (
    "llmgw",
    "README.md",
    "docs/installation.md",
    "docs/runtime-contract.md",
    "docs/client-compatibility.md",
)


def digest_bytes(data: bytes) -> str:
    return hashlib.sha256(data).hexdigest()


def sha256(path: Path) -> str:
    return digest_bytes(path.read_bytes())


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


def snapshot(label: str) -> dict[str, object]:
    manifest_path = FIX / "source-manifest.json"
    manifest = json.loads(manifest_path.read_text(encoding="utf-8"))
    rows = manifest["files"]
    live_drift = compare(rows, RESEARCH)

    old_manifest = json.loads((FINAL / "source-manifest.json").read_text(encoding="utf-8"))
    old = {item["path"]: item for item in old_manifest["files"]}
    new = {item["path"]: item for item in rows}
    changed = sorted(path for path in old.keys() & new.keys() if old[path]["sha256"] != new[path]["sha256"] or old[path]["bytes"] != new[path]["bytes"])
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
            if data is None or len(data) != expected_row["bytes"] or digest_bytes(data) != expected_row["sha256"]:
                source_errors.append(f"source_content_mismatch:{expected_row['path']}")

    package_errors: list[str] = []
    package_path = FIX / "llmgw-macos-arm64.tar.gz"
    with tarfile.open(package_path, "r:gz") as archive:
        members = archive.getmembers()
        names = [member.name for member in members]
        if names != list(PACKAGE_MEMBERS) or len(names) != len(set(names)):
            package_errors.append("package_member_mismatch")
        if any(not member.isfile() for member in members):
            package_errors.append("package_non_regular_member")
        for member in members:
            stream = archive.extractfile(member)
            data = stream.read() if stream is not None else b""
            expected_path = FINAL / "llmgw-macos-arm64" if member.name == "llmgw" else PRODUCT / member.name
            if data != expected_path.read_bytes():
                package_errors.append(f"package_content_mismatch:{member.name}")
        packaged_binary = archive.extractfile("llmgw")
        packaged_binary_sha = digest_bytes(packaged_binary.read()) if packaged_binary is not None else None

    baseline = json.loads((FIX / "preservation-before.json").read_text(encoding="utf-8"))
    baseline_drift = compare(baseline["files"], RESEARCH)
    previous_review_rows = [item for item in baseline["files"] if str(item["path"]).startswith("evidence/native-task6-spec-review-fallback/")]
    previous_review_names = {str(item["path"]) for item in previous_review_rows}

    parent_path = RESEARCH / "evidence/native-task6-spec-doc-parent-check.json"
    parent = json.loads(parent_path.read_text(encoding="utf-8"))
    doc_fix_drift = compare(parent["preserve_doc_fix"], RESEARCH)

    checks = {
        "source_files": len(rows),
        "source_live_drift": live_drift,
        "changed_paths": changed,
        "added_paths": added,
        "removed_paths": removed,
        "source_manifest_sha256": sha256(manifest_path),
        "source_archive_members": 102,
        "source_archive_sha256": sha256(source_archive_path),
        "source_archive_errors": source_errors,
        "package_members": 5,
        "package_sha256": sha256(package_path),
        "package_errors": package_errors,
        "packaged_binary_sha256": packaged_binary_sha,
        "final_v2_binary_sha256": sha256(FINAL / "llmgw-macos-arm64"),
        "doc_fix_hold_sha256": sha256(FIX / "FINAL_HOLD.json"),
        "baseline_rows": len(baseline["files"]),
        "baseline_drift": baseline_drift,
        "previous_review_rows": len(previous_review_rows),
        "previous_review_final_hold_present": "evidence/native-task6-spec-review-fallback/FINAL_HOLD.json" in previous_review_names,
        "doc_fix_hold_rows": len(parent["preserve_doc_fix"]),
        "doc_fix_hold_drift": doc_fix_drift,
        "parent_check_sha256": sha256(parent_path),
        "parent_check_passed": parent["passed"],
        "parent_disclosed_initial_wrong_base_failure": bool(parent.get("prior_parent_attempt")),
    }
    passed = (
        not live_drift
        and changed == ["product/docs/installation.md"]
        and not added and not removed
        and not source_errors and not package_errors
        and checks["source_manifest_sha256"] == EXPECTED["manifest"]
        and checks["source_archive_sha256"] == EXPECTED["source_archive"]
        and checks["package_sha256"] == EXPECTED["package"]
        and packaged_binary_sha == EXPECTED["binary"]
        and checks["final_v2_binary_sha256"] == EXPECTED["binary"]
        and checks["doc_fix_hold_sha256"] == EXPECTED["hold"]
        and len(baseline["files"]) == 1646 and not baseline_drift
        and len(previous_review_rows) == 17 and checks["previous_review_final_hold_present"]
        and len(parent["preserve_doc_fix"]) == 12 and not doc_fix_drift
        and parent["passed"] is True and parent["drift"] == 0
    )
    return {"at": datetime.now(timezone.utc).isoformat(), "label": label, "passed": passed, **checks}


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("label", choices=("before", "after"))
    args = parser.parse_args()
    payload = snapshot(args.label)
    output = OUT / f"identity-{args.label}.json"
    with output.open("x", encoding="utf-8") as stream:
        json.dump(payload, stream, indent=2)
        stream.write("\n")
    print(json.dumps({"label": args.label, "passed": payload["passed"]}))
    return 0 if payload["passed"] else 1


if __name__ == "__main__":
    raise SystemExit(main())
