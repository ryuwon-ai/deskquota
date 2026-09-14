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
TASK3 = PRODUCT / "artifacts/native-task3"
LOCK_FIX = TASK3 / "lock-fix"
SPEC_FIX = TASK3 / "spec-fix"
UNOWNED_FIX = TASK3 / "unowned-fix"
QUALITY_FIX = TASK3 / "quality-fix"
QUALITY_REREVIEW = RESEARCH / "evidence/native-task3-quality-review/rereview"
PROTECTED_BASE = RESEARCH / "evidence/native-task2-quality-review/rereview/after.json"
PROTECTED_EXTRA = QUALITY_REREVIEW / "after.json"
BENCH_BASE = RESEARCH / "evidence/native-task1-quality-review/pilot-preservation.json"
ACCOUNT_BASE = RESEARCH / "evidence/accounting-task2-quality-review/hashes-after.json"
TARGET = PRODUCT / "target/native-task3-nonfinite-fix"


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
extra = json.loads(PROTECTED_EXTRA.read_text())["extra_protected"]
if len(protected) != 426 or len(extra) < 283:
    raise SystemExit(f"protected counts changed: base={len(protected)} extra={len(extra)}")
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
    TASK3 / "source-manifest.json": "6cdfbec0b41839689364345e4b973a2f6af9409234fc5e2189f77f4144873e9a",
    TASK3 / "source-hold.tar.gz": "17c82d6aec084de464f4ae4b1eca850772c46f195d55997dcd534b0b15b4451d",
    TASK3 / "final-hold.json": "601a465a47c60418ef756edccaabc484e5afce9c1c8323e41fea754e03a22973",
    LOCK_FIX / "source-manifest.json": "f73a434551c85776950dd76794a86e14f05846dc366609f827f28a204613060c",
    LOCK_FIX / "source-hold.tar.gz": "1b4b96aa32e2c4414eb52aaf87ed2382cb40d310974b01adef63c90b20bbbe2e",
    LOCK_FIX / "final-hold.json": "903a07a420fc899e756d890703fad42264adca179d969094a288d598d6782f2d",
    SPEC_FIX / "source-manifest.json": "3239733f0fda5ec89f6aff138a7cf4ceba92f915dcb472dbc6d70e2581967af0",
    SPEC_FIX / "source-hold.tar.gz": "c01ffa1ef1f3f7ba514b93a3a2cbe7ea2c67d92cfffdfbd8eced973f3fd055f3",
    SPEC_FIX / "final-hold.json": "7e549e645da2f86252a61b8d6223b586d213f3604a80494e885485e9f6ffe799",
    UNOWNED_FIX / "source-manifest.json": "a73f9e5d7b587bce40ab7c6d4a70135540009c73cd657d0a3180bb8064a185bb",
    UNOWNED_FIX / "source-hold.tar.gz": "7fba05262ce872d1e845f8b9c32345ea71b13fdd6042077889354d858bb12f89",
    UNOWNED_FIX / "final-hold.json": "863232acda87ca141cc2c4b62a44e31ff1f596eda4d049261aeb6b7b7b3d0e3f",
    QUALITY_FIX / "source-manifest.json": "375024c646828619b8ddf5d40955db78f416fef69cc14be3b720c727fe2d4e41",
    QUALITY_FIX / "source-hold.tar.gz": "32c628f0fceb06d3b731985d45137b27f6cd76b34a838e8a39611dc28131b135",
    QUALITY_FIX / "final-hold.json": "3b7912899d00f9164f447fc293909f570d9a8c0f101c23d631bd5f930e8d86b9",
    PRODUCT / "target/native-task3/debug/llmgw": "66142c7dd7cd21a500b4075e77bf840283c4741ef178b275a09a5f19d3ba504c",
    PRODUCT / "target/native-task3/release/llmgw": "215511357446c5583ebd5d4a3d9aee6b8ea5df4d83387d31d0ce23ae2a97eb6d",
    PRODUCT / "target/native-task3-lock-fix/debug/llmgw": "9a5fa1d6754c4cb88061434d253429e0b89586b5eedf212de0277fa83227da91",
    PRODUCT / "target/native-task3-lock-fix/release/llmgw": "6b7a35b2b340037891cdf77a7b1c50fadfcc0767677bfcf7ec1fb8595a7c0598",
    PRODUCT / "target/native-task3-spec-fix/debug/llmgw": "a0e5572d4010d338c858acc64d2ef20e3a9355a496efd988803eaa4b3429e706",
    PRODUCT / "target/native-task3-spec-fix/release/llmgw": "4472df4f3fb68000cf31d4eb33edc4fa9faf9176b0fb04b18db3a0d1184573e4",
    PRODUCT / "target/native-task3-unowned-fix/debug/llmgw": "f1252148b2efc1edd67bf51c7210a7f373ee9c30f035bbcf6448da5beb15acc2",
    PRODUCT / "target/native-task3-unowned-fix/release/llmgw": "c325b5ce341626e6b57585a76f2d8cc026f5db3199b651b7c98cd0f6cc2c29a5",
    PRODUCT / "target/native-task3-quality-fix/debug/llmgw": "15eac52098867407b4f06ed3c39e3fc87b69ecd67c063979da2852fac3682e65",
    PRODUCT / "target/native-task3-quality-fix/release/llmgw": "8b6fde7b380d00568b3ce6fbcc2ea21b577cfdc50d730a01955355d8ca16b233",
    QUALITY_REREVIEW / "README.md": "f2971c917e7d575486c46443ad952fc71649394bc25e4c73eb9b7dea8c16f14d",
    QUALITY_REREVIEW / "FINAL_HOLD.json": "1ab40f45d354bba2cc2afc9cf54c8325e9148293e0e7844de10fd72da5021f14",
    QUALITY_REREVIEW / "after.json": "e5b628778289500e94d8b303149cae89d56b3a1b2b5bcb2372046e1853e1ce0a",
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
if len(full_rows) != 15 or sum(full_rows) != 313:
    raise SystemExit(f"bad full rows: {full_rows}")
if rows("patch-contract-final.log") != [49] or rows("patch-contract-release-final.log") != [49]:
    raise SystemExit("bad focused totals")
if rows("quality-probe-final.stdout") != [8]:
    raise SystemExit("adapted quality probe did not pass eight tests")
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
if (OUT / "quality-probe-build-attempt-2.stderr").read_text() or (
    OUT / "quality-probe-final.stderr"
).read_text():
    raise SystemExit("final quality probe stderr not empty")

sentinels = (
    "".join(("upstream-", "synthetic-", "8f2f5a47")),
    "".join(("local-data-", "synthetic-", "3dcb99a1")),
    "".join(("control-", "synthetic-", "6a01c442")),
)
hits = []
for path in [
    *(RESEARCH / item["path"] for item in manifest["files"]),
    *OUT.glob("*.log"),
    *OUT.glob("*.stdout"),
    *OUT.glob("*.stderr"),
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
    if any(
        value in line
        for value in (
            "target/native-task3-nonfinite-fix/debug/llmgw",
            "target/native-task3-nonfinite-fix/release/llmgw",
            "nonfinite-fix/quality-probe-tests",
            "CARGO_TARGET_DIR=/Users/ryuwon/Desktop/ryuwon-project/llm-gateway-research/product/target/native-task3-nonfinite-fix cargo",
        )
    )
]
tmp = pathlib.Path(tempfile.gettempdir())
temp_paths = sorted(
    {
        str(path)
        for pattern in (
            "llmgw-patch-*",
            "llmgw-windows-*",
            "llmgw-config-patch-locks-*",
            "quality-*",
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
    "task3_extra_baseline": str(PROTECTED_EXTRA.relative_to(RESEARCH)),
    "task3_extra_count": len(extra),
    "task3_extra_drift": extra_drift,
    "benchmark_raws": len(bench),
    "accounting_raws": len(accounting),
    "held_identities": [identity(path, str(path.relative_to(RESEARCH))) for path in expected],
}
(OUT / "protected-check.json").write_text(json.dumps(protected_result, indent=2) + "\n")

release = TARGET / "release/llmgw"
debug = TARGET / "debug/llmgw"
probe_rlib = TARGET / "debug/deps/libllmgw-eba4665eb2ddaf45.rlib"
if not probe_rlib.is_file():
    raise SystemExit(f"probe default-feature rlib missing: {probe_rlib}")
rustc = subprocess.run(
    ["rustc", "--version", "--verbose"], capture_output=True, text=True, check=True
).stdout.strip()
cargo = subprocess.run(
    ["cargo", "--version", "--verbose"], capture_output=True, text=True, check=True
).stdout.strip()

commands = {
    "target_dir": str(TARGET),
    "focused_debug": "cargo test --test patch_contract --locked",
    "focused_release": "cargo test --release --test patch_contract --locked",
    "full_debug": "cargo test --locked",
    "check": "cargo check --all-targets --locked",
    "clippy": "cargo clippy --all-targets --locked -- -D warnings",
    "debug_build": "cargo build --locked",
    "release_build": "cargo build --release --locked",
    "probe_rlib": str(probe_rlib),
}
(OUT / "build-execution.json").write_text(json.dumps(commands, indent=2) + "\n")

final = {
    "phase": "FINAL_HOLD",
    "fix": "native-task3 QUALITY Q3 owned TOML nonfinite values",
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
        "source": identity(OUT / "quality_probe.rs"),
        "binary": identity(OUT / "quality-probe-tests-attempt-2"),
        "stdout": identity(OUT / "quality-probe-final.stdout"),
        "all_cases_passed": True,
    },
    "lockfile": identity(PRODUCT / "Cargo.lock"),
    "toolchain_file": identity(PRODUCT / "rust-toolchain.toml"),
    "verification": {
        "full_debug": {"passed": 313, "failed": 0, "rows": full_rows},
        "patch_debug": {"passed": 49, "failed": 0},
        "patch_release": {"passed": 49, "failed": 0},
        "adapted_quality_probe": {"passed": 8, "failed": 0},
        "static": {
            "rustfmt": "passed",
            "check_all_targets": "passed",
            "clippy_all_targets_deny_warnings": "passed",
            "debug_build": "passed",
            "release_build": "passed",
        },
    },
    "red_green": {
        "red": "patch-contract-nonfinite-red.log: 47 passed, 2 failed",
        "green_attempt_1": "49 passed after the AST fix; subsequent error wording retained the established datetime phrase",
        "probe_attempt_1": "6 passed, 2 failed because the sanitized wording no longer contained the established phrase native date or time",
        "green": "patch-contract-final.log: 49 passed, 0 failed against final formatted source",
        "probe_green": "quality-probe-final.stdout: 8 passed, 0 failed",
        "release_green": "patch-contract-release-final.log: 49 passed, 0 failed",
    },
    "finding_resolution": {
        "Q3": "preview inspects each owned TOML AST value or subtree and refuses nonfinite Float values before backup, journal, or client writes; unrelated nonfinite values and owned finite floats remain supported",
    },
    "platform_evidence": {
        "macos_runtime": "focused and full tests cover scalar and nested nan/+inf/-inf refusal, unrelated nonfinite and owned finite round-trip, Q1/Q2, ownership, privacy, ACL, crash, and alias contracts",
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
    "commands": "artifacts/native-task3/nonfinite-fix/build-execution.json",
    "cleanup": "artifacts/native-task3/nonfinite-fix/cleanup-final.json",
    "limits": [
        "all prior Task3 holds, failed probes, held binaries, and reviewer evidence remain immutable",
        "preexisting failed-phase journals were preserved as evidence and no migration was added",
        "external-editor final-check to rename filesystem CAS is not claimed",
        "Windows and Linux runtime remain unverified",
        "native TOML date/time and nonfinite float ownership is explicitly unsupported and refused before write",
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
