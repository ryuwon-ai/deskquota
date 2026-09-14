#!/usr/bin/env python3
"""Build one deterministic, bounded POSIX native archive and SHA-256 manifest."""

from __future__ import annotations

import argparse
import gzip
import hashlib
import io
import json
from pathlib import Path
import tarfile


PRODUCT = Path(__file__).resolve().parents[1]
PACKAGE_MEMBERS = (
    "llmgw",
    "README.md",
    "docs/installation.md",
    "docs/runtime-contract.md",
    "docs/client-compatibility.md",
)


def sha256(path: Path) -> str:
    return hashlib.sha256(path.read_bytes()).hexdigest()


def package_tar(binary: Path, product: Path, output: Path) -> dict[str, object]:
    binary = binary.resolve(strict=True)
    product = product.resolve(strict=True)
    if binary.is_symlink() or not binary.is_file():
        raise ValueError("binary_must_be_a_regular_file")
    if any(character.isspace() for character in output.name):
        raise ValueError("archive_name_must_not_contain_whitespace")
    manifest = output.with_name(output.name + ".sha256")
    if output.exists() or manifest.exists():
        raise FileExistsError("archive_or_manifest_already_exists")

    contents: dict[str, bytes] = {"llmgw": binary.read_bytes()}
    if not contents["llmgw"]:
        raise ValueError("binary_is_empty")
    for name in PACKAGE_MEMBERS[1:]:
        path = (product / name).resolve(strict=True)
        if not path.is_relative_to(product) or path.is_symlink() or not path.is_file():
            raise ValueError(f"invalid_package_document:{name}")
        contents[name] = path.read_bytes()

    output.parent.mkdir(parents=True, exist_ok=True)
    with output.open("xb") as raw:
        with gzip.GzipFile(filename="", mode="wb", fileobj=raw, compresslevel=9, mtime=0) as compressed:
            with tarfile.open(fileobj=compressed, mode="w", format=tarfile.USTAR_FORMAT) as archive:
                for name in PACKAGE_MEMBERS:
                    entry = tarfile.TarInfo(name)
                    entry.size = len(contents[name])
                    entry.mode = 0o755 if name == "llmgw" else 0o644
                    entry.mtime = 0
                    entry.uid = 0
                    entry.gid = 0
                    entry.uname = ""
                    entry.gname = ""
                    archive.addfile(entry, io.BytesIO(contents[name]))

    with tarfile.open(output, "r:gz") as archive:
        members = archive.getmembers()
        if [entry.name for entry in members] != list(PACKAGE_MEMBERS):
            raise ValueError("package_member_mismatch")
        for entry in members:
            if not entry.isfile() or archive.extractfile(entry).read() != contents[entry.name]:
                raise ValueError(f"package_content_mismatch:{entry.name}")

    archive_hash = sha256(output)
    with manifest.open("x", encoding="ascii") as stream:
        stream.write(f"{archive_hash}  {output.name}\n")
    return {
        "archive": str(output.resolve()),
        "archive_bytes": output.stat().st_size,
        "archive_sha256": archive_hash,
        "binary_sha256": hashlib.sha256(contents["llmgw"]).hexdigest(),
        "manifest": str(manifest.resolve()),
        "manifest_sha256": sha256(manifest),
        "members": PACKAGE_MEMBERS,
    }


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--binary", type=Path, required=True)
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument("--identity-output", type=Path)
    args = parser.parse_args()
    result = package_tar(args.binary, PRODUCT, args.output)
    rendered = json.dumps(result, indent=2) + "\n"
    if args.identity_output:
        args.identity_output.parent.mkdir(parents=True, exist_ok=True)
        with args.identity_output.open("x", encoding="utf-8") as stream:
            stream.write(rendered)
    print(rendered, end="")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
