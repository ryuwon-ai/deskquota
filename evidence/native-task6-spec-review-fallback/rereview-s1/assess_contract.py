#!/usr/bin/env python3
"""Whitespace-normalized assessment preserving the failed literal audit."""

from __future__ import annotations

from datetime import datetime, timezone
import hashlib
import json
from pathlib import Path
import tarfile


RESEARCH = Path(__file__).resolve().parents[3]
PRODUCT = RESEARCH / "product"
FIX = PRODUCT / "artifacts/native-task6/spec-doc-fix"
OUT = Path(__file__).resolve().parent
DOC = PRODUCT / "docs/installation.md"


raw = DOC.read_text(encoding="utf-8")
text = " ".join(raw.split())
phrases = {
    "names_unconfirmed_recovery_error": "replace_failed_recovery_unconfirmed",
    "stops_and_keeps_target_backup_candidate": "stop and keep the reported target, destination-local `.llmgw-backup-*`, and candidate files",
    "forbids_rerun_or_recovery_file_deletion": "Do not rerun the installer or delete its recovery files",
    "discloses_backup_only_and_absent_target": "left `llmgw.exe` absent while the previous executable exists only at the reported backup path",
    "records_errors_and_paths": "Record the complete original and recovery errors and all three paths",
    "requires_absent_target_and_no_writer": "confirm that the target is still absent, no other installer or process is writing it",
    "requires_known_previous_identity": "backup matches the known previous binary by its trusted checksum or release identity",
    "copies_before_optional_move": "copy the verified previous executable back to the reported target; move it only if another recovery copy remains",
    "retains_copies_until_checksum_version_execution": "Keep the backup and candidate until the restored target's checksum, version, and execution have been verified",
    "forbids_blind_overwrite": "Never blindly overwrite an existing target",
    "preserves_all_when_identity_or_writer_unknown": "If the previous binary's identity or writer state cannot be established, preserve every reported file",
    "windows_runtime_stays_unverified": "Windows execution, Authenticode, SmartScreen, and user-PATH behavior remain unverified",
}
requirements = {name: phrase in text for name, phrase in phrases.items()}
with tarfile.open(FIX / "llmgw-macos-arm64.tar.gz", "r:gz") as archive:
    stream = archive.extractfile("docs/installation.md")
    packaged = stream.read() if stream is not None else b""
failed_audit = OUT / "contract-audit.json"
payload = {
    "at": datetime.now(timezone.utc).isoformat(),
    "preserved_failed_audit": {
        "path": failed_audit.name,
        "sha256": hashlib.sha256(failed_audit.read_bytes()).hexdigest(),
        "reported_passed": False,
        "reviewer_error": "literal expectations accidentally contained a plus sign after each encoded newline",
    },
    "method": "normalize Markdown whitespace and require every complete S1 phrase",
    "requirements": requirements,
    "packaged_document_equals_live": packaged == DOC.read_bytes(),
    "live_document_sha256": hashlib.sha256(DOC.read_bytes()).hexdigest(),
    "packaged_document_sha256": hashlib.sha256(packaged).hexdigest(),
    "windows_runtime_executed": False,
    "passed": all(requirements.values()) and packaged == DOC.read_bytes(),
}
with (OUT / "contract-assessment.json").open("x", encoding="utf-8") as stream:
    json.dump(payload, stream, indent=2)
    stream.write("\n")
print(json.dumps({"passed": payload["passed"], "clauses": len(requirements)}))
raise SystemExit(0 if payload["passed"] else 1)
