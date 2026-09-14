#!/usr/bin/env python3
import datetime
import gzip
import hashlib
import io
import json
import pathlib
import tarfile


PRODUCT = pathlib.Path(__file__).resolve().parents[3]
RESEARCH = PRODUCT.parent
OUT = pathlib.Path(__file__).resolve().parent
INITIAL = PRODUCT / "artifacts/native-task2/spec-fix"


def digest(data):
    return hashlib.sha256(data).hexdigest()


source_paths = (INITIAL / "source-files.txt").read_text().splitlines()
source_paths = sorted(set(source_paths))
if len(source_paths) != 70:
    raise SystemExit(f"expected 70 source files, found {len(source_paths)}")

files = []
for relative in source_paths:
    path = RESEARCH / relative
    data = path.read_bytes()
    files.append({"path": relative, "bytes": len(data), "sha256": digest(data)})

manifest = {
    "phase": "native-task2-f8-fix-final-hold",
    "captured_at": datetime.datetime.now(datetime.timezone.utc).isoformat(),
    "files": files,
}
(OUT / "source-files.txt").write_text("\n".join(source_paths) + "\n")
(OUT / "source-manifest.json").write_text(
    json.dumps(manifest, indent=2, ensure_ascii=False) + "\n"
)

initial_manifest = json.loads((INITIAL / "source-manifest.json").read_text())
before = {item["path"]: item for item in initial_manifest["files"]}
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
changed_manifest = {
    "baseline": "native-task2 spec-fix FINAL_HOLD",
    "baseline_manifest_sha256": digest((INITIAL / "source-manifest.json").read_bytes()),
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
