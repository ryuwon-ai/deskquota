"""Behavior regressions for the installed-Pi development probe."""

from __future__ import annotations

import importlib.util
import json
import os
import subprocess
import sys
import tempfile
import tomllib
import unittest
from pathlib import Path
from unittest import mock


PRODUCT_ROOT = Path(__file__).resolve().parents[1]
PROBE_PATH = PRODUCT_ROOT / "scripts" / "probe_pi.py"
SPEC = importlib.util.spec_from_file_location("task4_probe_pi", PROBE_PATH)
assert SPEC is not None and SPEC.loader is not None
probe = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(probe)


class PiProbeRegressionTests(unittest.TestCase):
    def test_transport_fixture_explicitly_uses_unlimited_rpm_and_unknown_tpm(self) -> None:
        config = tomllib.loads(probe.gateway_config(12345, 12346))
        self.assertEqual(config["quota"], {"rpm": {"kind": "unlimited"}, "tpm": {"kind": "unknown"}})

    def test_normal_child_output_is_collected_and_reaped(self) -> None:
        real_popen = subprocess.Popen
        captured: dict[str, subprocess.Popen[bytes]] = {}

        def start_real_child(*args: object, **kwargs: object) -> subprocess.Popen[bytes]:
            process = real_popen(*args, **kwargs)
            captured["process"] = process
            return process

        with tempfile.TemporaryDirectory(prefix="task4-pi-normal-") as temporary:
            with mock.patch.object(probe.subprocess, "Popen", side_effect=start_real_child):
                result = probe.run_pi(
                    [sys.executable, "-c", "print('synthetic-ok')"],
                    Path(temporary),
                    dict(os.environ),
                )
        exit_code, stdout, stderr, timed_out, cleanup = result
        self.assertEqual(exit_code, 0)
        self.assertEqual(stdout, b"synthetic-ok\n")
        self.assertEqual(stderr, b"")
        self.assertFalse(timed_out)
        self.assertEqual(cleanup, "normal_exit")
        self.assertIsNotNone(captured["process"].poll())
        self.assertTrue(captured["process"].stdout.closed)
        self.assertTrue(captured["process"].stderr.closed)

    def test_timeout_terminates_and_reaps_owned_pi_process(self) -> None:
        real_popen = subprocess.Popen
        captured: dict[str, subprocess.Popen[bytes]] = {}

        def start_real_child(*args: object, **kwargs: object) -> subprocess.Popen[bytes]:
            process = real_popen(*args, **kwargs)
            captured["process"] = process
            return process

        with tempfile.TemporaryDirectory(prefix="task4-pi-timeout-") as temporary:
            child = [sys.executable, "-c", "import time; time.sleep(30)"]
            with (
                mock.patch.object(probe.subprocess, "Popen", side_effect=start_real_child),
                mock.patch.object(probe, "PI_TIMEOUT_SECONDS", 0.02),
            ):
                exit_code, _stdout, _stderr, timed_out, cleanup = probe.run_pi(
                    child, Path(temporary), dict(os.environ)
                )
        self.assertNotEqual(exit_code, 0)
        self.assertTrue(timed_out)
        self.assertIn(cleanup, {"terminated", "killed"})
        self.assertIsNotNone(captured["process"].poll())
        self.assertTrue(captured["process"].stdout.closed)
        self.assertTrue(captured["process"].stderr.closed)

    def test_keyboard_interrupt_reaps_owned_pi_process(self) -> None:
        real_popen = subprocess.Popen
        captured: dict[str, subprocess.Popen[bytes]] = {}

        class InterruptedCommunicate:
            def __init__(self, process: subprocess.Popen[bytes]):
                self.process = process

            def communicate(self, *_args: object, **_kwargs: object) -> tuple[bytes, bytes]:
                raise KeyboardInterrupt

            def __getattr__(self, name: str) -> object:
                return getattr(self.process, name)

        def start_real_child(*args: object, **kwargs: object) -> InterruptedCommunicate:
            process = real_popen(*args, **kwargs)
            captured["process"] = process
            return InterruptedCommunicate(process)

        with tempfile.TemporaryDirectory(prefix="task4-pi-interrupt-") as temporary:
            child = [sys.executable, "-c", "import time; time.sleep(30)"]
            try:
                with mock.patch.object(probe.subprocess, "Popen", side_effect=start_real_child):
                    with self.assertRaises(KeyboardInterrupt):
                        probe.run_pi(child, Path(temporary), dict(os.environ))
                self.assertIsNotNone(captured["process"].poll(), "owned Pi child remained alive after interruption")
                self.assertTrue(captured["process"].stdout.closed)
                self.assertTrue(captured["process"].stderr.closed)
            finally:
                process = captured.get("process")
                if process is not None and process.poll() is None:
                    process.terminate()
                    try:
                        process.wait(timeout=2)
                    except subprocess.TimeoutExpired:
                        process.kill()
                        process.wait(timeout=2)

    def test_absent_reference_clone_keeps_version_comparison_unknown(self) -> None:
        real_run = subprocess.run

        def reject_git(*args: object, **kwargs: object) -> subprocess.CompletedProcess[object]:
            command = args[0] if args else kwargs.get("args")
            if isinstance(command, (list, tuple)) and command and command[0] == "git":
                raise AssertionError("unexpected Git lookup")
            return real_run(*args, **kwargs)

        with tempfile.TemporaryDirectory(prefix="task4-no-clone-") as temporary:
            root = Path(temporary)
            package_dir = root / "installed-pi"
            entry = package_dir / "dist" / "cli.js"
            entry.parent.mkdir(parents=True)
            entry.write_text("synthetic entry", encoding="utf-8")
            package = {"name": "@earendil-works/pi-coding-agent", "version": "0.84.2"}
            (package_dir / "package.json").write_text(json.dumps(package), encoding="utf-8")
            binary = root / "llmgw"
            binary.write_bytes(b"synthetic binary")
            with (
                mock.patch.object(probe, "ROOT", root / "product"),
                mock.patch.object(probe.subprocess, "run", side_effect=reject_git),
            ):
                metadata = probe.build_metadata(entry, entry, package, binary)
            self.assertIsNone(metadata["reference_clone"]["version"])
            self.assertIsNone(metadata["reference_clone"]["commit"])
            self.assertIsNone(metadata["reference_clone"]["differs_from_installed"])


if __name__ == "__main__":
    unittest.main()
