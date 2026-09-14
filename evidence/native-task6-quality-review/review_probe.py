#!/usr/bin/env python3
"""Read-only Task 6 quality audit plus one isolated package identity race probe."""

from __future__ import annotations

import hashlib
import importlib.util
import json
from pathlib import Path
import tarfile
import tempfile


RESEARCH = Path(__file__).resolve().parents[2]
PRODUCT = RESEARCH / "product"
CURRENT_MANIFEST = PRODUCT / "artifacts/native-task6/spec-doc-fix/source-manifest.json"
CURRENT_SOURCE_ARCHIVE = PRODUCT / "artifacts/native-task6/spec-doc-fix/source.tar.gz"
CURRENT_PACKAGE = PRODUCT / "artifacts/native-task6/spec-doc-fix/llmgw-macos-arm64.tar.gz"
TASK5_MANIFEST = PRODUCT / "artifacts/native-task5/quality-fix-v2/source-manifest-final.json"
PACKAGE_SCRIPT = PRODUCT / "scripts/package_native.py"


def digest(path: Path) -> str:
    return hashlib.sha256(path.read_bytes()).hexdigest()


def rows(path: Path) -> list[dict[str, object]]:
    return json.loads(path.read_text(encoding="utf-8"))["files"]


def verify_rows(items: list[dict[str, object]]) -> list[dict[str, object]]:
    drift = []
    for item in items:
        path = RESEARCH / str(item["path"])
        if not path.is_file():
            drift.append({"path": item["path"], "reason": "missing"})
            continue
        actual_bytes = path.stat().st_size
        actual_hash = digest(path)
        if actual_bytes != item["bytes"] or actual_hash != item["sha256"]:
            drift.append(
                {
                    "path": item["path"],
                    "expected_bytes": item["bytes"],
                    "actual_bytes": actual_bytes,
                    "expected_sha256": item["sha256"],
                    "actual_sha256": actual_hash,
                }
            )
    return drift


def current_source_audit() -> dict[str, object]:
    source_rows = rows(CURRENT_MANIFEST)
    archive_drift = []
    with tarfile.open(CURRENT_SOURCE_ARCHIVE, "r:gz") as archive:
        members = archive.getmembers()
        names = [member.name for member in members]
        archived = {member.name: archive.extractfile(member).read() for member in members if member.isfile()}
    for item in source_rows:
        name = str(item["path"])
        payload = archived.get(name)
        if payload is None:
            archive_drift.append({"path": name, "reason": "missing_from_archive"})
        elif len(payload) != item["bytes"] or hashlib.sha256(payload).hexdigest() != item["sha256"]:
            archive_drift.append({"path": name, "reason": "archive_manifest_mismatch"})
    return {
        "manifest_rows": len(source_rows),
        "manifest_sha256": digest(CURRENT_MANIFEST),
        "live_drift": verify_rows(source_rows),
        "archive_members": len(names),
        "archive_has_embedded_manifest": "source-manifest.json" in names,
        "archive_sha256": digest(CURRENT_SOURCE_ARCHIVE),
        "archive_drift": archive_drift,
    }


def package_audit() -> dict[str, object]:
    expected = [
        "llmgw",
        "README.md",
        "docs/installation.md",
        "docs/runtime-contract.md",
        "docs/client-compatibility.md",
    ]
    with tarfile.open(CURRENT_PACKAGE, "r:gz") as archive:
        members = archive.getmembers()
        names = [member.name for member in members]
        payloads = {member.name: archive.extractfile(member).read() for member in members}
    docs_equal = {
        name: payloads[name] == (PRODUCT / name).read_bytes()
        for name in expected[1:]
    }
    return {
        "members": names,
        "members_exact": names == expected and all(member.isfile() for member in members),
        "archive_sha256": digest(CURRENT_PACKAGE),
        "binary_bytes": len(payloads["llmgw"]),
        "binary_sha256": hashlib.sha256(payloads["llmgw"]).hexdigest(),
        "documents_equal_live": docs_equal,
    }


def task5_delta() -> dict[str, object]:
    before = {str(item["path"]): str(item["sha256"]) for item in rows(TASK5_MANIFEST)}
    after = {str(item["path"]): str(item["sha256"]) for item in rows(CURRENT_MANIFEST)}
    return {
        "added": sorted(after.keys() - before.keys()),
        "changed": sorted(path for path in after.keys() & before.keys() if after[path] != before[path]),
        "removed": sorted(before.keys() - after.keys()),
    }


def held_preservation() -> dict[str, object]:
    baseline = rows(PRODUCT / "artifacts/native-task6/spec-doc-fix/preservation-before.json")
    parent = json.loads((RESEARCH / "evidence/native-task6-parent-hold-check.json").read_text(encoding="utf-8"))
    doc_parent = json.loads((RESEARCH / "evidence/native-task6-spec-doc-parent-check.json").read_text(encoding="utf-8"))
    groups = {
        "prior_baseline": baseline,
        "final_v2_hold": parent["preserve_task6"],
        "final_build_extras": parent["preserve_final_build_extras"],
        "doc_fix_hold": doc_parent["preserve_doc_fix"],
    }
    result = {}
    for name, items in groups.items():
        result[name] = {"rows": len(items), "drift": verify_rows(items)}
    other_paths = [
        RESEARCH / "evidence/native-task6-parent-hold-check.json",
        RESEARCH / "evidence/native-task6-spec-doc-parent-check.json",
        *sorted((RESEARCH / "evidence/native-task6-spec-review-fallback").glob("*")),
        *sorted((RESEARCH / "evidence/native-task6-spec-review-fallback/rereview-s1").glob("*")),
    ]
    other_files = [path for path in other_paths if path.is_file()]
    result["review_and_parent_files"] = {
        "rows": len(other_files),
        "identities": [
            {
                "path": str(path.relative_to(RESEARCH)),
                "bytes": path.stat().st_size,
                "sha256": digest(path),
            }
            for path in other_files
        ],
    }
    return result


def package_identity_race_probe() -> dict[str, object]:
    spec = importlib.util.spec_from_file_location("task6_package_native", PACKAGE_SCRIPT)
    assert spec is not None and spec.loader is not None
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    with tempfile.TemporaryDirectory(prefix="llmgw-task6-quality-race-") as raw:
        root = Path(raw)
        product = root / "product"
        (product / "docs").mkdir(parents=True)
        for name in module.PACKAGE_MEMBERS[1:]:
            path = product / name
            path.parent.mkdir(parents=True, exist_ok=True)
            path.write_text(f"fixture {name}\n", encoding="utf-8")
        binary = root / "llmgw"
        before = b"binary-before-package-race"
        after = b"binary-after-package-race"
        binary.write_bytes(before)
        output = root / "llmgw-test.tar.gz"
        original_sha256 = module.sha256
        mutation_count = 0

        def mutate_between_reads(path: Path) -> str:
            nonlocal mutation_count
            result = original_sha256(path)
            if path == output:
                binary.write_bytes(after)
                mutation_count += 1
            return result

        module.sha256 = mutate_between_reads
        result = module.package_tar(binary, product, output)
        with tarfile.open(output, "r:gz") as archive:
            packaged = archive.extractfile("llmgw").read()
        packaged_hash = hashlib.sha256(packaged).hexdigest()
        reported_hash = str(result["binary_sha256"])
        return {
            "isolated_temp_removed_on_exit": True,
            "mutation_count": mutation_count,
            "packaged_bytes_are_before": packaged == before,
            "live_binary_bytes_are_after": binary.read_bytes() == after,
            "packaged_binary_sha256": packaged_hash,
            "reported_binary_sha256": reported_hash,
            "identity_mismatch_reproduced": packaged_hash != reported_hash,
        }


def cache_incident() -> dict[str, object]:
    affected = [
        PRODUCT / "scripts/__pycache__/measure_native.cpython-314.pyc",
        PRODUCT / "scripts/__pycache__/package_native.cpython-314.pyc",
        PRODUCT / "tests/__pycache__/test_installers.cpython-314.pyc",
        PRODUCT / "tests/__pycache__/test_measure_native.cpython-314.pyc",
        PRODUCT / "tests/__pycache__/test_package_native.cpython-314.pyc",
    ]
    baseline_paths = {
        str(item["path"])
        for item in rows(PRODUCT / "artifacts/native-task6/spec-doc-fix/preservation-before.json")
    }
    source_paths = {str(item["path"]) for item in rows(CURRENT_MANIFEST)}
    return {
        "command": "python3 -m py_compile on five Task6 Python sources/tests",
        "classification": "reviewer_error_ignored_unheld_cache_refresh",
        "deleted_or_restored": False,
        "files": [
            {
                "path": str(path.relative_to(RESEARCH)),
                "bytes": path.stat().st_size,
                "sha256": digest(path),
                "in_source_manifest": str(path.relative_to(RESEARCH)) in source_paths,
                "in_prior_preservation": str(path.relative_to(RESEARCH)) in baseline_paths,
            }
            for path in affected
        ],
    }


def main() -> int:
    result = {
        "source": current_source_audit(),
        "package": package_audit(),
        "task5_delta": task5_delta(),
        "preservation": held_preservation(),
        "targeted_package_identity_race": package_identity_race_probe(),
        "review_incident": cache_incident(),
    }
    print(json.dumps(result, indent=2))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
