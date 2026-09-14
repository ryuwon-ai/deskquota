#!/usr/bin/env python3
"""Independent bounded Native Task 6 SPEC review probes."""

from __future__ import annotations

import argparse
import contextlib
import functools
import hashlib
import http.server
import io
import json
import os
from pathlib import Path
import shutil
import subprocess
import tarfile
import tempfile
import threading
from datetime import datetime, timezone


RESEARCH = Path(__file__).resolve().parents[2]
PRODUCT = RESEARCH / "product"
FINAL = PRODUCT / "artifacts/native-task6/final-v2"
OUT = Path(__file__).resolve().parent
INSTALLER = PRODUCT / "packaging/install.sh"
ARCHIVE = FINAL / "llmgw-macos-arm64.tar.gz"
CHECKSUM = FINAL / "llmgw-macos-arm64.tar.gz.sha256"
BINARY = FINAL / "llmgw-macos-arm64"
EXPECTED_BINARY_SHA256 = "cf7c436c442c426a6e9a1d485a5261fe5e5a05c8b131b130e679e00eec2c646b"
EXPECTED_PACKAGE_SHA256 = "902aa7e58f7292c82ba1777065398060c393d331652709a544919f6410e6e902"
EXPECTED_SOURCE_MANIFEST_SHA256 = "1981823429bc99e1cb776b9a007a8ea7828ae9cd2337b7d97569f442a5d95e33"
EXPECTED_SOURCE_ARCHIVE_SHA256 = "0b9e0a9ab118327898f3fc01d650d1248686c8b1efe1f0a5be8557b62859a3c9"
EXPECTED_FINAL_HOLD_SHA256 = "ff1a9d56b1704b39dfe1511974a070c70c44b35a217a8ca107210267ae48d68c"
PACKAGE_MEMBERS = (
    "llmgw",
    "README.md",
    "docs/installation.md",
    "docs/runtime-contract.md",
    "docs/client-compatibility.md",
)


def now() -> str:
    return datetime.now(timezone.utc).isoformat()


def sha256(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as stream:
        for block in iter(lambda: stream.read(1024 * 1024), b""):
            digest.update(block)
    return digest.hexdigest()


def row(path: Path, relative_to: Path = RESEARCH) -> dict[str, object]:
    return {
        "path": path.relative_to(relative_to).as_posix(),
        "bytes": path.stat().st_size,
        "sha256": sha256(path),
    }


def compare_rows(rows: list[dict[str, object]], root: Path) -> list[dict[str, object]]:
    drift: list[dict[str, object]] = []
    for expected in rows:
        path = root / str(expected["path"])
        if not path.is_file():
            drift.append({"path": expected["path"], "error": "missing_or_not_regular"})
            continue
        actual = {"bytes": path.stat().st_size, "sha256": sha256(path)}
        if actual["bytes"] != expected["bytes"] or actual["sha256"] != expected["sha256"]:
            drift.append({"path": expected["path"], "expected": expected, "actual": actual})
    return drift


def identity_snapshot(label: str) -> dict[str, object]:
    source_manifest_path = FINAL / "source-manifest.json"
    source_manifest = json.loads(source_manifest_path.read_text(encoding="utf-8"))
    source_rows = source_manifest["files"]
    source_drift = compare_rows(source_rows, RESEARCH)

    source_archive = FINAL / "source.tar.gz"
    archive_errors: list[str] = []
    with tarfile.open(source_archive, "r:gz") as archive:
        members = archive.getmembers()
        names = [member.name for member in members]
        if len(names) != len(set(names)):
            archive_errors.append("duplicate_source_archive_member")
        if any(not member.isfile() for member in members):
            archive_errors.append("non_file_source_archive_member")
        expected_names = [str(item["path"]) for item in source_rows] + ["source-manifest.json"]
        if set(names) != set(expected_names) or len(names) != 102:
            archive_errors.append("source_archive_member_set_mismatch")
        embedded = archive.extractfile("source-manifest.json")
        if embedded is None or embedded.read() != source_manifest_path.read_bytes():
            archive_errors.append("embedded_manifest_mismatch")
        by_name = {member.name: member for member in members}
        for expected in source_rows:
            member = by_name.get(str(expected["path"]))
            stream = archive.extractfile(member) if member is not None else None
            if stream is None:
                archive_errors.append(f"missing_source_member:{expected['path']}")
                continue
            payload = stream.read()
            if len(payload) != expected["bytes"] or hashlib.sha256(payload).hexdigest() != expected["sha256"]:
                archive_errors.append(f"source_archive_content_mismatch:{expected['path']}")

    package_errors: list[str] = []
    with tarfile.open(ARCHIVE, "r:gz") as archive:
        members = archive.getmembers()
        names = [member.name for member in members]
        if names != list(PACKAGE_MEMBERS):
            package_errors.append("package_member_order_or_set_mismatch")
        if len(names) != len(set(names)) or any(not member.isfile() for member in members):
            package_errors.append("package_duplicate_or_non_file")
        for member in members:
            stream = archive.extractfile(member)
            payload = stream.read() if stream is not None else b""
            expected_path = BINARY if member.name == "llmgw" else PRODUCT / member.name
            if payload != expected_path.read_bytes():
                package_errors.append(f"package_content_mismatch:{member.name}")

    parent = json.loads((RESEARCH / "evidence/native-task6-parent-hold-check.json").read_text(encoding="utf-8"))
    prior = json.loads((FINAL / "preservation-before.json").read_text(encoding="utf-8"))
    parent_drift = compare_rows(parent["preserve_task6"], RESEARCH)
    extras_drift = compare_rows(parent["preserve_final_build_extras"], RESEARCH)
    prior_drift = compare_rows(prior["files"], RESEARCH)

    checks = {
        "source_files": len(source_rows),
        "source_live_drift": source_drift,
        "source_manifest_sha256": sha256(source_manifest_path),
        "source_archive_members": 102,
        "source_archive_errors": archive_errors,
        "source_archive_sha256": sha256(source_archive),
        "package_members": len(PACKAGE_MEMBERS),
        "package_errors": package_errors,
        "package_bytes": ARCHIVE.stat().st_size,
        "package_sha256": sha256(ARCHIVE),
        "binary_bytes": BINARY.stat().st_size,
        "binary_sha256": sha256(BINARY),
        "final_hold_sha256": sha256(FINAL / "FINAL_HOLD.json"),
        "parent_task6_rows": len(parent["preserve_task6"]),
        "parent_task6_drift": parent_drift,
        "parent_extra_rows": len(parent["preserve_final_build_extras"]),
        "parent_extra_drift": extras_drift,
        "prior_rows": len(prior["files"]),
        "prior_drift": prior_drift,
    }
    passed = (
        not source_drift
        and not archive_errors
        and not package_errors
        and not parent_drift
        and not extras_drift
        and not prior_drift
        and checks["source_manifest_sha256"] == EXPECTED_SOURCE_MANIFEST_SHA256
        and checks["source_archive_sha256"] == EXPECTED_SOURCE_ARCHIVE_SHA256
        and checks["package_sha256"] == EXPECTED_PACKAGE_SHA256
        and checks["package_bytes"] == 4_202_300
        and checks["binary_sha256"] == EXPECTED_BINARY_SHA256
        and checks["binary_bytes"] == 10_011_264
        and checks["final_hold_sha256"] == EXPECTED_FINAL_HOLD_SHA256
    )
    return {"at": now(), "label": label, "passed": passed, **checks}


class QuietHandler(http.server.SimpleHTTPRequestHandler):
    def log_message(self, _format: str, *_args: object) -> None:
        return


@contextlib.contextmanager
def release_server(directory: Path):
    handler = functools.partial(QuietHandler, directory=str(directory))
    server = http.server.ThreadingHTTPServer(("127.0.0.1", 0), handler)
    thread = threading.Thread(target=server.serve_forever, daemon=True)
    thread.start()
    try:
        yield f"http://127.0.0.1:{server.server_port}"
    finally:
        server.shutdown()
        server.server_close()
        thread.join(timeout=5)


def run_installer(home: Path, archive: str, manifest: str, install_dir: Path, action: str = "none") -> dict[str, object]:
    env = {
        "HOME": str(home),
        "PATH": "/usr/bin:/bin:/usr/sbin:/sbin",
        "TMPDIR": str(home / "tmp"),
        "LC_ALL": "C",
    }
    (home / "tmp").mkdir(parents=True, exist_ok=True)
    command = [
        "/bin/sh", str(INSTALLER), "--archive", archive,
        "--checksum-manifest", manifest, "--install-dir", str(install_dir),
        "--path-action", action,
    ]
    completed = subprocess.run(command, env=env, text=True, capture_output=True, timeout=20, check=False)
    return {
        "command": command,
        "exit_code": completed.returncode,
        "stdout": completed.stdout,
        "stderr": completed.stderr,
    }


def write_tar(path: Path, entries: list[tuple[str, bytes, str]]) -> Path:
    with tarfile.open(path, "w:gz") as archive:
        for name, payload, kind in entries:
            info = tarfile.TarInfo(name)
            if kind == "file":
                info.size = len(payload)
                info.mode = 0o755 if name == "llmgw" else 0o644
                archive.addfile(info, io.BytesIO(payload))
            elif kind == "symlink":
                info.type = tarfile.SYMTYPE
                info.linkname = "/tmp/never-read"
                archive.addfile(info)
            else:
                raise ValueError(kind)
    manifest = path.with_name(path.name + ".sha256")
    manifest.write_text(f"{sha256(path)}  {path.name}\n", encoding="ascii")
    return manifest


def installer_probe() -> dict[str, object]:
    result: dict[str, object] = {"at": now(), "binary_sha256": EXPECTED_BINARY_SHA256}
    with tempfile.TemporaryDirectory(prefix="llmgw-task6-spec-review-") as temporary:
        root = Path(temporary)
        home = root / "home"
        home.mkdir()
        profile = home / ".profile"
        profile.write_text("# reviewer sentinel\n", encoding="utf-8")
        install_dir = home / "bin quote'-$HOME-`false`"

        local = run_installer(home, str(ARCHIVE), str(CHECKSUM), install_dir, "preview")
        target = install_dir / "llmgw"
        path_lines = [line.strip() for line in str(local["stdout"]).splitlines() if line.strip().startswith("export PATH=")]
        child = subprocess.run(
            ["/bin/sh", "-c", path_lines[0] + "\ncommand -v llmgw\nllmgw --version"],
            env={"HOME": str(home), "PATH": "/usr/bin:/bin", "LC_ALL": "C"},
            text=True, capture_output=True, timeout=10, check=False,
        )
        reinstall = run_installer(home, str(ARCHIVE), str(CHECKSUM), install_dir)
        result["local_actual_package"] = {
            "install": local,
            "installed_sha256": sha256(target),
            "installed_executable": os.access(target, os.X_OK),
            "profile_unchanged": profile.read_text(encoding="utf-8") == "# reviewer sentinel\n",
            "exact_path_line_count": len(path_lines),
            "child_exit": child.returncode,
            "child_stdout": child.stdout,
            "child_stderr": child.stderr,
            "reinstall_exit": reinstall["exit_code"],
            "reinstall_sha256": sha256(target),
        }

        sentinel = b"#!/bin/sh\necho preserved-reviewer-binary\n"
        target.write_bytes(sentinel)
        target.chmod(0o755)
        bad_manifest = root / "bad.sha256"
        bad_manifest.write_text(f"{'0' * 64}  {ARCHIVE.name}\n", encoding="ascii")
        wrong_hash = run_installer(home, str(ARCHIVE), str(bad_manifest), install_dir)
        missing = run_installer(home, str(root / "missing.tar.gz"), str(CHECKSUM), install_dir)
        result["pre_replace_failures"] = {
            "wrong_hash_exit": wrong_hash["exit_code"],
            "wrong_hash_stderr": wrong_hash["stderr"],
            "wrong_hash_preserved": target.read_bytes() == sentinel and os.access(target, os.X_OK),
            "missing_exit": missing["exit_code"],
            "missing_stderr": missing["stderr"],
            "missing_preserved": target.read_bytes() == sentinel and os.access(target, os.X_OK),
        }

        invalid_results: dict[str, object] = {}
        base_docs = [(name, b"doc", "file") for name in PACKAGE_MEMBERS[1:]]
        cases = {
            "traversal": [("llmgw", b"new", "file"), ("../escape", b"x", "file")],
            "symlink": [("llmgw", b"", "symlink")],
            "duplicate": [("llmgw", b"one", "file"), ("llmgw", b"two", "file")],
            "unexpected": [("llmgw", b"new", "file"), ("secret.env", b"x", "file")],
        }
        for name, entries in cases.items():
            archive = root / f"{name}.tar.gz"
            manifest = write_tar(archive, entries + ([] if name in {"symlink", "duplicate"} else base_docs))
            probe = run_installer(home, str(archive), str(manifest), install_dir)
            invalid_results[name] = {
                "exit_code": probe["exit_code"],
                "stderr": probe["stderr"],
                "existing_preserved": target.read_bytes() == sentinel and os.access(target, os.X_OK),
                "escape_absent": not (root / "escape").exists(),
            }
        result["invalid_archives"] = invalid_results

        http_home = root / "http-home"
        http_home.mkdir()
        http_install = http_home / "bin"
        with release_server(FINAL) as base:
            http_probe = run_installer(
                http_home,
                f"{base}/{ARCHIVE.name}",
                f"{base}/{CHECKSUM.name}",
                http_install,
            )
        result["fake_loopback_release"] = {
            "exit_code": http_probe["exit_code"],
            "stdout": http_probe["stdout"],
            "stderr": http_probe["stderr"],
            "installed_sha256": sha256(http_install / "llmgw") if (http_install / "llmgw").is_file() else None,
        }
        result["owned_temp_cleanup_pending_inside_probe"] = True

    result["owned_temp_cleanup_verified_after_context"] = not Path(temporary).exists()
    local_result = result["local_actual_package"]
    pre = result["pre_replace_failures"]
    invalid = result["invalid_archives"]
    fake = result["fake_loopback_release"]
    result["passed"] = bool(
        local_result["install"]["exit_code"] == 0
        and local_result["installed_sha256"] == EXPECTED_BINARY_SHA256
        and local_result["installed_executable"]
        and local_result["profile_unchanged"]
        and local_result["exact_path_line_count"] == 1
        and local_result["child_exit"] == 0
        and "llmgw 0.1.0" in local_result["child_stdout"]
        and local_result["reinstall_exit"] == 0
        and local_result["reinstall_sha256"] == EXPECTED_BINARY_SHA256
        and pre["wrong_hash_exit"] != 0 and pre["wrong_hash_preserved"]
        and pre["missing_exit"] != 0 and pre["missing_preserved"]
        and all(item["exit_code"] != 0 and item["existing_preserved"] and item["escape_absent"] for item in invalid.values())
        and fake["exit_code"] == 0 and fake["installed_sha256"] == EXPECTED_BINARY_SHA256
        and result["owned_temp_cleanup_verified_after_context"]
    )
    return result


def write_exclusive(name: str, payload: object) -> None:
    path = OUT / name
    with path.open("x", encoding="utf-8") as stream:
        json.dump(payload, stream, indent=2)
        stream.write("\n")


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("mode", choices=("before", "installer", "after"))
    args = parser.parse_args()
    if args.mode == "before":
        payload = identity_snapshot("before_runtime")
        write_exclusive("identity-before.json", payload)
    elif args.mode == "installer":
        payload = installer_probe()
        write_exclusive("installer-probe.json", payload)
    else:
        payload = identity_snapshot("after_runtime")
        write_exclusive("identity-after.json", payload)
    print(json.dumps({"mode": args.mode, "passed": payload["passed"]}))
    return 0 if payload["passed"] else 1


if __name__ == "__main__":
    raise SystemExit(main())
