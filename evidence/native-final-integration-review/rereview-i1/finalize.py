#!/usr/bin/env python3
"""Freeze the bounded same-reviewer I1 rereview evidence."""
import datetime
import hashlib
import json
import pathlib
import tarfile

OUT = pathlib.Path(__file__).resolve().parent
RESEARCH = OUT.parents[2]
ARTIFACT = RESEARCH / "product/artifacts/native-final-integration-fix"
EXPECTED = {
    "manifest": "3f2c38648167168e2d9d22324cdae7850f1a839a791e42d65c4168e45269f8aa",
    "archive": "de54b78c74ce6e29e0b2a3960f07a870086cfe9a319a47b725e04198cb58d967",
    "binary": "16de493ce1ec7f034d0fed31db111bd4ddcf2750c813e37fb12a8b2b74f10fa7",
    "artifact_hold": "97fb4309a365e598e210841201c6548ccf77a411cda08a55d0c8d7380ad2e8c4",
    "original_review_hold": "9e1c4a30220fd447f708989dcf00dec25c1053a2ceb0f70a8cd504aceba053f4",
    "original_size_correction": "ae72208fa3c192c97afe2e675ba336ef8fe256fc4a10352030d1fbe384effd52",
}
CHANGED = [
    "product/docs/client-compatibility.md",
    "product/docs/installation.md",
    "product/docs/runtime-contract.md",
    "product/src/lifecycle/mod.rs",
    "product/src/setup/persist.rs",
    "product/src/setup/summary.rs",
    "product/tests/client_profiles.rs",
    "product/tests/setup_contract.rs",
]


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


manifest_path = ARTIFACT / "source-manifest.json"
archive_path = ARTIFACT / "source.tar.gz"
binary_path = ARTIFACT / "llmgw-macos-arm64"
artifact_hold_path = ARTIFACT / "FINAL_HOLD.json"
parent_path = RESEARCH / "evidence/native-final-integration-fix-parent-check.json"
old_hold = RESEARCH / "evidence/native-final-integration-review/FINAL_HOLD.json"
old_correction = RESEARCH / "evidence/native-final-integration-review/HOLD-size-correction.json"
runtime = json.loads((OUT / "direct-runtime.json").read_text())
manifest = json.loads(manifest_path.read_text())
parent = json.loads(parent_path.read_text())
artifact_hold = json.loads(artifact_hold_path.read_text())

identity_checks = {
    "source_manifest": sha(manifest_path) == EXPECTED["manifest"],
    "source_archive": sha(archive_path) == EXPECTED["archive"],
    "release_binary": sha(binary_path) == EXPECTED["binary"] and binary_path.stat().st_size == 10038272,
    "artifact_hold": sha(artifact_hold_path) == EXPECTED["artifact_hold"],
    "original_review_hold": sha(old_hold) == EXPECTED["original_review_hold"],
    "original_hold_size_correction": sha(old_correction) == EXPECTED["original_size_correction"],
}
source_rows = manifest["files"]
source_drift = []
for row in source_rows:
    path = RESEARCH / row["path"]
    if not path.exists() or path.stat().st_size != row["bytes"] or sha(path) != row["sha256"]:
        source_drift.append(row["path"])
with tarfile.open(archive_path, "r:gz") as archive:
    members = [member.name for member in archive.getmembers() if member.isfile()]
archive_exact = members == [row["path"] for row in source_rows]
parent_ok = (
    parent.get("passed") is True
    and parent.get("source_files") == 101
    and parent.get("archive_members") == 101
    and parent.get("drift") == 0
    and parent.get("changed_paths") == CHANGED
    and parent.get("rust_tests_passed") == 370
    and parent.get("rust_tests_failed") == 0
)
artifact_hold_ok = artifact_hold.get("passed") is True and artifact_hold.get("owned_workers_remaining") == 0
passed = (
    runtime.get("passed") is True
    and runtime.get("owned_workers_remaining") == 0
    and runtime.get("scratch_removed") is True
    and runtime.get("upstream_calls") == 0
    and all(identity_checks.values())
    and len(source_rows) == 101
    and not source_drift
    and archive_exact
    and manifest.get("changed_paths") == CHANGED
    and parent_ok
    and artifact_hold_ok
)
if not passed:
    raise SystemExit("rereview inputs did not pass freeze checks")

checks = {
    "phase": "native-final-integration-rereview-i1",
    "passed": True,
    "verdict": "PASS_WITH_PLATFORM_LIMITATIONS",
    "i1": "fixed",
    "identities": identity_checks,
    "release_binary": record(binary_path),
    "source_manifest": record(manifest_path),
    "source_archive": record(archive_path),
    "artifact_hold": record(artifact_hold_path),
    "source_files_checked": len(source_rows),
    "source_drift": source_drift,
    "archive_members": len(members),
    "archive_exact_manifest_order_and_members": archive_exact,
    "changed_paths": CHANGED,
    "parent_check": {"record": record(parent_path), "passed": parent_ok, "retained_rust_tests": {"passed": 370, "failed": 0, "rerun_by_reviewer": False}},
    "original_review_evidence": {"hold": record(old_hold), "size_correction": record(old_correction), "preserved": True},
    "source_review": [
        {"path": "product/src/lifecycle/mod.rs", "lines": "109-135", "finding": "canonical config/state resolution, private directory check, operation lock, exact fingerprint recheck, worker-lock-gated provisioning, protected reads, and common credential validation"},
        {"path": "product/src/setup/persist.rs", "lines": "181-265", "finding": "setup snapshot/lock and config+pending publication precede state initialization; partial failure is explicit; SaveOnly returns before lifecycle activation"},
        {"path": "product/src/cli.rs", "lines": "503-651", "finding": "the now-provisioned data token feeds the reviewed client plan; fresh runtime state is rechecked and authenticated readiness precedes apply_reviewed"},
        {"path": "product/tests/setup_contract.rs", "lines": "1130-1288", "finding": "retained focused coverage includes private token creation/preservation, explicit partial failures, invalid-token preservation, and no repair under a running worker"},
        {"path": "product/tests/client_profiles.rs", "lines": "1485-1599", "finding": "retained actual-CLI/fake-version coverage closes the SaveOnly-to-client-preview path for three profile types; it is not installed-client execution"},
    ],
    "documentation_alignment": [
        "runtime-contract.md:101-117 and 701-710",
        "installation.md:144-151",
        "client-compatibility.md:58-72",
    ],
}
write_json(OUT / "source-evidence-checks.json", checks)

result = {
    "phase": "native-final-integration-rereview-i1",
    "passed": True,
    "verdict": "PASS_WITH_PLATFORM_LIMITATIONS",
    "actionable_findings": [],
    "i1_disposition": "fixed",
    "direct_execution": runtime,
    "retained_evidence": {"rust_tests": "370 passed, 0 failed in frozen artifact/parent check; reviewer did not rerun Cargo", "neighbor_contracts": "private permissions, token preservation, partial failure, running-worker no-repair, and three fake-version client profiles"},
    "reviewer_harness_note": "The first reviewer attempt incorrectly required Pi JSON whitespace to restore byte-for-byte. Authenticated off and scratch cleanup completed. The assertion was corrected to the documented owned-key/user-value restoration contract; the one final bounded flow then passed.",
    "limitations": [
        "Direct client integration used a synthetic Pi --version executable; no installed Pi/Claude/Codex client was executed in this rereview.",
        "Windows, Linux, macOS x64, real human login, signing, notarization, quarantine, and clean-account execution remain unverified.",
        "No production-all-OS, percentile latency, low-end memory, or performance-advantage claim is made.",
    ],
    "cleanup": {"execution_finished": True, "runtime_ownership_released": True, "owned_workers_remaining": 0, "owned_fixture_paths_remaining": 0},
}
write_json(OUT / "result.json", result)

(OUT / "README.md").write_text("""# Native final integration rereview — I1

**Verdict: PASS_WITH_PLATFORM_LIMITATIONS.** No remaining actionable finding was found in the bounded I1 delta or its adjacent setup/client lifecycle seams.

## Directly executed

The frozen macOS arm64 binary `16de493c…10fa7` ran in an isolated HOME with owned loopback ports. A real PTY setup **Save only** created distinct 64-byte data/control tokens under 0700/0600 permissions while status remained stopped and worker count remained zero. Pi preview produced a 64-character reviewed hash without changing client bytes or token identity. Reusing that exact hash with `--restart` established the desired authenticated fingerprint before the client patch. Disconnect preserved the user's JSON values and removed the owned llmgw provider; authenticated off returned stopped. Upstream calls, leaked protected token values, remaining workers, and remaining fixture paths were all zero.

The Pi executable in this rereview was an isolated synthetic `--version` fixture reporting 0.84.2. This proves the actual llmgw CLI integration path, not execution of an installed Pi client. The installer-owned client run remains separate.

## Source and retained evidence

The setup apply now calls the common config-scoped credential path under the operation lock, rechecks the exact saved fingerprint, provisions only while the worker lock is available, and validates existing protected credentials in both stopped and running branches. Setup publishes config and pending metadata before initialization and reports that boundary explicitly on failure; Save only returns before worker activation. The client path still binds the preview hash to the gateway fingerprint/client snapshot and reaches `apply_reviewed` only after fresh authenticated readiness.

All 101 current source rows match the frozen manifest, and the 101-member source archive matches it exactly. The release binary, artifact HOLD, original integration review HOLD, and its size correction retain their expected SHA-256 identities. The parent-retained Rust result is 370 passed / 0 failed; Cargo was not rerun here.

The first reviewer attempt used an overly strict byte-for-byte JSON whitespace assertion after disconnect. It completed authenticated cleanup with no worker or scratch left. The final run checks the documented owned-key and user-value restoration contract and passes.

## Limits

This rereview does not add Windows, Linux, macOS x64, real-login, signing/notarization/quarantine, clean-account, performance-percentile, low-end-memory, or installed-client evidence. Those remain platform or later-run limitations rather than new defects.
""")

held_names = [
    "README.md",
    "result.json",
    "source-evidence-checks.json",
    "direct-runtime.json",
    "setup.pty.txt",
    "connect-preview.txt",
    "connect-apply.txt",
    "disconnect.txt",
    "probe.py",
    "finalize.py",
]
hold = {
    "phase": "native-final-integration-rereview-i1",
    "captured_at": datetime.datetime.now(datetime.timezone.utc).isoformat(),
    "status": "FINAL_HOLD",
    "do_not_overwrite": True,
    "passed": True,
    "verdict": "PASS_WITH_PLATFORM_LIMITATIONS",
    "i1": "fixed",
    "actionable_findings": 0,
    "source_files": 101,
    "archive_members": 101,
    "release_binary_sha256": EXPECTED["binary"],
    "source_manifest_sha256": EXPECTED["manifest"],
    "source_archive_sha256": EXPECTED["archive"],
    "artifact_hold_sha256": EXPECTED["artifact_hold"],
    "direct_runtime_passed": True,
    "upstream_calls": 0,
    "installed_clients_executed": False,
    "execution_finished": True,
    "runtime_ownership_released": True,
    "owned_workers_remaining": 0,
    "owned_fixture_paths_remaining": 0,
    "files": [record(OUT / name) for name in held_names],
}
write_json(OUT / "FINAL_HOLD.json", hold)
print(json.dumps({"passed": True, "hold": record(OUT / "FINAL_HOLD.json")}, indent=2))
