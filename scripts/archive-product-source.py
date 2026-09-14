#!/usr/bin/env python3
"""Archive only fingerprinted source inputs so later edits preserve reproducibility."""

import argparse
import hashlib
import io
import json
from pathlib import Path
import tarfile


ROOT = Path(__file__).resolve().parents[1]


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--manifest", required=True, type=Path)
    parser.add_argument("--output", required=True, type=Path)
    args = parser.parse_args()
    manifest_bytes = args.manifest.read_bytes()
    manifest = json.loads(manifest_bytes)
    members = {"source-manifest.json": manifest_bytes}
    for row in manifest["files"]:
        path = (ROOT / row["path"]).resolve(strict=True)
        if not path.is_relative_to(ROOT / "product"):
            raise ValueError("source_path_outside_product")
        name = path.relative_to(ROOT).as_posix()
        if name in members:
            raise ValueError("duplicate_source_path")
        content = path.read_bytes()
        if hashlib.sha256(content).hexdigest() != row["sha256"]:
            raise ValueError("source_hash_mismatch")
        members[name] = content
    args.output.parent.mkdir(parents=True, exist_ok=True)
    # Exclusive creation preserves every prior held-source artifact.
    with args.output.open("xb") as output:
        with tarfile.open(fileobj=output, mode="w:gz") as archive:
            for name, content in sorted(members.items()):
                entry = tarfile.TarInfo(name)
                entry.size = len(content)
                entry.mode = 0o644
                archive.addfile(entry, io.BytesIO(content))
    with tarfile.open(args.output, "r:gz") as archive:
        actual = {}
        for member in archive.getmembers():
            if not member.isfile() or member.name in actual:
                raise ValueError("unexpected_archive_member")
            actual[member.name] = archive.extractfile(member).read()
        if actual != members:
            raise ValueError("archive_verification_failed")
    print(json.dumps({
        "passed": True, "source_files": len(manifest["files"]),
        "archive": str(args.output), "bytes": args.output.stat().st_size,
        "sha256": hashlib.sha256(args.output.read_bytes()).hexdigest(),
        "scope": "fingerprinted_source_only_no_binaries_or_runtime_state",
        "limitations": ["Source snapshot, not a Git commit or build provenance proof"],
    }))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
