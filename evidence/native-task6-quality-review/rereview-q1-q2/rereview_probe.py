#!/usr/bin/env python3
"""Focused read-only rereview for Native Task 6 quality findings Q1 and Q2."""

from __future__ import annotations

import hashlib
import importlib.util
import json
from pathlib import Path
import tarfile
import tempfile


RESEARCH = Path(__file__).resolve().parents[3]
PRODUCT = RESEARCH / "product"
PACKAGE_SCRIPT = PRODUCT / "scripts/package_native.py"
INSTALLER = PRODUCT / "packaging/install.ps1"


def sha256(path: Path) -> str:
    return hashlib.sha256(path.read_bytes()).hexdigest()


def q1_interleaving() -> dict[str, object]:
    spec = importlib.util.spec_from_file_location("quality_fix_package_native", PACKAGE_SCRIPT)
    assert spec is not None and spec.loader is not None
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    with tempfile.TemporaryDirectory(prefix="llmgw-task6-quality-rereview-") as raw:
        root = Path(raw)
        fixture_product = root / "product"
        (fixture_product / "docs").mkdir(parents=True)
        for name in module.PACKAGE_MEMBERS[1:]:
            path = fixture_product / name
            path.parent.mkdir(parents=True, exist_ok=True)
            path.write_text(f"fixture {name}\n", encoding="utf-8")
        binary = root / "llmgw"
        captured = b"captured-archive-member"
        replacement = b"concurrent-path-replacement"
        binary.write_bytes(captured)
        output = root / "llmgw-rereview.tar.gz"
        original_sha = module.sha256
        replaced = False

        def replace_after_archive_hash(path: Path) -> str:
            nonlocal replaced
            result = original_sha(path)
            if path == output and not replaced:
                binary.write_bytes(replacement)
                replaced = True
            return result

        module.sha256 = replace_after_archive_hash
        try:
            result = module.package_tar(binary, fixture_product, output)
        finally:
            module.sha256 = original_sha
        with tarfile.open(output, "r:gz") as archive:
            member = archive.extractfile("llmgw").read()
        member_hash = hashlib.sha256(member).hexdigest()
        passed = (
            replaced
            and member == captured
            and binary.read_bytes() == replacement
            and result["binary_sha256"] == member_hash
            and result["binary_sha256"] != hashlib.sha256(replacement).hexdigest()
        )
        return {
            "passed": passed,
            "path_replaced_after_archive_hash": replaced,
            "archive_member_sha256": member_hash,
            "reported_binary_sha256": result["binary_sha256"],
            "replacement_path_sha256": hashlib.sha256(replacement).hexdigest(),
            "reported_identity_bound_to_archive_member": result["binary_sha256"] == member_hash,
            "temporary_directory_removed_by_context": True,
        }


def q2_static_boundary() -> dict[str, object]:
    text = INSTALLER.read_text(encoding="utf-8")
    catch_start = text.index("        } catch {\n            $replaceError = $_.Exception.Message")
    catch_end = text.index("\n        }\n    } else {", catch_start)
    catch = text[catch_start:catch_end]
    finally_start = text.index("} finally {")
    finally_block = text[finally_start:]
    required = [
        "$replaceError = $_.Exception.Message",
        "replace_failed_recovery_unconfirmed: $replaceError",
        "target=$target; backup=$backup; candidate=$candidate",
    ]
    forbidden_catch = [
        "[System.IO.File]::Copy",
        "[System.IO.File]::Move",
        "Remove-Item",
        "$preserveRecovery = $false",
    ]
    no_target_remove_anywhere = "Remove-Item -LiteralPath $target" not in text
    finally_cleanup_is_guarded = (
        "if (-not $preserveRecovery -and $null -ne $candidate)" in finally_block
        and "if (-not $preserveRecovery -and $null -ne $backup)" in finally_block
    )
    passed = (
        all(value in catch for value in required)
        and all(value not in catch for value in forbidden_catch)
        and no_target_remove_anywhere
        and finally_cleanup_is_guarded
    )
    return {
        "passed": passed,
        "catch_required_reporting_present": {value: value in catch for value in required},
        "catch_forbidden_mutations_absent": {value: value not in catch for value in forbidden_catch},
        "target_remove_absent_from_script": no_target_remove_anywhere,
        "candidate_and_backup_finally_cleanup_guarded_by_preserveRecovery": finally_cleanup_is_guarded,
        "powershell_or_windows_executed": False,
    }


def identities() -> dict[str, object]:
    return {
        "quality_fix_hold_sha256": sha256(PRODUCT / "artifacts/native-task6/quality-fix/FINAL_HOLD.json"),
        "source_manifest_sha256": sha256(PRODUCT / "artifacts/native-task6/quality-fix/source-manifest.json"),
        "source_archive_sha256": sha256(PRODUCT / "artifacts/native-task6/quality-fix/source.tar.gz"),
        "package_sha256": sha256(PRODUCT / "artifacts/native-task6/quality-fix/llmgw-macos-arm64.tar.gz"),
        "original_quality_review_hold_sha256": sha256(
            RESEARCH / "evidence/native-task6-quality-review/FINAL_HOLD.json"
        ),
        "quality_parent_check_sha256": sha256(RESEARCH / "evidence/native-task6-quality-parent-check.json"),
    }


def main() -> int:
    q1 = q1_interleaving()
    q2 = q2_static_boundary()
    result = {
        "verdict": "PASS" if q1["passed"] and q2["passed"] else "CHANGES_REQUESTED",
        "q1": q1,
        "q2": q2,
        "identities": identities(),
        "fresh_runtime_scenarios": 1,
        "fresh_static_checks": 1,
        "cargo_or_full_suite_reruns": 0,
        "product_runtime_started": False,
    }
    print(json.dumps(result, indent=2))
    return 0 if result["verdict"] == "PASS" else 1


if __name__ == "__main__":
    raise SystemExit(main())
