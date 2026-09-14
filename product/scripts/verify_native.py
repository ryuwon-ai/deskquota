#!/usr/bin/env python3
"""Bounded native autostart template checks and explicit manual acceptance stages."""

from __future__ import annotations

import argparse
import datetime as dt
import hashlib
import json
import os
import pathlib
import shutil
import socket
import subprocess
import sys
import tempfile


ROOT = pathlib.Path(__file__).resolve().parents[1]
HELD_TARGET = ROOT / "target" / "native-task5"


def safe_child_env(home: pathlib.Path, _source: dict[str, str] | None = None) -> dict[str, str]:
    """Return the login-fixture allowlist; shell credentials and PATH never cross it."""
    home = home.resolve()
    temporary = home / "tmp"
    temporary.mkdir(parents=True, exist_ok=True)
    return {
        "HOME": str(home),
        "USERPROFILE": str(home),
        "TMPDIR": str(temporary),
        "LC_ALL": "C",
    }


def terminal_child_env(
    home: pathlib.Path, source: dict[str, str] | None = None
) -> dict[str, str]:
    """Preserve the explicitly executing terminal environment without recording it."""
    home = home.resolve()
    temporary = home / "tmp"
    temporary.mkdir(parents=True, exist_ok=True)
    env = dict(os.environ if source is None else source)
    env.update({"HOME": str(home), "USERPROFILE": str(home), "TMPDIR": str(temporary)})
    return env


def preview_hash(text: str) -> str:
    matches = [line.removeprefix("preview hash: ") for line in text.splitlines() if line.startswith("preview hash: ")]
    if len(matches) != 1 or not matches[0]:
        raise ValueError("exactly one autostart preview hash was required")
    return matches[0]


def missing_login_auth(stderr: str) -> bool:
    return "configured upstream environment credential is missing" in stderr


def login_on_verified(observed: dict, human_attested: bool) -> bool:
    return bool(
        human_attested
        and observed.get("state") == "running"
        and observed.get("autostart", {}).get("registration") == "registered"
    )


def login_off_verified(observed: dict, human_attested: bool) -> bool:
    return bool(
        human_attested
        and observed.get("state") == "stopped"
        and observed.get("autostart", {}).get("registration") == "disabled"
    )


def login_observation_summary(observed: dict) -> dict:
    """Return login evidence fields without authenticated runtime identity."""
    autostart = observed.get("autostart", {})
    return {
        "state": observed.get("state"),
        "autostart": {
            key: autostart.get(key)
            for key in (
                "registration",
                "manager",
                "manager_scope",
                "manager_ownership",
                "reason",
                "label",
                "target",
                "executable",
                "config",
                "login_auth_availability",
            )
            if key in autostart
        },
    }


def templates_acceptance_errors(record: dict) -> list[str]:
    expected = [
        ("template_contract", record.get("template_contract", {}).get("passed") is True),
        ("actual_user_registration_changed", record.get("actual_user_registration_changed") is False),
    ]
    if not record.get("host_file_fixture_executed"):
        return [name for name, accepted in expected if not accepted]
    expected.extend([
        (
            "worker_processes_after_autostart_on",
            record.get("runtime_separation", {}).get("worker_processes_after_autostart_on") == 0,
        ),
        ("worker_preserved", record.get("runtime_separation", {}).get("worker_preserved") is True),
        (
            "manual_cleanup_state",
            record.get("runtime_separation", {}).get("manual_cleanup_state") == "stopped",
        ),
        ("auth_registration", record.get("login_environment", {}).get("registration") == "registered"),
        (
            "unobserved_login_auth_availability",
            record.get("login_environment", {}).get("unobserved_login_auth_availability")
            == "unknown",
        ),
        (
            "observed_empty_login_auth_availability",
            record.get("login_environment", {}).get("observed_empty_login_auth_availability")
            == "configured_but_unavailable",
        ),
        (
            "empty_login_environment_run_failed_for_missing_auth",
            record.get("login_environment", {}).get(
                "empty_login_environment_run_failed_for_missing_auth"
            )
            is True,
        ),
        (
            "secret_copied_into_registration",
            record.get("login_environment", {}).get("secret_copied_into_registration") is False,
        ),
        (
            "auth_env_name_copied_into_registration",
            record.get("login_environment", {}).get("auth_env_name_copied_into_registration")
            is False,
        ),
    ])
    return [name for name, accepted in expected if not accepted]


def run(command: list[str], env: dict[str, str], timeout: float = 20.0) -> subprocess.CompletedProcess[str]:
    return subprocess.run(
        command,
        stdin=subprocess.DEVNULL,
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
        text=True,
        env=env,
        timeout=timeout,
        check=False,
    )


def cli(
    binary: pathlib.Path,
    config: pathlib.Path,
    home: pathlib.Path,
    *arguments: str,
    environment: dict[str, str] | None = None,
) -> subprocess.CompletedProcess[str]:
    return run(
        [str(binary), *arguments, "--config", str(config)],
        safe_child_env(home) if environment is None else environment,
    )


def apply_autostart(
    binary: pathlib.Path,
    config: pathlib.Path,
    home: pathlib.Path,
    action: str,
    environment: dict[str, str] | None = None,
) -> tuple[str, subprocess.CompletedProcess[str]]:
    preview = cli(binary, config, home, "autostart", action, environment=environment)
    if preview.returncode != 2:
        raise RuntimeError(f"expected preview-only exit 2, got {preview.returncode}: {preview.stderr}")
    digest = preview_hash(preview.stdout)
    applied = cli(
        binary,
        config,
        home,
        "autostart",
        action,
        "--apply-hash",
        digest,
        environment=environment,
    )
    if applied.returncode != 0:
        raise RuntimeError(f"autostart {action} failed: {applied.stderr}")
    return preview.stdout, applied


def status(
    binary: pathlib.Path,
    config: pathlib.Path,
    home: pathlib.Path,
    environment: dict[str, str] | None = None,
) -> dict:
    result = cli(binary, config, home, "status", "--json", environment=environment)
    if result.returncode != 0:
        raise RuntimeError(f"status failed: {result.stderr}")
    return json.loads(result.stdout)


def free_port() -> int:
    with socket.socket() as listener:
        listener.bind(("127.0.0.1", 0))
        return listener.getsockname()[1]


def config_text(port: int, auth: str) -> str:
    return f'''listen = "127.0.0.1:{port}"
concurrency = 1
cancel_policy = "drain"
accounting = "reserved"

[upstream]
api_base = "http://127.0.0.1:9/v1"

[upstream.auth]
{auth}

[quota.rpm]
kind = "unlimited"

[quota.tpm]
kind = "unlimited"

[[models]]
id = "fixture-model"

[[roots]]
id = "fixture"
endpoints = ["responses"]
models = ["fixture-model"]
'''


def cargo_template_tests(target_dir: pathlib.Path) -> dict:
    target_dir = target_dir.resolve()
    if target_dir == HELD_TARGET.resolve():
        raise ValueError("the frozen native-task5 target cannot be reused")
    env = dict(os.environ)
    env["CARGO_TARGET_DIR"] = str(target_dir)
    result = subprocess.run(
        ["cargo", "test", "--locked", "--test", "autostart_templates"],
        cwd=ROOT,
        env=env,
        stdin=subprocess.DEVNULL,
        stdout=subprocess.PIPE,
        stderr=subprocess.STDOUT,
        text=True,
        timeout=180,
        check=False,
    )
    return {
        "command": "cargo test --locked --test autostart_templates",
        "exit_code": result.returncode,
        "passed": result.returncode == 0,
        "output_tail": result.stdout[-4000:],
    }


def templates_only(binary: pathlib.Path, cargo_target_dir: pathlib.Path) -> dict:
    contract = cargo_template_tests(cargo_target_dir)
    if not contract["passed"]:
        raise RuntimeError("autostart template contract tests failed")
    if sys.platform != "darwin":
        record = {
            "schema_version": 2,
            "mode": "templates_only",
            "passed": True,
            "actual_user_registration_changed": False,
            "actual_login_verified": False,
            "host_file_fixture_executed": False,
            "binary": str(binary),
            "binary_sha256": hashlib.sha256(binary.read_bytes()).hexdigest()
            if binary.is_file()
            else "unavailable_in_mock_only_unit_test",
            "template_contract": contract,
            "platform_scope": {
                "macos": "template contract only on this host",
                "linux": "render contract only; systemctl was not called",
                "windows": "render contract only; schtasks was not called",
            },
        }
        errors = templates_acceptance_errors(record)
        if errors:
            raise RuntimeError(f"templates-only acceptance failed: {', '.join(errors)}")
        return record
    home = pathlib.Path(tempfile.mkdtemp(prefix="llmgw-native-task5-")).resolve()
    worker_start_attempted = False
    worker_cleanup_confirmed = False
    record = None
    original_failure: BaseException | None = None
    try:
        config = home / "설정 & $100%" / "gateway <fixture>.toml"
        config.parent.mkdir(parents=True)
        config.write_text(config_text(free_port(), 'mode = "none"'), encoding="utf-8")

        stopped_before = status(binary, config, home)
        on_preview, _ = apply_autostart(binary, config, home, "on")
        stopped_after_on = status(binary, config, home)
        target = pathlib.Path(stopped_after_on["autostart"]["target"])
        definition = target.read_text(encoding="utf-8")
        validators: dict[str, dict] = {
            "macos_plutil": {"available": False, "executed": False},
            "linux_systemd_analyze": {"available": shutil.which("systemd-analyze") is not None, "executed": False},
            "windows_task_scheduler": {"available": shutil.which("schtasks") is not None, "executed": False},
        }
        plutil = pathlib.Path("/usr/bin/plutil")
        if sys.platform == "darwin" and plutil.is_file():
            checked = subprocess.run(
                [str(plutil), "-lint", str(target)],
                stdin=subprocess.DEVNULL,
                stdout=subprocess.PIPE,
                stderr=subprocess.STDOUT,
                text=True,
                timeout=10,
                check=False,
            )
            validators["macos_plutil"] = {
                "available": True,
                "executed": True,
                "exit_code": checked.returncode,
                "output": checked.stdout.strip(),
            }
            if checked.returncode != 0:
                raise RuntimeError("plutil rejected owned temporary LaunchAgent")

        worker_start_attempted = True
        started = cli(binary, config, home, "on")
        if started.returncode != 0:
            raise RuntimeError(f"manual on fixture failed: {started.stderr}")
        running_before_off = status(binary, config, home)
        nonce = running_before_off["identity"]["nonce"]
        off_preview, _ = apply_autostart(binary, config, home, "off")
        running_after_off = status(binary, config, home)
        if running_after_off.get("identity", {}).get("nonce") != nonce:
            raise RuntimeError("autostart off changed the authenticated worker identity")
        stopped = cli(binary, config, home, "off")
        if stopped.returncode != 0:
            raise RuntimeError(f"manual authenticated off cleanup failed: {stopped.stderr}")
        manual_cleanup = status(binary, config, home)
        if manual_cleanup["state"] != "stopped":
            raise RuntimeError("manual authenticated off did not leave the fixture stopped")
        worker_cleanup_confirmed = True

        secret = "synthetic-native-task5-secret"
        auth_config = home / "빈 로그인 환경" / "gateway auth.toml"
        auth_config.parent.mkdir(parents=True)
        auth_config.write_text(
            config_text(free_port(), 'mode = "env"\nheader = "authorization"\nname = "LLMGW_LOGIN_FIXTURE_SECRET"'),
            encoding="utf-8",
        )
        shell_env = safe_child_env(home)
        shell_env["LLMGW_LOGIN_FIXTURE_SECRET"] = secret
        preview = run(
            [str(binary), "autostart", "on", "--config", str(auth_config)],
            shell_env,
        )
        digest = preview_hash(preview.stdout)
        applied = run(
            [str(binary), "autostart", "on", "--apply-hash", digest, "--config", str(auth_config)],
            shell_env,
        )
        if applied.returncode != 0:
            raise RuntimeError(f"auth registration fixture failed: {applied.stderr}")
        auth_status = status(binary, auth_config, home)
        auth_target = pathlib.Path(auth_status["autostart"]["target"])
        auth_definition = auth_target.read_text(encoding="utf-8")
        login_attempt = cli(binary, auth_config, home, "run")
        apply_autostart(binary, auth_config, home, "off")

        record = {
            "schema_version": 2,
            "mode": "templates_only",
            "actual_user_registration_changed": False,
            "actual_login_verified": False,
            "host_file_fixture_executed": True,
            "binary": str(binary),
            "binary_sha256": hashlib.sha256(binary.read_bytes()).hexdigest(),
            "template_contract": contract,
            "validators": validators,
            "platform_scope": {
                "macos": "owned temporary HOME fixture plus plutil; no launchctl bootstrap or bootout",
                "linux": "template contract only on this host; real user manager unverified",
                "windows": "template contract only on this host; real Task Scheduler unverified",
            },
            "runtime_separation": {
                "stopped_before_registration": stopped_before["state"],
                "stopped_after_autostart_on": stopped_after_on["state"],
                "worker_processes_after_autostart_on": 0 if stopped_after_on["state"] == "stopped" else "unexpected",
                "authenticated_identity_preserved": running_after_off["identity"]["nonce"] == nonce,
                "worker_preserved": running_after_off["state"] == "running" and running_after_off["identity"]["nonce"] == nonce,
                "manual_cleanup_state": manual_cleanup["state"],
            },
            "login_environment": {
                "registration": auth_status["autostart"]["registration"],
                "unobserved_login_auth_availability": auth_status["autostart"]["login_auth_availability"],
                "observed_empty_login_auth_availability": "configured_but_unavailable"
                if login_attempt.returncode != 0 and missing_login_auth(login_attempt.stderr)
                else "observed_not_missing",
                "empty_login_environment_run_exit": login_attempt.returncode,
                "empty_login_environment_run_failed_for_missing_auth": login_attempt.returncode != 0 and missing_login_auth(login_attempt.stderr),
                "secret_copied_into_registration": secret in auth_definition,
                "auth_env_name_copied_into_registration": "LLMGW_LOGIN_FIXTURE_SECRET" in auth_definition,
            },
            "template_paths": {
                "host_target_was": str(target),
                "host_definition_bytes": len(definition.encode("utf-8")),
            },
            "preview_contract": {
                "on_printed_before_apply": "preview hash:" in on_preview,
                "off_printed_before_apply": "preview hash:" in off_preview,
            },
        }
        errors = templates_acceptance_errors(record)
        if errors:
            raise RuntimeError(f"templates-only acceptance failed: {', '.join(errors)}")
        record["passed"] = True
    except BaseException as error:
        original_failure = error

    cleanup_failure = None
    if worker_start_attempted and not worker_cleanup_confirmed:
        try:
            cleanup = cli(binary, config, home, "off")
            if cleanup.returncode != 0:
                cleanup_failure = f"authenticated off exited {cleanup.returncode}"
            else:
                cleanup_status = status(binary, config, home)
                if cleanup_status.get("state") == "stopped":
                    worker_cleanup_confirmed = True
                else:
                    cleanup_failure = "authenticated off did not confirm a stopped fixture"
        except BaseException:
            cleanup_failure = "authenticated off or its status confirmation raised an error"

    if not worker_start_attempted or worker_cleanup_confirmed:
        try:
            shutil.rmtree(home)
        except OSError as error:
            if original_failure is not None:
                raise RuntimeError(
                    f"{original_failure}; owned fixture removal failed; recovery files retained at {home}"
                ) from original_failure
            raise error
    else:
        detail = cleanup_failure or "authenticated cleanup was not confirmed"
        if original_failure is not None:
            raise RuntimeError(
                f"{original_failure}; authenticated cleanup failed ({detail}); "
                f"recovery files retained at {home}"
            ) from original_failure
        raise RuntimeError(
            f"authenticated cleanup failed ({detail}); recovery files retained at {home}"
        )

    if original_failure is not None:
        raise original_failure
    assert record is not None
    return record


def iso_timestamp(value: str | None) -> str:
    if not value:
        raise ValueError("--human-login-at with an explicit timezone is required for login observation stages")
    parsed = dt.datetime.fromisoformat(value)
    if parsed.tzinfo is None:
        raise ValueError("human login timestamp must include a timezone")
    return parsed.isoformat()


def manual_stage(args: argparse.Namespace, binary: pathlib.Path, config: pathlib.Path) -> tuple[dict, int]:
    home = pathlib.Path.home().resolve()
    terminal_env = terminal_child_env(home)
    current = status(binary, config, home, terminal_env)
    registration = current["autostart"]
    action = "off" if args.stage == "remove" else "on"
    print(f"generated/removed label: {registration.get('label', 'unknown')}")
    print(f"generated/removed path: {registration.get('target', 'unknown')}")
    print(f"foreground argv: {registration.get('executable', binary)} --config {config} run")
    record = {
        "schema_version": 1,
        "mode": "exercise_user_service",
        "stage": args.stage,
        "temporary_os_account_confirmed": bool(args.confirm_temporary_os_account),
        "execution_explicit": bool(args.execute),
        "label": registration.get("label"),
        "path": registration.get("target"),
        "config": str(config),
        "binary": str(binary),
        "actual_login_verified": False,
    }
    if not args.execute or not args.confirm_temporary_os_account:
        record["result"] = "preview_only_explicit_execution_and_temporary_account_confirmation_required"
        return record, 2

    if args.stage == "install":
        preview, _ = apply_autostart(binary, config, home, "on", terminal_env)
        record.update(result="installed_for_next_login", preview=preview)
    elif args.stage == "manual-cycle":
        before = status(binary, config, home, terminal_env)
        on = cli(binary, config, home, "on", environment=terminal_env)
        running = status(binary, config, home, terminal_env)
        off = cli(binary, config, home, "off", environment=terminal_env)
        after = status(binary, config, home, terminal_env)
        lifecycle_passed = (
            on.returncode == 0
            and running["state"] == "running"
            and off.returncode == 0
            and after["state"] == "stopped"
        )
        record.update(
            result="manual_cycle_observed" if lifecycle_passed else "manual_cycle_failed",
            before=before["state"],
            on_exit=on.returncode,
            running=running["state"],
            off_exit=off.returncode,
            after=after["state"],
            terminal_auth_availability="available" if on.returncode == 0 else "unavailable",
            login_auth_availability=running["autostart"]["login_auth_availability"],
        )
        return record, 0 if lifecycle_passed else 1
    elif args.stage == "observe-login-on":
        observed = status(binary, config, home, terminal_env)
        verified = login_on_verified(observed, args.confirm_human_login_observed)
        record.update(
            result="human_login_observation_recorded",
            human_login_at=iso_timestamp(args.human_login_at),
            human_login_event_attested=bool(args.confirm_human_login_observed),
            observed=login_observation_summary(observed),
            actual_login_verified=verified,
        )
        return record, 0 if verified else 1
    elif args.stage == "remove":
        preview, _ = apply_autostart(binary, config, home, "off", terminal_env)
        record.update(result="removed_for_next_login", preview=preview)
    elif args.stage == "observe-login-off":
        observed = status(binary, config, home, terminal_env)
        verified = login_off_verified(observed, args.confirm_human_login_observed)
        record.update(
            result="human_login_off_observation_recorded",
            human_login_at=iso_timestamp(args.human_login_at),
            human_login_event_attested=bool(args.confirm_human_login_observed),
            observed=login_observation_summary(observed),
            actual_login_verified=verified,
        )
        return record, 0 if verified else 1
    else:
        raise ValueError(f"unsupported stage {args.stage}")
    return record, 0


def arguments() -> argparse.Namespace:
    parser = argparse.ArgumentParser(description=__doc__)
    mode = parser.add_mutually_exclusive_group(required=True)
    mode.add_argument("--templates-only", action="store_true")
    mode.add_argument("--exercise-user-service", action="store_true")
    parser.add_argument("--output", type=pathlib.Path, required=True)
    parser.add_argument("--binary", type=pathlib.Path, required=True)
    parser.add_argument("--cargo-target-dir", type=pathlib.Path)
    parser.add_argument("--config", type=pathlib.Path)
    parser.add_argument(
        "--stage",
        choices=["install", "manual-cycle", "observe-login-on", "remove", "observe-login-off"],
        default="install",
    )
    parser.add_argument("--execute", action="store_true")
    parser.add_argument("--confirm-temporary-os-account", action="store_true")
    parser.add_argument("--human-login-at")
    parser.add_argument(
        "--confirm-human-login-observed",
        action="store_true",
        help="attest that a human performed this login and did not manually start the worker afterward",
    )
    return parser.parse_args()


def main() -> int:
    args = arguments()
    binary = args.binary.resolve()
    if not binary.is_file():
        raise SystemExit(f"binary is unavailable: {binary}")
    if args.templates_only:
        if args.cargo_target_dir is None:
            raise SystemExit("--cargo-target-dir is required with --templates-only")
        record = templates_only(binary, args.cargo_target_dir)
        code = 0
    else:
        if args.config is None:
            raise SystemExit("--config is required with --exercise-user-service")
        config = args.config.resolve()
        if not config.is_file():
            raise SystemExit(f"config is unavailable: {config}")
        record, code = manual_stage(args, binary, config)
    args.output.parent.mkdir(parents=True, exist_ok=True)
    with args.output.open("x", encoding="utf-8") as output:
        output.write(json.dumps(record, ensure_ascii=False, indent=2) + "\n")
    print(f"wrote {args.output}")
    return code


if __name__ == "__main__":
    raise SystemExit(main())
