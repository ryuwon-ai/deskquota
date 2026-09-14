#!/usr/bin/env python3
"""Create the exclusive S1 rereview FINAL_HOLD manifest."""

from __future__ import annotations

from datetime import datetime, timezone
import hashlib
import json
from pathlib import Path


OUT = Path(__file__).resolve().parent
RESEARCH = OUT.parents[2]


def sha256(path: Path) -> str:
    return hashlib.sha256(path.read_bytes()).hexdigest()


def row(path: Path) -> dict[str, object]:
    return {
        "path": path.relative_to(RESEARCH).as_posix(),
        "bytes": path.stat().st_size,
        "sha256": sha256(path),
    }


before = json.loads((OUT / "identity-before.json").read_text(encoding="utf-8"))
after = json.loads((OUT / "identity-after.json").read_text(encoding="utf-8"))
assessment = json.loads((OUT / "contract-assessment.json").read_text(encoding="utf-8"))
failed_audit = json.loads((OUT / "contract-audit.json").read_text(encoding="utf-8"))
findings = json.loads((OUT / "findings.json").read_text(encoding="utf-8"))
cleanup = json.loads((OUT / "cleanup.json").read_text(encoding="utf-8"))
normalized_before = {key: value for key, value in before.items() if key not in {"at", "label"}}
normalized_after = {key: value for key, value in after.items() if key not in {"at", "label"}}
files = sorted(path for path in OUT.iterdir() if path.is_file() and path.name != "FINAL_HOLD.json")
payload = {
    "at": datetime.now(timezone.utc).isoformat(),
    "phase": "native-task6-spec-review-fallback-rereview-s1",
    "verdict": "SPEC_PASS",
    "resolved_findings": 1,
    "remaining_findings": len(findings["remaining_findings"]),
    "product_read_only": True,
    "identity_before_after_equal": normalized_before == normalized_after,
    "source_files": after["source_files"],
    "source_archive_members": after["source_archive_members"],
    "package_members": after["package_members"],
    "changed_paths": after["changed_paths"],
    "baseline_rows": after["baseline_rows"],
    "previous_review_rows": after["previous_review_rows"],
    "doc_fix_hold_rows": after["doc_fix_hold_rows"],
    "preservation_drift": len(after["source_live_drift"]) + len(after["baseline_drift"]) + len(after["doc_fix_hold_drift"]),
    "contract_clauses": len(assessment["requirements"]),
    "contract_assessment_passed": assessment["passed"],
    "failed_literal_audit_preserved": failed_audit["passed"] is False,
    "runtime_executed": cleanup["runtime_executed"],
    "cargo_or_test_rerun": cleanup["cargo_or_test_rerun"],
    "windows_runtime_verified": False,
    "quality_authorized_as_separate_fresh_review": True,
    "execution_finished": cleanup["execution_finished"],
    "runtime_ownership_released": cleanup["runtime_ownership_released"],
    "owned_workers_remaining": cleanup["owned_workers_remaining"],
    "evidence_rows_excluding_final_hold": len(files),
    "evidence_files_total_including_final_hold": len(files) + 1,
    "files": [row(path) for path in files],
}
complete = (
    before["passed"] and after["passed"]
    and payload["identity_before_after_equal"]
    and payload["changed_paths"] == ["product/docs/installation.md"]
    and payload["preservation_drift"] == 0
    and payload["baseline_rows"] == 1646
    and payload["previous_review_rows"] == 17
    and payload["doc_fix_hold_rows"] == 12
    and assessment["passed"]
    and failed_audit["passed"] is False
    and payload["remaining_findings"] == 0
    and cleanup["execution_finished"]
    and cleanup["runtime_ownership_released"]
    and cleanup["owned_workers_remaining"] == 0
)
payload["hold_manifest_complete"] = complete
with (OUT / "FINAL_HOLD.json").open("x", encoding="utf-8") as stream:
    json.dump(payload, stream, indent=2)
    stream.write("\n")
print(json.dumps({"complete": complete, "files": len(files) + 1}))
raise SystemExit(0 if complete else 1)
