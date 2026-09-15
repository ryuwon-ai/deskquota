"""Verify a temporary login task on Windows with the current user logged in.

Uses Python's standard library. Registration and a manual Task Scheduler run
are checked; a real logout/login is deliberately outside this probe.
"""

import argparse
import ctypes
import hashlib
import json
import os
import subprocess
import time
from ctypes import wintypes
from pathlib import Path


def is_elevated(pid):
    kernel = ctypes.WinDLL("kernel32", use_last_error=True)
    security = ctypes.WinDLL("advapi32", use_last_error=True)
    kernel.OpenProcess.argtypes = [wintypes.DWORD, wintypes.BOOL, wintypes.DWORD]
    kernel.OpenProcess.restype = wintypes.HANDLE
    kernel.CloseHandle.argtypes = [wintypes.HANDLE]
    security.OpenProcessToken.argtypes = [
        wintypes.HANDLE, wintypes.DWORD, ctypes.POINTER(wintypes.HANDLE)
    ]
    security.GetTokenInformation.argtypes = [
        wintypes.HANDLE, ctypes.c_int, wintypes.LPVOID, wintypes.DWORD,
        ctypes.POINTER(wintypes.DWORD),
    ]
    process = kernel.OpenProcess(0x1000, False, pid)  # QUERY_LIMITED_INFORMATION
    token = wintypes.HANDLE()
    try:
        if not process or not security.OpenProcessToken(process, 8, ctypes.byref(token)):
            raise ctypes.WinError(ctypes.get_last_error())
        elevated, size = wintypes.DWORD(), wintypes.DWORD()
        if not security.GetTokenInformation(
            token, 20, ctypes.byref(elevated), ctypes.sizeof(elevated), ctypes.byref(size)
        ):  # TokenElevation
            raise ctypes.WinError(ctypes.get_last_error())
        return bool(elevated.value)
    finally:
        if token:
            kernel.CloseHandle(token)
        if process:
            kernel.CloseHandle(process)


def field(output, name):
    return next(
        line.split(": ", 1)[1]
        for line in output.splitlines() if line.startswith(name + ": ")
    )


def verify(binary, work):
    work.mkdir(parents=True, exist_ok=False)
    config = work / "gateway.toml"
    config.write_text('''listen = "127.0.0.1:0"
startup_hold_secs = 0
concurrency = 1
[upstream]
api_base = "http://127.0.0.1:9/v1"
[upstream.auth]
mode = "none"
[quota.rpm]
kind = "known"
value = 18
[quota.tpm]
kind = "known"
value = 450000
[[models]]
id = "synthetic"
max_output_tokens = 64
[[roots]]
id = "one"
endpoints = ["chat/completions"]
models = ["synthetic"]
''', encoding="utf-8")
    inherited = {"SYSTEMROOT", "WINDIR", "COMSPEC", "PATH", "PATHEXT", "TEMP", "TMP"}
    env = {key: value for key, value in os.environ.items() if key.upper() in inherited}
    for name, suffix in [
        ("LOCALAPPDATA", "local"), ("APPDATA", "roaming"),
        ("HOME", "home"), ("USERPROFILE", "home"),
    ]:
        path = work / suffix
        path.mkdir(exist_ok=True)
        env[name] = str(path)
    cli = [str(binary), "--config", str(config)]

    def run(command):
        return subprocess.run(
            command, env=env, capture_output=True, timeout=20,
            encoding="utf-8", errors="replace",
        )

    result = {"passed": False, "login_event_tested": False}
    installed = False
    try:
        preview = run(cli + ["autostart", "on"])
        assert "preview hash: " in preview.stdout, preview.stderr
        label, target = field(preview.stdout, "label"), Path(field(preview.stdout, "target"))
        assert label.startswith("io.llmgw.gateway.") and target.is_relative_to(work)
        result["label"] = label
        applied = run(cli + ["autostart", "on", "--apply-hash", field(preview.stdout, "preview hash")])
        installed = target.exists()
        result["apply_exit"] = applied.returncode
        assert applied.returncode == 0, applied.stderr
        started = run(["schtasks.exe", "/Run", "/TN", "\\llmgw\\" + label])
        result["task_run_exit"] = started.returncode
        assert started.returncode == 0, started.stderr
        for _ in range(30):
            status = run(cli + ["status", "--json"])
            if status.returncode == 0:
                state = json.loads(status.stdout)
                if state.get("state") == "running":
                    break
            time.sleep(0.2)
        else:
            raise RuntimeError("Scheduled worker did not become ready")
        result.update(
            worker_pid=state["identity"]["pid"], worker_state=state["state"],
            registration=state["autostart"]["registration"],
            manager=state["autostart"]["manager"],
            manager_ownership=state["autostart"]["manager_ownership"],
            definition_least_privilege="LeastPrivilege" in target.read_text(encoding="utf-16"),
        )
        result["worker_token_elevated"] = is_elevated(result["worker_pid"])
        assert result["registration"] == "registered"
        assert result["manager_ownership"] == "owned"
        assert result["definition_least_privilege"] and not result["worker_token_elevated"]
        result["binary_sha256"] = hashlib.sha256(binary.read_bytes()).hexdigest()
        result["passed"] = True
    except Exception as error:
        result["error"] = str(error)
    finally:
        result["off_exit"] = run(cli + ["off"]).returncode
        if installed:
            preview = run(cli + ["autostart", "off"])
            if "preview hash: " in preview.stdout:
                removed = run(cli + ["autostart", "off", "--apply-hash", field(preview.stdout, "preview hash")])
                result["remove_exit"] = removed.returncode
                if removed.returncode:
                    result["remove_error"] = removed.stderr
            else:
                result["remove_preview_error"] = preview.stderr
        status = run(cli + ["status", "--json"])
        if status.returncode == 0:
            result["final_status"] = json.loads(status.stdout)
        final = result.get("final_status", {})
        result["passed"] = (
            result["passed"] and result["off_exit"] == 0 and result.get("remove_exit") == 0
            and final.get("state") == "stopped"
            and final.get("autostart", {}).get("manager") == "task_absent"
        )
        (work / "result.json").write_text(json.dumps(result, indent=2) + "\n", encoding="utf-8")
        print(json.dumps(result, indent=2))
    return result["passed"]


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--binary", type=Path, required=True)
    parser.add_argument("--work", type=Path, required=True, help="New isolated directory outside OneDrive")
    args = parser.parse_args()
    if os.name != "nt":
        parser.error("Run on Windows with the current user logged in")
    return 0 if verify(args.binary.resolve(strict=True), args.work.resolve()) else 1


if __name__ == "__main__":
    raise SystemExit(main())
