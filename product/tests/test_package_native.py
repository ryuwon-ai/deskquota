#!/usr/bin/env python3
import hashlib
import importlib.util
from pathlib import Path
import tarfile
import tempfile
import unittest
import zipfile


PRODUCT = Path(__file__).resolve().parents[1]
SCRIPT = PRODUCT / "scripts" / "package_native.py"


def load_module():
    spec = importlib.util.spec_from_file_location("package_native", SCRIPT)
    assert spec and spec.loader
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


class PackageNativeTests(unittest.TestCase):
    def test_binary_identity_is_bound_to_captured_archive_member(self):
        module = load_module()
        with tempfile.TemporaryDirectory(prefix="llmgw-package-interleave-") as raw:
            root = Path(raw)
            product = root / "product"
            (product / "docs").mkdir(parents=True)
            for name in (
                "README.md",
                "docs/installation.md",
                "docs/runtime-contract.md",
                "docs/client-compatibility.md",
            ):
                (product / name).write_text(name, encoding="utf-8")
            binary = root / "llmgw"
            captured = b"captured-archive-member"
            replacement = b"concurrent-path-replacement"
            binary.write_bytes(captured)
            output = root / "llmgw-interleave.tar.gz"

            original_sha256 = module.sha256
            replaced = False

            def replace_after_archive_hash(path):
                nonlocal replaced
                digest = original_sha256(path)
                if path == output and not replaced:
                    binary.write_bytes(replacement)
                    replaced = True
                return digest

            module.sha256 = replace_after_archive_hash
            try:
                result = module.package_native(binary, product, output)
            finally:
                module.sha256 = original_sha256

            with tarfile.open(output, "r:gz") as archive:
                member = archive.extractfile("llmgw").read()
            self.assertTrue(replaced)
            self.assertEqual(member, captured)
            self.assertEqual(binary.read_bytes(), replacement)
            self.assertEqual(result["binary_sha256"], hashlib.sha256(member).hexdigest())

    def test_tar_contains_only_binary_and_required_user_docs(self):
        module = load_module()
        with tempfile.TemporaryDirectory(prefix="llmgw-package-test-") as raw:
            root = Path(raw)
            product = root / "product"
            (product / "docs").mkdir(parents=True)
            for name in ("README.md", "docs/installation.md", "docs/runtime-contract.md", "docs/client-compatibility.md"):
                path = product / name
                path.parent.mkdir(parents=True, exist_ok=True)
                path.write_text(f"fixture {name}\n", encoding="utf-8")
            binary = root / "llmgw"
            binary.write_bytes(b"fixture-native-binary")
            output = root / "llmgw-test.tar.gz"
            result = module.package_native(binary, product, output)
            self.assertEqual(result["members"], module.PACKAGE_MEMBERS)
            with tarfile.open(output, "r:gz") as archive:
                members = archive.getmembers()
                self.assertEqual([entry.name for entry in members], list(module.PACKAGE_MEMBERS))
                self.assertTrue(all(entry.isfile() for entry in members))
                self.assertEqual(archive.extractfile("llmgw").read(), binary.read_bytes())
                self.assertEqual(archive.getmember("llmgw").mode, 0o755)
            manifest = output.with_name(output.name + ".sha256")
            digest, name = manifest.read_text(encoding="ascii").split()
            self.assertEqual(digest, hashlib.sha256(output.read_bytes()).hexdigest())
            self.assertEqual(name, output.name)

    def test_packaging_is_deterministic_and_never_overwrites(self):
        module = load_module()
        with tempfile.TemporaryDirectory(prefix="llmgw-package-deterministic-") as raw:
            root = Path(raw)
            product = root / "product"
            (product / "docs").mkdir(parents=True)
            for name in ("README.md", "docs/installation.md", "docs/runtime-contract.md", "docs/client-compatibility.md"):
                (product / name).write_text(name, encoding="utf-8")
            binary = root / "llmgw"
            binary.write_bytes(b"same")
            first = root / "first.tar.gz"
            second = root / "second.tar.gz"
            module.package_native(binary, product, first)
            module.package_native(binary, product, second)
            self.assertEqual(first.read_bytes(), second.read_bytes())
            with self.assertRaises(FileExistsError):
                module.package_native(binary, product, first)


    def test_zip_is_deterministic_bounded_and_checksummed(self):
        module = load_module()
        with tempfile.TemporaryDirectory() as raw:
            root = Path(raw)
            product = root / "product"
            (product / "docs").mkdir(parents=True)
            for name in module.PACKAGE_MEMBERS[1:]:
                (product / name).write_text(name, encoding="utf-8")
            binary = root / "llmgw.exe"
            binary.write_bytes(b"native-fixture")
            first, second = root / "first.zip", root / "second.zip"
            result = module.package_native(binary, product, first)
            module.package_native(binary, product, second)
            self.assertEqual(first.read_bytes(), second.read_bytes())
            with zipfile.ZipFile(first) as archive:
                self.assertEqual(archive.namelist(), ["llmgw.exe", *module.PACKAGE_MEMBERS[1:]])
                self.assertEqual(archive.read("llmgw.exe"), binary.read_bytes())
                self.assertEqual(result["binary_sha256"], hashlib.sha256(archive.read("llmgw.exe")).hexdigest())
            self.assertEqual(first.with_suffix(".zip.sha256").read_text(),
                             f"{hashlib.sha256(first.read_bytes()).hexdigest()}  first.zip\n")
            with self.assertRaises(FileExistsError):
                module.package_native(binary, product, first)
            with self.assertRaises(ValueError):
                module.package_native(binary, product, root / "bad.txt")
            binary.write_bytes(b"")
            with self.assertRaises(ValueError):
                module.package_native(binary, product, root / "empty.zip")
            self.assertFalse((root / "empty.zip").exists())


if __name__ == "__main__":
    unittest.main()
