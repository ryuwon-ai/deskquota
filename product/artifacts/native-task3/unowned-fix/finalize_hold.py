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
PRODUCT = OUT.parents[2]
RESEARCH = PRODUCT.parent
INITIAL = PRODUCT / "artifacts/native-task3"
LOCK_FIX = INITIAL / "lock-fix"
SPEC_FIX = INITIAL / "spec-fix"
PROTECTED_BASE = RESEARCH / "evidence/native-task2-quality-review/rereview/after.json"
REREVIEW = RESEARCH / "evidence/native-task3-spec-review/rereview"
REREVIEW_AFTER = REREVIEW / "after.json"
BENCH_BASE = RESEARCH / "evidence/native-task1-quality-review/pilot-preservation.json"
ACCOUNT_BASE = RESEARCH / "evidence/accounting-task2-quality-review/hashes-after.json"


def digest(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def identity(path, shown=None):
    return {
        "path": shown or str(path.relative_to(PRODUCT)),
        "bytes": path.stat().st_size,
        "sha256": digest(path),
    }


def verify(items):
    drift = []
    for item in items:
        path = RESEARCH / item["path"]
        if not path.is_file():
            drift.append({"path": item["path"], "reason": "missing"})
        elif path.stat().st_size != item["bytes"] or digest(path) != item["sha256"]:
            drift.append({"path": item["path"], "reason": "content changed"})
    return drift


capture = json.loads((OUT / "capture-summary.json").read_text())
manifest = json.loads((OUT / "source-manifest.json").read_text())
source_drift = verify(manifest["files"])
if source_drift:
    raise SystemExit(f"source drift: {source_drift}")

with tarfile.open(OUT / "source-hold.tar.gz", "r:gz") as archive:
    members = archive.getmembers()
    if [member.name for member in members] != [item["path"] for item in manifest["files"]]:
        raise SystemExit("archive member mismatch")
    for member, item in zip(members, manifest["files"]):
        data = archive.extractfile(member).read()
        if len(data) != item["bytes"] or hashlib.sha256(data).hexdigest() != item["sha256"]:
            raise SystemExit(f"archive content mismatch: {member.name}")

protected = json.loads(PROTECTED_BASE.read_text())["protected"]
extra = json.loads(REREVIEW_AFTER.read_text())["extra_protected"]
protected_drift = verify(protected)
extra_drift = verify(extra)
if protected_drift or extra_drift:
    raise SystemExit(f"protected drift: base={protected_drift} extra={extra_drift}")

bench = sorted((PRODUCT / "artifacts/pilot-runs").glob("*.json"))
accounting = sorted((PRODUCT / "artifacts/accounting-ablation/pilot-runs").glob("*.json"))
if len(bench) != 40 or len(accounting) != 10:
    raise SystemExit("raw count changed")
bench_hash = {item["path"]: item["sha256"] for item in json.loads(BENCH_BASE.read_text())}
account_hash = json.loads(ACCOUNT_BASE.read_text())
for path, hashes in [
    *((path, bench_hash) for path in bench),
    *((path, account_hash) for path in accounting),
]:
    relative = str(path.relative_to(RESEARCH))
    if relative not in hashes or digest(path) != hashes[relative]:
        raise SystemExit(f"raw drift: {relative}")

expected = {
    INITIAL / "source-manifest.json": "6cdfbec0b41839689364345e4b973a2f6af9409234fc5e2189f77f4144873e9a",
    INITIAL / "source-hold.tar.gz": "17c82d6aec084de464f4ae4b1eca850772c46f195d55997dcd534b0b15b4451d",
    INITIAL / "final-hold.json": "601a465a47c60418ef756edccaabc484e5afce9c1c8323e41fea754e03a22973",
    LOCK_FIX / "source-manifest.json": "f73a434551c85776950dd76794a86e14f05846dc366609f827f28a204613060c",
    LOCK_FIX / "source-hold.tar.gz": "1b4b96aa32e2c4414eb52aaf87ed2382cb40d310974b01adef63c90b20bbbe2e",
    LOCK_FIX / "final-hold.json": "903a07a420fc899e756d890703fad42264adca179d969094a288d598d6782f2d",
    SPEC_FIX / "source-manifest.json": "3239733f0fda5ec89f6aff138a7cf4ceba92f915dcb472dbc6d70e2581967af0",
    SPEC_FIX / "source-hold.tar.gz": "c01ffa1ef1f3f7ba514b93a3a2cbe7ea2c67d92cfffdfbd8eced973f3fd055f3",
    SPEC_FIX / "final-hold.json": "7e549e645da2f86252a61b8d6223b586d213f3604a80494e885485e9f6ffe799",
    PRODUCT / "target/native-task3/debug/llmgw": "66142c7dd7cd21a500b4075e77bf840283c4741ef178b275a09a5f19d3ba504c",
    PRODUCT / "target/native-task3/release/llmgw": "215511357446c5583ebd5d4a3d9aee6b8ea5df4d83387d31d0ce23ae2a97eb6d",
    PRODUCT / "target/native-task3-lock-fix/debug/llmgw": "9a5fa1d6754c4cb88061434d253429e0b89586b5eedf212de0277fa83227da91",
    PRODUCT / "target/native-task3-lock-fix/release/llmgw": "6b7a35b2b340037891cdf77a7b1c50fadfcc0767677bfcf7ec1fb8595a7c0598",
    PRODUCT / "target/native-task3-spec-fix/debug/llmgw": "a0e5572d4010d338c858acc64d2ef20e3a9355a496efd988803eaa4b3429e706",
    PRODUCT / "target/native-task3-spec-fix/release/llmgw": "4472df4f3fb68000cf31d4eb33edc4fa9faf9176b0fb04b18db3a0d1184573e4",
    RESEARCH / "evidence/native-task3-spec-review/FINAL_HOLD.json": "39711385c931b90fde35f98b122d74f39b6d4a8df711042cab3b42d3a0cf1590",
    REREVIEW / "FINAL_HOLD.json": "713aa191d0ab9ce471b86cde8f2b5dcb1d85137f492cb452e852064743c3a377",
    RESEARCH / "evidence/native-task3-parent-lock-rerun/result.json": "254303156115ac6878065412c4c23ad2052687b4c2523b1e1d64e75a3667e3d3",
}
for path, sha256 in expected.items():
    if digest(path) != sha256:
        raise SystemExit(f"held identity drift: {path}")


def rows(name):
    return [
        int(value)
        for value in re.findall(
            r"test result: ok\. (\d+) passed; 0 failed;", (OUT / name).read_text()
        )
    ]


full_rows = rows("cargo-test-full-final.log")
if len(full_rows) != 15 or sum(full_rows) != 302:
    raise SystemExit(f"bad full rows: {full_rows}")
if rows("patch-contract-final.log") != [38] or rows("patch-contract-release-final.log") != [38]:
    raise SystemExit("bad focused totals")
for name in (
    "cargo-check-all-targets-final.log",
    "cargo-clippy-all-targets-final.log",
    "cargo-build-debug-final.log",
    "cargo-build-release-final.log",
):
    if "Finished" not in (OUT / name).read_text():
        raise SystemExit(f"incomplete log: {name}")
if (OUT / "cargo-fmt-final.log").read_text():
    raise SystemExit("fmt output not empty")

probe = json.loads((OUT / "spec-probe-result.json").read_text())
if len(probe) != 4 or not all(case.get("spec_pass") is True for case in probe):
    raise SystemExit(f"probe failed: {probe}")
if (OUT / "spec-probe-build.stderr").read_text() or (OUT / "spec-probe.stderr").read_text():
    raise SystemExit("probe stderr not empty")

sentinels = (
    "".join(("upstream-", "synthetic-", "8f2f5a47")),
    "".join(("local-data-", "synthetic-", "3dcb99a1")),
    "".join(("control-", "synthetic-", "6a01c442")),
)
hits = []
for path in [
    *(RESEARCH / item["path"] for item in manifest["files"]),
    *OUT.glob("*.log"),
    OUT / "spec-probe-result.json",
    OUT / "spec-probe.stderr",
]:
    text = path.read_text(errors="replace")
    for index, sentinel in enumerate(sentinels):
        if sentinel in text:
            hits.append({"path": str(path.relative_to(RESEARCH)), "sentinel_index": index})
if hits:
    raise SystemExit(f"sentinel exposure: {hits}")

processes = subprocess.run(
    ["ps", "-axo", "pid=,ppid=,command="], capture_output=True, text=True, check=True
).stdout
owned = [
    line.strip()
    for line in processes.splitlines()
    if "native-task3-unowned-fix" in line
    and any(value in line for value in ("/llmgw", "patch_contract-", "spec-probe", "/cargo", "/rustc"))
]
tmp = pathlib.Path(tempfile.gettempdir())
temp_paths = sorted(
    {
        str(path)
        for pattern in (
            "llmgw-patch-*",
            "llmgw-windows-*",
            "llmgw-config-patch-locks-*",
            "llmgw-task3-unowned-probe-*",
        )
        for path in tmp.glob(pattern)
    }
)
cleanup = {
    "captured_at": datetime.datetime.now(datetime.timezone.utc).isoformat(),
    "owned_processes": owned,
    "matching_temp_paths": temp_paths,
    "execution_finished": not owned and not temp_paths,
}
(OUT / "cleanup-final.json").write_text(json.dumps(cleanup, indent=2) + "\n")
(OUT / "owned-processes-final.txt").write_text("none\n" if not owned else "\n".join(owned) + "\n")
(OUT / "owned-temp-paths-final.txt").write_text(
    "none\n" if not temp_paths else "\n".join(temp_paths) + "\n"
)
if not cleanup["execution_finished"]:
    raise SystemExit(f"cleanup incomplete: {cleanup}")

protected_result = {
    "base": str(PROTECTED_BASE.relative_to(RESEARCH)),
    "base_count": len(protected),
    "base_drift": protected_drift,
    "task3_extra_baseline": str(REREVIEW_AFTER.relative_to(RESEARCH)),
    "task3_extra_count": len(extra),
    "task3_extra_drift": extra_drift,
    "benchmark_raws": len(bench),
    "accounting_raws": len(accounting),
    "held_identities": [identity(path, str(path.relative_to(RESEARCH))) for path in expected],
}
(OUT / "protected-check.json").write_text(json.dumps(protected_result, indent=2) + "\n")

release = PRODUCT / "target/native-task3-unowned-fix/release/llmgw"
debug = PRODUCT / "target/native-task3-unowned-fix/debug/llmgw"
probe_rlib = PRODUCT / "target/native-task3-unowned-fix/debug/deps/libllmgw-eba4665eb2ddaf45.rlib"
if not probe_rlib.is_file():
    raise SystemExit(f"probe default-feature rlib missing: {probe_rlib}")
rustc = subprocess.run(
    ["rustc", "--version", "--verbose"], capture_output=True, text=True, check=True
).stdout.strip()
cargo = subprocess.run(
    ["cargo", "--version", "--verbose"], capture_output=True, text=True, check=True
).stdout.strip()

final = {
    "phase": "FINAL_HOLD",
    "fix": "native-task3 S5 never-owned retry preownership",
    "execution_finished": True,
    "writer_runtime_ownership_released": True,
    "source_stable_after_capture": True,
    "source_drift": source_drift,
    **capture,
    "binaries": {
        "release": identity(release),
        "debug": identity(debug),
        "debug_rlib": identity(probe_rlib),
    },
    "probe": {
        "source": identity(OUT / "spec-probe.rs"),
        "binary": identity(OUT / "spec-probe"),
        "result": identity(OUT / "spec-probe-result.json"),
        "all_cases_passed": True,
    },
    "lockfile": identity(PRODUCT / "Cargo.lock"),
    "toolchain_file": identity(PRODUCT / "rust-toolchain.toml"),
    "verification": {
        "full_debug": {"passed": 302, "failed": 0, "rows": full_rows},
        "patch_debug": {"passed": 38, "failed": 0},
        "patch_release": {"passed": 38, "failed": 0},
        "adapted_spec_probe": {"passed": 4, "failed": 0},
        "static": {
            "rustfmt": "passed",
            "check_all_targets": "passed",
            "clippy_all_targets_deny_warnings": "passed",
            "debug_build": "passed",
            "release_build": "passed",
        },
    },
    "red_green": {
        "red": "patch-contract-unowned-red.log: 35 passed, 3 failed",
        "green": "patch-contract-final.log: 38 passed, 0 failed",
        "release_green": "patch-contract-release-final.log: 38 passed, 0 failed",
    },
    "finding_resolution": {
        "S5": "a failed or unstarted journal row without owned state takes original/created/privacy metadata from the first actual snapshot under its resource lock after any required protected backup succeeds; established ownership is unchanged"
    },
    "platform_evidence": {
        "macos_runtime": "focused/full tests and four-case adapted reviewer probe, including failed-first, unstarted-later, and reverse existing-to-absent retry boundaries",
        "windows": "static cfg only; no Windows runtime claim",
        "linux": "static cfg only; no Linux runtime claim",
    },
    "protected": {
        "accepted_task2": len(protected),
        "task3_extra": len(extra),
        "benchmark_raws": len(bench),
        "accounting_raws": len(accounting),
        "drift": [],
    },
    "host": platform.platform(),
    "rustc": rustc,
    "cargo": cargo,
    "cleanup": "artifacts/native-task3/unowned-fix/cleanup-final.json",
    "limits": [
        "all prior Task3 holds and reviewer evidence remain immutable",
        "the earlier unavailable pre-redaction capture remains unavailable and was not reconstructed",
        "external-editor final-check to rename filesystem CAS is not claimed",
        "Windows and Linux runtime remain unverified",
        "Task4 adapters and CLI wiring remain out of scope",
    ],
}
(OUT / "final-hold.json").write_text(json.dumps(final, indent=2) + "\n")
sha256 = digest(OUT / "final-hold.json")
(OUT / "FINAL_HOLD").write_text(
    "FINAL_HOLD\n"
    "execution_finished=true\n"
    "writer_runtime_ownership_released=true\n"
    f"final_hold_sha256={sha256}\n"
)
print(
    json.dumps(
        {
            "final_hold_sha256": sha256,
            "release": final["binaries"]["release"],
            "debug": final["binaries"]["debug"],
            "rlib": final["binaries"]["debug_rlib"],
            "cleanup": cleanup,
        },
        indent=2,
    )
)
