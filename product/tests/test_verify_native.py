import importlib.util
import os
import pathlib
import shutil
import subprocess
import tempfile
import types
import unittest
from unittest import mock


SCRIPT = pathlib.Path(__file__).parents[1] / "scripts" / "verify_native.py"


class VerifyNativeUnitTests(unittest.TestCase):
    def load(self):
        spec = importlib.util.spec_from_file_location("verify_native", SCRIPT)
        module = importlib.util.module_from_spec(spec)
        spec.loader.exec_module(module)
        return module

    def test_child_environment_is_allowlisted_and_excludes_credentials(self):
        module = self.load()
        env = module.safe_child_env(pathlib.Path("/tmp/owned-home"), {"PATH": "/usr/bin", "SECRET": "fixture"})
        self.assertEqual(env["HOME"], str(pathlib.Path("/tmp/owned-home").resolve()))
        self.assertNotIn("PATH", env)
        self.assertNotIn("SECRET", env)

    def test_terminal_environment_preserves_explicit_auth_and_windows_runtime(self):
        module = self.load()
        env = module.terminal_child_env(
            pathlib.Path("/tmp/owned-home"),
            {"REVIEW_SYNTHETIC_AUTH": "fixture", "SystemRoot": r"C:\Windows"},
        )
        self.assertEqual(env["REVIEW_SYNTHETIC_AUTH"], "fixture")
        self.assertEqual(env["SystemRoot"], r"C:\Windows")

    def test_preview_hash_parser_requires_exact_hash_line(self):
        module = self.load()
        self.assertEqual(module.preview_hash("label: x\npreview hash: abc123\n"), "abc123")
        with self.assertRaises(ValueError):
            module.preview_hash("preview unavailable")

    def test_login_auth_failure_matches_sanitized_runtime_error(self):
        module = self.load()
        self.assertTrue(module.missing_login_auth("error: configured upstream environment credential is missing\n"))
        self.assertFalse(module.missing_login_auth("error: port in use\n"))

    def test_login_timestamp_without_human_attestation_is_not_login_proof(self):
        module = self.load()
        observed = {"state": "running", "autostart": {"registration": "registered"}}
        self.assertFalse(module.login_on_verified(observed, human_attested=False))
        self.assertTrue(module.login_on_verified(observed, human_attested=True))
        self.assertFalse(module.login_on_verified({"state": "stopped", "autostart": {"registration": "registered"}}, human_attested=True))

    def test_templates_acceptance_rejects_a_recorded_false_check(self):
        module = self.load()
        valid = {
            "actual_user_registration_changed": False,
            "host_file_fixture_executed": True,
            "template_contract": {"passed": True},
            "runtime_separation": {
                "worker_processes_after_autostart_on": 0,
                "worker_preserved": True,
                "manual_cleanup_state": "stopped",
            },
            "login_environment": {
                "registration": "registered",
                "unobserved_login_auth_availability": "unknown",
                "observed_empty_login_auth_availability": "configured_but_unavailable",
                "empty_login_environment_run_failed_for_missing_auth": True,
                "secret_copied_into_registration": False,
                "auth_env_name_copied_into_registration": False,
            },
        }
        self.assertEqual(module.templates_acceptance_errors(valid), [])
        valid["runtime_separation"]["worker_preserved"] = False
        self.assertIn("worker_preserved", module.templates_acceptance_errors(valid))

    def test_cargo_template_tests_honors_caller_target(self):
        module = self.load()
        target = pathlib.Path("/tmp/native task5 spec target")
        completed = subprocess.CompletedProcess([], 0, "ok", "")
        with mock.patch.object(module.subprocess, "run", return_value=completed) as runner:
            module.cargo_template_tests(target)
        self.assertEqual(runner.call_args.kwargs["env"]["CARGO_TARGET_DIR"], str(target.resolve()))

    def test_templates_only_never_dispatches_linux_or_windows_registration(self):
        module = self.load()
        binary = pathlib.Path("/tmp/llmgw")
        contract = {"passed": True, "command": "fixture", "stdout": "", "stderr": ""}
        for platform in ("linux", "win32"):
            with self.subTest(platform=platform), mock.patch.object(
                module.sys, "platform", platform
            ), mock.patch.object(module, "cargo_template_tests", return_value=contract), mock.patch.object(
                module, "apply_autostart"
            ) as apply:
                record = module.templates_only(binary, pathlib.Path("/tmp/target"))
                apply.assert_not_called()
                self.assertFalse(record["actual_user_registration_changed"])

    def test_failed_manual_cycle_returns_nonzero(self):
        module = self.load()
        args = types.SimpleNamespace(
            stage="manual-cycle", execute=True, confirm_temporary_os_account=True
        )
        stopped = {
            "state": "stopped",
            "autostart": {"registration": "disabled", "login_auth_availability": "unknown"},
        }
        failed = subprocess.CompletedProcess([], 1, "", "missing synthetic auth")
        with mock.patch.object(module, "status", return_value=stopped), mock.patch.object(
            module, "cli", return_value=failed
        ), mock.patch.object(pathlib.Path, "home", return_value=pathlib.Path("/tmp/driver-home")):
            record, code = module.manual_stage(
                args, pathlib.Path("/tmp/llmgw"), pathlib.Path("/tmp/config.toml")
            )
        self.assertEqual(record["on_exit"], 1)
        self.assertNotEqual(code, 0)

    def test_failed_attested_login_observations_return_nonzero(self):
        module = self.load()
        cases = (
            ("observe-login-on", "stopped", "registered", False, 1),
            ("observe-login-on", "running", "registered", True, 0),
            ("observe-login-off", "running", "disabled", False, 1),
            ("observe-login-off", "stopped", "disabled", True, 0),
        )
        for stage, state, registration, verified, expected_code in cases:
            with self.subTest(stage=stage, state=state):
                args = types.SimpleNamespace(
                    stage=stage,
                    execute=True,
                    confirm_temporary_os_account=True,
                    confirm_human_login_observed=True,
                    human_login_at="2026-09-13T15:00:00+09:00",
                )
                observed = {
                    "state": state,
                    "autostart": {"registration": registration},
                }
                with mock.patch.object(module, "status", return_value=observed), mock.patch.object(
                    module, "terminal_child_env", return_value={}
                ), mock.patch.object(
                    pathlib.Path,
                    "home",
                    return_value=pathlib.Path("/tmp/owned-driver-home"),
                ):
                    record, code = module.manual_stage(
                        args,
                        pathlib.Path("/tmp/llmgw"),
                        pathlib.Path("/tmp/config.toml"),
                    )
                self.assertEqual(record["actual_login_verified"], verified)
                self.assertEqual(code, expected_code)

    def test_post_start_failure_attempts_authenticated_cleanup_before_deleting_home(self):
        module = self.load()
        homes = []
        events = []
        status_calls = 0
        real_mkdtemp = tempfile.mkdtemp

        def owned_mkdtemp(*_args, **_kwargs):
            home = pathlib.Path(real_mkdtemp(prefix="llmgw-quality-cleanup-"))
            homes.append(home)
            return str(home)

        def fake_status(_binary, _config, home):
            nonlocal status_calls
            status_calls += 1
            if status_calls == 3:
                raise RuntimeError("injected status failure after manual on")
            return {
                "state": "stopped",
                "autostart": {"target": str(home / "fixture.plist")},
            }

        def fake_autostart(_binary, _config, home, action, *_args, **_kwargs):
            events.append(f"autostart {action}")
            (home / "fixture.plist").write_text("fixture", encoding="utf-8")
            return "preview hash: fixture", subprocess.CompletedProcess([], 0, "", "")

        def fake_cli(_binary, _config, _home, *arguments, **_kwargs):
            events.append(f"cli {' '.join(arguments)}")
            return subprocess.CompletedProcess([], 0, "", "")

        with mock.patch.object(module.sys, "platform", "darwin"), mock.patch.object(
            module, "cargo_template_tests", return_value={"passed": True}
        ), mock.patch.object(module.tempfile, "mkdtemp", side_effect=owned_mkdtemp), mock.patch.object(
            module, "free_port", return_value=12345
        ), mock.patch.object(module, "status", side_effect=fake_status), mock.patch.object(
            module, "apply_autostart", side_effect=fake_autostart
        ), mock.patch.object(module, "cli", side_effect=fake_cli), mock.patch.object(
            module.shutil, "which", return_value=None
        ), mock.patch.object(
            module.subprocess,
            "run",
            return_value=subprocess.CompletedProcess([], 0, "fixture plutil OK", ""),
        ):
            with self.assertRaisesRegex(RuntimeError, "injected status failure"):
                module.templates_only(pathlib.Path("/tmp/llmgw"), pathlib.Path("/tmp/target"))

        self.assertIn("cli off", events)
        self.assertTrue(homes)
        self.assertTrue(all(not home.exists() for home in homes))

    def test_failed_authenticated_cleanup_retains_recovery_home(self):
        module = self.load()
        homes = []
        status_calls = 0
        real_mkdtemp = tempfile.mkdtemp

        def owned_mkdtemp(*_args, **_kwargs):
            home = pathlib.Path(real_mkdtemp(prefix="llmgw-quality-recovery-"))
            homes.append(home)
            return str(home)

        def fake_status(_binary, _config, home):
            nonlocal status_calls
            status_calls += 1
            if status_calls == 3:
                raise RuntimeError("injected original failure")
            return {
                "state": "stopped",
                "autostart": {"target": str(home / "fixture.plist")},
            }

        def fake_autostart(_binary, _config, home, _action, *_args, **_kwargs):
            (home / "fixture.plist").write_text("fixture", encoding="utf-8")
            return "preview hash: fixture", subprocess.CompletedProcess([], 0, "", "")

        def fake_cli(_binary, _config, _home, *arguments, **_kwargs):
            return subprocess.CompletedProcess(
                [], 1 if arguments == ("off",) else 0, "", "injected cleanup failure"
            )

        try:
            with mock.patch.object(module.sys, "platform", "darwin"), mock.patch.object(
                module, "cargo_template_tests", return_value={"passed": True}
            ), mock.patch.object(module.tempfile, "mkdtemp", side_effect=owned_mkdtemp), mock.patch.object(
                module, "free_port", return_value=12345
            ), mock.patch.object(module, "status", side_effect=fake_status), mock.patch.object(
                module, "apply_autostart", side_effect=fake_autostart
            ), mock.patch.object(module, "cli", side_effect=fake_cli), mock.patch.object(
                module.shutil, "which", return_value=None
            ), mock.patch.object(
                module.subprocess,
                "run",
                return_value=subprocess.CompletedProcess([], 0, "fixture plutil OK", ""),
            ):
                with self.assertRaisesRegex(
                    RuntimeError,
                    "original failure.*authenticated cleanup failed.*recovery files retained",
                ) as raised:
                    module.templates_only(
                        pathlib.Path("/tmp/llmgw"), pathlib.Path("/tmp/target")
                    )
            self.assertTrue(homes[0].is_dir())
            self.assertIn(str(homes[0]), str(raised.exception))
        finally:
            for home in homes:
                shutil.rmtree(home, ignore_errors=True)

    def test_login_observation_summary_omits_authenticated_identity(self):
        module = self.load()
        observed = {
            "state": "running",
            "identity": {"nonce": "synthetic-secret-must-not-be-recorded"},
            "autostart": {
                "registration": "registered",
                "manager": "enabled",
                "login_auth_availability": "unknown",
            },
        }
        summary = module.login_observation_summary(observed)
        self.assertEqual(summary["state"], "running")
        self.assertNotIn("identity", summary)
        self.assertNotIn("synthetic-secret-must-not-be-recorded", repr(summary))


if __name__ == "__main__":
    unittest.main()
