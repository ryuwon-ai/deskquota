#!/usr/bin/env python3
"""Check the S1 recovery documentation clauses and package binding."""

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


text = DOC.read_text(encoding="utf-8")
requirements = {
    "names_unconfirmed_recovery_error": "replace_failed_recovery_unconfirmed" in text,
    "stops_and_keeps_target_backup_candidate": "stop and keep\n+the reported target, destination-local `.llmgw-backup-*`, and candidate files" in text,
    "forbids_rerun_or_recovery_file_deletion": "Do not rerun the installer or delete its recovery files" in text,
    "discloses_backup_only_and_absent_target": "left `llmgw.exe` absent while the previous executable exists only at\n+the reported backup path" in text,
    "records_errors_and_paths": "Record the complete original and recovery errors and\n+all three paths" in text,
    "requires_absent_target_and_no_writer": "confirm that the target is still\n+absent, no other installer or process is writing it" in text,
    "requires_known_previous_identity": "backup matches the\n+known previous binary by its trusted checksum or release identity" in text,
    "copies_before_optional_move": "copy the verified previous executable back to the reported target; move it only\n+if another recovery copy remains" in text,
    "retains_copies_until_checksum_version_execution": "Keep the backup and candidate until the\n+restored target's checksum, version, and execution have been verified" in text,
    "forbids_blind_overwrite": "Never\n+blindly overwrite an existing target" in text,
    "preserves_all_when_identity_or_writer_unknown": "If the previous binary's identity or\n+writer state cannot be established, preserve every reported file" in text,
    "windows_runtime_stays_unverified": "Windows execution, Authenticode,\n+SmartScreen, and user-PATH behavior remain unverified" in text,
}
with tarfile.open(FIX / "llmgw-macos-arm64.tar.gz", "r:gz") as archive:
    packaged = archive.extractfile("docs/installation.md")
    packaged_bytes = packaged.read() if packaged is not None else b""
payload = {
    "at": datetime.now(timezone.utc).isoformat(),
    "review_type": "S1_documentation_only_static_contract_and_package_binding",
    "requirements": requirements,
    "live_document_sha256": hashlib.sha256(DOC.read_bytes()).hexdigest(),
    "packaged_document_sha256": hashlib.sha256(packaged_bytes).hexdigest(),
    "packaged_document_equals_live": packaged_bytes == DOC.read_bytes(),
    "windows_runtime_executed": False,
    "passed": all(requirements.values()) and packaged_bytes == DOC.read_bytes(),
}
with (OUT / "contract-audit.json").open("x", encoding="utf-8") as stream:
    json.dump(payload, stream, indent=2)
    stream.write("\n")
print(json.dumps({"passed": payload["passed"], "clauses": len(requirements)}))
raise SystemExit(0 if payload["passed"] else 1)
