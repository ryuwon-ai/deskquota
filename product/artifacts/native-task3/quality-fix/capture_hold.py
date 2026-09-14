#!/usr/bin/env python3
import datetime
import gzip
import hashlib
import io
import json
import pathlib
import tarfile

OUT = pathlib.Path(__file__).resolve().parent
PRODUCT = OUT.parents[2]
RESEARCH = PRODUCT.parent
TASK2 = PRODUCT / "artifacts/native-task2/quality-fix"
UNOWNED_FIX = PRODUCT / "artifacts/native-task3/unowned-fix"


def digest_bytes(data):
    return hashlib.sha256(data).hexdigest()


def digest(path):
    return digest_bytes(path.read_bytes())


source_paths = (UNOWNED_FIX / "source-files.txt").read_text().splitlines()
if len(source_paths) != 79 or len(set(source_paths)) != 79:
    raise SystemExit(f"expected 79 unique source files, found {len(source_paths)}")

files = []
for relative in source_paths:
    path = RESEARCH / relative
    data = path.read_bytes()
    files.append({"path": relative, "bytes": len(data), "sha256": digest_bytes(data)})

manifest = {
    "phase": "native-task3-quality-fix-final-hold",
    "captured_at": datetime.datetime.now(datetime.timezone.utc).isoformat(),
    "files": files,
}
(OUT / "source-files.txt").write_text("\n".join(source_paths) + "\n")
(OUT / "source-manifest.json").write_text(json.dumps(manifest, indent=2) + "\n")


def changed_against(path):
    before = {item["path"]: item for item in json.loads(path.read_text())["files"]}
    after = {item["path"]: item for item in files}
    changed = []
    for relative in sorted(before.keys() | after.keys()):
        old = before.get(relative)
        new = after.get(relative)
        if old is not None and new is not None and old["sha256"] == new["sha256"]:
            continue
        changed.append(
            {
                "path": relative,
                "change": "added" if old is None else "removed" if new is None else "modified",
                "before_sha256": None if old is None else old["sha256"],
                "after_sha256": None if new is None else new["sha256"],
            }
        )
    return changed


task2_changed = changed_against(TASK2 / "source-manifest.json")
quality_changed = changed_against(UNOWNED_FIX / "source-manifest.json")
if len(task2_changed) != 12:
    raise SystemExit(f"expected 12 Task2-relative changes, got {len(task2_changed)}")
expected = {
    "product/src/config_patch/document.rs",
    "product/src/config_patch/storage.rs",
    "product/tests/patch_contract.rs",
}
if {item["path"] for item in quality_changed} != expected:
    raise SystemExit(f"unexpected quality-fix paths: {quality_changed}")

for name, payload in [
    (
        "changed-files.json",
        {
            "baseline": "native-task2 quality-fix FINAL_HOLD",
            "baseline_manifest_sha256": digest(TASK2 / "source-manifest.json"),
            "current_source_files": 79,
            "changed_files": task2_changed,
        },
    ),
    (
        "quality-fix-changed-files.json",
        {
            "baseline": "native-task3 unowned-fix FINAL_HOLD",
            "baseline_manifest_sha256": digest(UNOWNED_FIX / "source-manifest.json"),
            "current_source_files": 79,
            "changed_files": quality_changed,
        },
    ),
]:
    (OUT / name).write_text(json.dumps(payload, indent=2) + "\n")

archive = OUT / "source-hold.tar.gz"
with archive.open("wb") as raw:
    with gzip.GzipFile(filename="", mode="wb", fileobj=raw, mtime=0) as zipped:
        with tarfile.open(fileobj=zipped, mode="w") as tar:
            for relative in source_paths:
                path = RESEARCH / relative
                data = path.read_bytes()
                stat = path.stat()
                info = tarfile.TarInfo(relative)
                info.size = len(data)
                info.mode = stat.st_mode & 0o777
                info.mtime = 0
                info.uid = 0
                info.gid = 0
                info.uname = ""
                info.gname = ""
                tar.addfile(info, io.BytesIO(data))

summary = {
    "source_files": 79,
    "changed_files_from_task2": len(task2_changed),
    "changed_files_from_unowned_fix": len(quality_changed),
    "source_manifest_sha256": digest(OUT / "source-manifest.json"),
    "changed_files_manifest_sha256": digest(OUT / "changed-files.json"),
    "quality_fix_changed_files_manifest_sha256": digest(OUT / "quality-fix-changed-files.json"),
    "source_archive_sha256": digest(archive),
    "source_archive_bytes": archive.stat().st_size,
}
(OUT / "capture-summary.json").write_text(json.dumps(summary, indent=2) + "\n")
print(json.dumps(summary, indent=2))
