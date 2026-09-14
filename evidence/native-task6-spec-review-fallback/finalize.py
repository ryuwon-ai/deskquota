#!/usr/bin/env python3
"""Create the exclusive review FINAL_HOLD manifest."""

from __future__ import annotations

from datetime import datetime, timezone
import hashlib
import json
from pathlib import Path


OUT = Path(__file__).resolve().parent
RESEARCH = OUT.parents[1]


def sha256(path: Path) -> str:
    return hashlib.sha256(path.read_bytes()).hexdigest()


def evidence_row(path: Path) -> dict[str, object]:
    return {
        "path": path.relative_to(RESEARCH).as_posix(),
        "bytes": path.stat().st_size,
        "sha256": sha256(path),
    }


before = json.loads((OUT / "identity-before.json").read_text(encoding="utf-8"))
after = json.loads((OUT / "identity-after.json").read_text(encoding="utf-8"))
runtime = json.loads((OUT / "runtime-assessment.json").read_text(encoding="utf-8"))
installer = json.loads((OUT / "installer-probe.json").read_text(encoding="utf-8"))
cleanup = json.loads((OUT / "cleanup.json").read_text(encoding="utf-8"))
findings = json.loads((OUT / "findings.json").read_text(encoding="utf-8"))
normalized_before = {key: value for key, value in before.items() if key not in {"at", "label"}}
normalized_after = {key: value for key, value in after.items() if key not in {"at", "label"}}
files = sorted(path for path in OUT.iterdir() if path.is_file() and path.name != "FINAL_HOLD.json")
payload = {
    "at": datetime.now(timezone.utc).isoformat(),
    "phase": "native-task6-spec-review-fallback",
    "verdict": "CHANGES_REQUESTED",
    "finding_count": len(findings["findings"]),
    "severity_counts": {"P2": 1},
    "product_read_only": True,
    "held_targets_read_only": True,
    "identity_before_after_equal": normalized_before == normalized_after,
    "source_files": after["source_files"],
    "source_archive_members": after["source_archive_members"],
    "package_members": after["package_members"],
    "task6_hold_rows": after["parent_task6_rows"],
    "final_build_extra_rows": after["parent_extra_rows"],
    "prior_preserved_rows": after["prior_rows"],
    "preservation_drift": len(after["source_live_drift"]) + len(after["parent_task6_drift"]) + len(after["parent_extra_drift"]) + len(after["prior_drift"]),
    "installer_probe_passed": installer["passed"],
    "fresh_runtime_assessment_passed": runtime["passed"],
    "fresh_runtime_commands": 4,
    "fresh_client_commands": 3,
    "fresh_native_measurement_commands": 1,
    "review_wrapper_raw_false_preserved": True,
    "powerShell_runtime_verified": False,
    "quality_authorized": False,
    "execution_finished": cleanup["execution_finished"],
    "runtime_ownership_released": cleanup["runtime_ownership_released"],
    "owned_workers_remaining": cleanup["owned_workers_remaining"],
    "evidence_rows_excluding_final_hold": len(files),
    "evidence_files_total_including_final_hold": len(files) + 1,
    "files": [evidence_row(path) for path in files],
}
passed_to_hold = (
    before["passed"]
    and after["passed"]
    and payload["identity_before_after_equal"]
    and payload["preservation_drift"] == 0
    and installer["passed"]
    and runtime["passed"]
    and cleanup["execution_finished"]
    and cleanup["runtime_ownership_released"]
    and cleanup["owned_workers_remaining"] == 0
    and payload["finding_count"] == 1
)
payload["hold_manifest_complete"] = passed_to_hold
with (OUT / "FINAL_HOLD.json").open("x", encoding="utf-8") as stream:
    json.dump(payload, stream, indent=2)
    stream.write("\n")
print(json.dumps({"hold_manifest_complete": passed_to_hold, "files": len(files) + 1}))
raise SystemExit(0 if passed_to_hold else 1)
