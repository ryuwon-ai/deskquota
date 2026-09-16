#!/usr/bin/env python3
import contextlib
import functools
import hashlib
import importlib.util
import http.server
import os
from pathlib import Path
import shutil
import subprocess
import sys
import tarfile
import tempfile
import threading
import unittest
from unittest.mock import patch


PRODUCT = Path(__file__).resolve().parents[1]
INSTALLER = PRODUCT / "packaging" / "install.sh"


class QuietHandler(http.server.SimpleHTTPRequestHandler):
    def log_message(self, _format, *_args):
        pass


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


class PosixInstallerTests(unittest.TestCase):
    def setUp(self):
        if os.name != "posix":
            self.skipTest("POSIX installer requires a POSIX host")
        self.temp = tempfile.TemporaryDirectory(prefix="llmgw-installer-test-")
        self.root = Path(self.temp.name)
        self.release = self.root / "release"
        self.release.mkdir()
        self.home = self.root / "home"
        self.home.mkdir()
        self.install_dir = self.home / ".local" / "bin"

    def tearDown(self):
        self.temp.cleanup()

    def make_release(self, binary=b"#!/bin/sh\necho llmgw-fixture\n", members=None):
        source = self.root / "source"
        source.mkdir(exist_ok=True)
        member_names = members or (
            "llmgw",
            "README.md",
            "docs/installation.md",
            "docs/runtime-contract.md",
            "docs/client-compatibility.md",
        )
        archive = self.release / "llmgw-test.tar.gz"
        with tarfile.open(archive, "w:gz") as output:
            for index, name in enumerate(member_names):
                path = source / f"member-{index}"
                path.write_bytes(binary if name == "llmgw" else f"fixture {name}\n".encode())
                output.add(path, arcname=name, recursive=False)
        digest = hashlib.sha256(archive.read_bytes()).hexdigest()
        manifest = self.release / "llmgw-test.tar.gz.sha256"
        manifest.write_text(f"{digest}  {archive.name}\n", encoding="ascii")
        return archive, manifest, binary

    def run_installer(self, archive, manifest, *extra):
        env = {
            "HOME": str(self.home),
            "PATH": "/usr/bin:/bin:/usr/sbin:/sbin",
            "TMPDIR": str(self.root),
            "LC_ALL": "C",
        }
        return subprocess.run(
            [
                "/bin/sh",
                str(INSTALLER),
                "--archive",
                str(archive),
                "--checksum-manifest",
                str(manifest),
                "--install-dir",
                str(self.install_dir),
                *extra,
            ],
            env=env,
            capture_output=True,
            text=True,
            timeout=15,
        )

    def test_local_archive_installs_only_verified_binary_and_reinstalls(self):
        archive, manifest, binary = self.make_release()
        first = self.run_installer(archive, manifest, "--path-action", "preview")
        self.assertEqual(first.returncode, 0, first.stderr)
        installed = self.install_dir / "llmgw"
        self.assertEqual(installed.read_bytes(), binary)
        self.assertTrue(os.access(installed, os.X_OK))
        self.assertIn(str(self.install_dir), first.stdout)
        second = self.run_installer(archive, manifest, "--path-action", "none")
        self.assertEqual(second.returncode, 0, second.stderr)
        self.assertEqual(installed.read_bytes(), binary)

    def test_fake_release_server_downloads_archive_and_manifest(self):
        archive, manifest, binary = self.make_release()
        digest, name = manifest.read_text(encoding="ascii").split()
        manifest.write_text(f"{digest.upper()}  {name}\n", encoding="ascii")
        with release_server(self.release) as base:
            completed = self.run_installer(
                f"{base}/{archive.name}", f"{base}/{manifest.name}", "--path-action", "none"
            )
        self.assertEqual(completed.returncode, 0, completed.stderr)
        self.assertEqual((self.install_dir / "llmgw").read_bytes(), binary)

    def test_wrong_hash_and_missing_artifact_preserve_existing_executable(self):
        self.install_dir.mkdir(parents=True)
        installed = self.install_dir / "llmgw"
        installed.write_bytes(b"#!/bin/sh\necho existing\n")
        installed.chmod(0o755)
        before = installed.read_bytes()
        archive, manifest, _ = self.make_release()
        manifest.write_text(f"{'0' * 64}  {archive.name}\n", encoding="ascii")
        wrong = self.run_installer(archive, manifest, "--path-action", "none")
        self.assertNotEqual(wrong.returncode, 0)
        self.assertEqual(installed.read_bytes(), before)
        self.assertTrue(os.access(installed, os.X_OK))
        missing = self.run_installer(
            self.release / "missing.tar.gz", manifest, "--path-action", "none"
        )
        self.assertNotEqual(missing.returncode, 0)
        self.assertEqual(installed.read_bytes(), before)
        self.assertTrue(os.access(installed, os.X_OK))

    def test_unwritable_destination_preserves_existing_executable(self):
        archive, manifest, _ = self.make_release()
        blocker = self.home / "not-a-directory"
        blocker.write_text("fixture", encoding="utf-8")
        self.install_dir = blocker / "bin"
        completed = self.run_installer(archive, manifest, "--path-action", "none")
        self.assertNotEqual(completed.returncode, 0)
        self.assertEqual(blocker.read_text(encoding="utf-8"), "fixture")

    def test_destination_permission_failure_preserves_existing_executable(self):
        archive, manifest, _ = self.make_release()
        self.install_dir.mkdir(parents=True)
        installed = self.install_dir / "llmgw"
        installed.write_bytes(b"#!/bin/sh\necho existing\n")
        installed.chmod(0o755)
        before = installed.read_bytes()
        self.install_dir.chmod(0o555)
        try:
            completed = self.run_installer(archive, manifest, "--path-action", "none")
        finally:
            self.install_dir.chmod(0o755)
        self.assertNotEqual(completed.returncode, 0)
        self.assertEqual(installed.read_bytes(), before)
        self.assertTrue(os.access(installed, os.X_OK))

    def test_path_preview_does_not_edit_profile_and_new_child_path_resolves_binary(self):
        archive, manifest, _ = self.make_release()
        profile = self.home / ".profile"
        profile.write_text("# existing\n", encoding="utf-8")
        profile.chmod(0o644)
        preview = self.run_installer(archive, manifest, "--path-action", "preview")
        self.assertEqual(preview.returncode, 0, preview.stderr)
        self.assertEqual(profile.read_text(encoding="utf-8"), "# existing\n")
        absent = subprocess.run(
            ["/bin/sh", "-c", "command -v llmgw"],
            env={"HOME": str(self.home), "PATH": "/usr/bin:/bin"},
            capture_output=True,
            text=True,
        )
        self.assertNotEqual(absent.returncode, 0)
        unsupported = self.run_installer(archive, manifest, "--path-action", "update")
        self.assertNotEqual(unsupported.returncode, 0)
        self.assertEqual(profile.read_text(encoding="utf-8"), "# existing\n")
        self.assertEqual(profile.stat().st_mode & 0o777, 0o644)
        child = subprocess.run(
            ["/bin/sh", "-c", "command -v llmgw; llmgw"],
            env={"HOME": str(self.home), "PATH": f"{self.install_dir}:/usr/bin:/bin"},
            capture_output=True,
            text=True,
        )
        self.assertEqual(child.returncode, 0, child.stderr)
        self.assertIn(str(self.install_dir / "llmgw"), child.stdout)
        self.assertIn("llmgw-fixture", child.stdout)

    def test_existing_directory_at_binary_path_is_rejected(self):
        archive, manifest, _ = self.make_release()
        target = self.install_dir / "llmgw"
        target.mkdir(parents=True)
        marker = target / "keep"
        marker.write_text("existing", encoding="utf-8")
        completed = self.run_installer(archive, manifest, "--path-action", "none")
        self.assertNotEqual(completed.returncode, 0)
        self.assertTrue(target.is_dir())
        self.assertEqual(marker.read_text(encoding="utf-8"), "existing")
        self.assertFalse((target / ".llmgw-install").exists())

    def test_path_preview_never_executes_custom_directory_text(self):
        archive, manifest, _ = self.make_release()
        marker = self.root / "must-not-exist"
        self.install_dir = self.home / f'bin-quote\'-$(touch {marker})-`false`-$HOME-"quoted"'
        completed = self.run_installer(archive, manifest, "--path-action", "preview")
        self.assertEqual(completed.returncode, 0, completed.stderr)
        self.assertFalse(marker.exists())
        self.assertIn(str(self.install_dir), completed.stdout)
        path_lines = [line.strip() for line in completed.stdout.splitlines() if line.strip().startswith("export PATH=")]
        self.assertEqual(len(path_lines), 1)
        child = subprocess.run(
            ["/bin/sh", "-c", path_lines[0] + "\ncommand -v llmgw\nllmgw"],
            env={"HOME": str(self.home), "PATH": "/usr/bin:/bin"},
            capture_output=True,
            text=True,
            timeout=5,
        )
        self.assertEqual(child.returncode, 0, child.stderr)
        self.assertFalse(marker.exists())
        self.assertIn(str(self.install_dir / "llmgw"), child.stdout)

    def test_rejects_traversal_links_duplicates_and_unexpected_members(self):
        cases = (
            ("traversal", ("llmgw", "../escape")),
            ("duplicate", ("llmgw", "llmgw")),
            ("unexpected", ("llmgw", "secret.env")),
        )
        for label, members in cases:
            with self.subTest(label=label):
                archive, manifest, _ = self.make_release(members=members)
                completed = self.run_installer(archive, manifest, "--path-action", "none")
                self.assertNotEqual(completed.returncode, 0)
                self.assertFalse((self.install_dir / "llmgw").exists())
                archive.unlink()
                manifest.unlink()
        source = self.root / "link-source"
        source.write_text("fixture", encoding="utf-8")
        archive = self.release / "llmgw-link.tar.gz"
        with tarfile.open(archive, "w:gz") as output:
            info = tarfile.TarInfo("llmgw")
            info.type = tarfile.SYMTYPE
            info.linkname = str(source)
            output.addfile(info)
        digest = hashlib.sha256(archive.read_bytes()).hexdigest()
        manifest = self.release / "llmgw-link.tar.gz.sha256"
        manifest.write_text(f"{digest}  {archive.name}\n", encoding="ascii")
        linked = self.run_installer(archive, manifest, "--path-action", "none")
        self.assertNotEqual(linked.returncode, 0)
        self.assertFalse((self.install_dir / "llmgw").exists())


class PowerShellInstallerStaticTests(unittest.TestCase):
    def test_windows_installer_has_bounded_zip_and_never_mutates_user_path(self):
        text = (PRODUCT / "packaging" / "install.ps1").read_text(encoding="utf-8")
        self.assertIn("Get-FileHash", text)
        self.assertIn("ZipFile", text)
        self.assertIn("IsWellFormedUriString", text)
        self.assertIn("duplicate_archive_entry", text)
        self.assertIn("unexpected_archive_entry", text)
        self.assertIn("ReparsePoint", text)
        self.assertIn('$backup = Join-Path $InstallDir', text)
        self.assertIn("replace_failed_recovery_unconfirmed", text)
        self.assertIn("$preserveRecovery", text)
        self.assertNotIn("Remove-Item -LiteralPath $target -Force", text)
        self.assertNotIn('$backup = Join-Path $work', text)
        self.assertNotIn("SetEnvironmentVariable", text)
        self.assertNotIn("ExecutionPolicy", text)
        self.assertNotIn("Add-MpPreference", text)

    def test_windows_replace_failure_only_preserves_and_reports_recovery_paths(self):
        text = (PRODUCT / "packaging" / "install.ps1").read_text(encoding="utf-8")
        start = text.index("            $replaceError = $_.Exception.Message")
        end = text.index("    } else {", start)
        recovery = text[start:end]
        self.assertIn("replace_failed_recovery_unconfirmed: $replaceError", recovery)
        self.assertIn("target=$target; backup=$backup; candidate=$candidate", recovery)
        self.assertNotIn("[System.IO.File]::Copy", recovery)
        self.assertNotIn("[System.IO.File]::Move", recovery)
        self.assertNotIn("Remove-Item", recovery)
        self.assertNotIn("$preserveRecovery = $false", recovery)


class AcceptanceDiagnosticTests(unittest.TestCase):
    def test_resource_probe_uses_system_executable_in_host_environment(self):
        spec = importlib.util.spec_from_file_location("verify_windows", PRODUCT / "scripts/verify_windows.py")
        module = importlib.util.module_from_spec(spec)
        spec.loader.exec_module(module)
        with patch.dict(os.environ, {"SystemRoot": "C:/Windows"}), patch.object(module, "run", return_value=subprocess.CompletedProcess([], 0, stdout="{}")) as child:
            self.assertEqual(module.resources(1), {})
            self.assertEqual(child.call_args.args[0][0], Path("C:/Windows/System32/WindowsPowerShell/v1.0/powershell.exe"))
            self.assertNotIn("env", child.call_args.kwargs)

    def test_powershell_child_rebuilds_module_path_without_mutating_parent(self):
        spec = importlib.util.spec_from_file_location("verify_windows", PRODUCT / "scripts/verify_windows.py")
        module = importlib.util.module_from_spec(spec)
        spec.loader.exec_module(module)
        parent = {"PSModulePath": "incompatible-ps7-modules", "PATH": "preserve-path"}
        with patch.object(module.subprocess, "run", return_value=subprocess.CompletedProcess([], 0)) as child:
            module.run(["powershell.exe", "-NoProfile"], env=parent)
            self.assertEqual(child.call_args.kwargs["env"], {"PATH": "preserve-path"})
            module.run(["other.exe"], env=parent)
            self.assertEqual(child.call_args.kwargs["env"], parent)
        self.assertIn("PSModulePath", parent)

    def test_failed_probe_command_preserves_failure_reason(self):
        spec = importlib.util.spec_from_file_location("verify_windows", PRODUCT / "scripts/verify_windows.py")
        module = importlib.util.module_from_spec(spec)
        spec.loader.exec_module(module)
        with self.assertRaisesRegex(RuntimeError, "exited 3: synthetic_failure_reason"):
            module.run([sys.executable, "-c", "import sys; sys.stderr.write('synthetic_failure_reason'); sys.exit(3)"])


if __name__ == "__main__":
    unittest.main()
