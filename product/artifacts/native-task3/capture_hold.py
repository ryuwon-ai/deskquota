#!/usr/bin/env python3
import datetime
import gzip
import hashlib
import io
import json
import pathlib
import tarfile


OUT = pathlib.Path(__file__).resolve().parent
PRODUCT = OUT.parents[1]
RESEARCH = PRODUCT.parent
BASE = PRODUCT / "artifacts/native-task2/quality-fix"


def digest(data):
    return hashlib.sha256(data).hexdigest()


source_paths = (BASE / "source-files.txt").read_text().splitlines()
source_paths.extend(
    [
        "product/src/config_patch/apply.rs",
        "product/src/config_patch/document.rs",
        "product/src/config_patch/journal.rs",
        "product/src/config_patch/mod.rs",
        "product/src/config_patch/restore.rs",
        "product/src/config_patch/storage.rs",
        "product/src/file_replace.rs",
        "product/tests/patch_contract.rs",
    ]
)
source_paths = sorted(set(source_paths))
if len(source_paths) != 79:
    raise SystemExit(f"expected 79 source files, found {len(source_paths)}")

files = []
for relative in source_paths:
    path = RESEARCH / relative
    data = path.read_bytes()
    files.append({"path": relative, "bytes": len(data), "sha256": digest(data)})

manifest = {
    "phase": "native-task3-final-hold",
    "captured_at": datetime.datetime.now(datetime.timezone.utc).isoformat(),
    "files": files,
}
(OUT / "source-files.txt").write_text("\n".join(source_paths) + "\n")
(OUT / "source-manifest.json").write_text(
    json.dumps(manifest, indent=2, ensure_ascii=False) + "\n"
)

base_manifest = json.loads((BASE / "source-manifest.json").read_text())
before = {item["path"]: item for item in base_manifest["files"]}
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
if len(changed) != 12:
    raise SystemExit(f"expected 12 changed files, found {len(changed)}")
changed_manifest = {
    "baseline": "native-task2 quality-fix FINAL_HOLD",
    "baseline_manifest_sha256": digest((BASE / "source-manifest.json").read_bytes()),
    "current_source_files": len(files),
    "changed_files": changed,
}
(OUT / "changed-files.json").write_text(
    json.dumps(changed_manifest, indent=2, ensure_ascii=False) + "\n"
)

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
    "source_files": len(files),
    "changed_files": len(changed),
    "source_manifest_sha256": digest((OUT / "source-manifest.json").read_bytes()),
    "changed_files_manifest_sha256": digest((OUT / "changed-files.json").read_bytes()),
    "source_archive_sha256": digest(archive.read_bytes()),
    "source_archive_bytes": archive.stat().st_size,
}
(OUT / "capture-summary.json").write_text(
    json.dumps(summary, indent=2, ensure_ascii=False) + "\n"
)
print(json.dumps(summary, indent=2))
