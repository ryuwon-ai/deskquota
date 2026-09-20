#!/usr/bin/env python3
"""Build one deterministic native tar.gz or ZIP and SHA-256 manifest."""

from __future__ import annotations

import argparse
import gzip
import hashlib
import io
import json
from pathlib import Path
import tarfile
import zipfile


PRODUCT = Path(__file__).resolve().parents[1]
PACKAGE_MEMBERS = (
    "llmgw",
    "README.md",
    "LICENSE-MIT",
    "LICENSE-APACHE",
)


def sha256(path: Path) -> str:
    return hashlib.sha256(path.read_bytes()).hexdigest()


def package_native(binary: Path, product: Path, output: Path) -> dict[str, object]:
    if not (output.name.endswith(".tar.gz") or output.suffix == ".zip"):
        raise ValueError("archive_must_be_tar_gz_or_zip")
    if binary.is_symlink():
        raise ValueError("binary_must_be_a_regular_file")
    binary = binary.resolve(strict=True)
    product = product.resolve(strict=True)
    if binary.is_symlink() or not binary.is_file():
        raise ValueError("binary_must_be_a_regular_file")
    if any(character.isspace() for character in output.name):
        raise ValueError("archive_name_must_not_contain_whitespace")
    manifest = output.with_name(output.name + ".sha256")
    if output.exists() or manifest.exists():
        raise FileExistsError("archive_or_manifest_already_exists")

    executable = "llmgw.exe" if output.suffix == ".zip" else "llmgw"
    names = (executable, *PACKAGE_MEMBERS[1:])
    contents: dict[str, bytes] = {executable: binary.read_bytes()}
    if not contents[executable]:
        raise ValueError("binary_is_empty")
    for name in PACKAGE_MEMBERS[1:]:
        directory = product if name == "README.md" else product.parent
        source = directory / name
        path = source.resolve(strict=True)
        if not path.is_relative_to(directory) or source.is_symlink() or not path.is_file():
            raise ValueError(f"invalid_package_document:{name}")
        contents[name] = path.read_bytes()

    output.parent.mkdir(parents=True, exist_ok=True)
    if output.suffix == ".zip":
        with zipfile.ZipFile(output, "x", compression=zipfile.ZIP_DEFLATED, compresslevel=9) as archive:
            for name in names:
                entry = zipfile.ZipInfo(name, date_time=(1980, 1, 1, 0, 0, 0))
                entry.create_system = 3
                entry.external_attr = (0o100755 if name == executable else 0o100644) << 16
                archive.writestr(entry, contents[name], compress_type=zipfile.ZIP_DEFLATED, compresslevel=9)
        with zipfile.ZipFile(output) as archive:
            if archive.namelist() != list(names) or any(archive.read(n) != contents[n] for n in names):
                raise ValueError("package_content_mismatch")
    else:
        with output.open("xb") as raw:
            with gzip.GzipFile(filename="", mode="wb", fileobj=raw, compresslevel=9, mtime=0) as compressed:
                with tarfile.open(fileobj=compressed, mode="w", format=tarfile.USTAR_FORMAT) as archive:
                    for name in names:
                        entry = tarfile.TarInfo(name)
                        entry.size = len(contents[name])
                        entry.mode = 0o755 if name == executable else 0o644
                        entry.mtime = 0
                        archive.addfile(entry, io.BytesIO(contents[name]))
        with tarfile.open(output, "r:gz") as archive:
            if archive.getnames() != list(names) or any(archive.extractfile(n).read() != contents[n] for n in names):
                raise ValueError("package_content_mismatch")

    archive_hash = sha256(output)
    with manifest.open("x", encoding="ascii") as stream:
        stream.write(f"{archive_hash}  {output.name}\n")
    return {
        "archive": str(output.resolve()),
        "archive_bytes": output.stat().st_size,
        "archive_sha256": archive_hash,
        "binary_sha256": hashlib.sha256(contents[executable]).hexdigest(),
        "manifest": str(manifest.resolve()),
        "manifest_sha256": sha256(manifest),
        "members": names,
    }


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--binary", type=Path, required=True)
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument("--identity-output", type=Path)
    args = parser.parse_args()
    result = package_native(args.binary, PRODUCT, args.output)
    rendered = json.dumps(result, indent=2) + "\n"
    if args.identity_output:
        args.identity_output.parent.mkdir(parents=True, exist_ok=True)
        with args.identity_output.open("x", encoding="utf-8") as stream:
            stream.write(rendered)
    print(rendered, end="")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
