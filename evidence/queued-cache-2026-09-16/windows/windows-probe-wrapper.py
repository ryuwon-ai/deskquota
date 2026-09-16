"""Run the unchanged rejection-head probe with isolated native Windows state."""
import ast
import base64
import socket
import hashlib
import importlib.util
import json
import os
from pathlib import Path
import subprocess
import sys
import tempfile
import types

owned = Path(os.environ["LOCALAPPDATA"]) / "deskquota-windows-lab-20260915"
run = owned / "queued-cache-20260916"
source = run / "source"
probe_path = source / "scripts/probe-rejection-head.py"
windows_temp = run / "probe-temp"
windows_temp.mkdir(parents=True, exist_ok=True)
tempfile.tempdir = str(windows_temp)
# Only two helpers are used by this probe. Avoid unrelated macOS fcntl imports.
measure_path = source / "product/scripts/measure_native.py"
module_tree = ast.parse(measure_path.read_text(encoding="utf-8"))
free_port_node = next(node for node in module_tree.body if isinstance(node, ast.FunctionDef) and node.name == "free_port")
helper_namespace = {"socket": socket}
exec(compile(ast.Module(body=[free_port_node], type_ignores=[]), str(measure_path), "exec"), helper_namespace)
measure = types.ModuleType("measure_native")
measure.free_port = helper_namespace["free_port"]
measure.safe_environment = None  # Replaced by the native isolated implementation below.
sys.modules["measure_native"] = measure
spec = importlib.util.spec_from_file_location("rejection_head", probe_path)
probe = importlib.util.module_from_spec(spec)
spec.loader.exec_module(probe)
system = Path(os.environ["SystemRoot"])
powershell = system / "System32/WindowsPowerShell/v1.0/powershell.exe"


def windows_environment(home):
    home = home.resolve()
    if not home.is_relative_to(windows_temp.resolve()):
        raise AssertionError("home_outside_owned_probe_temp")
    temporary = home / "tmp"
    temporary.mkdir(parents=True)
    appdata = home / "AppData/Local"
    appdata.mkdir(parents=True)
    return {"SystemRoot": str(system), "WINDIR": str(system),
            "PATH": str(system / "System32") + ";" + str(system),
            "HOME": str(home), "USERPROFILE": str(home), "LOCALAPPDATA": str(appdata),
            "TEMP": str(temporary), "TMP": str(temporary)}


def native_check_output(arguments, **kwargs):
    output = subprocess.check_output(arguments, **kwargs)
    if "doctor" not in arguments:
        raise AssertionError("unexpected_adapter_check_output")
    result = json.loads(output)
    state = result["state_directory"]
    prefix = "\\" * 2 + "?" + "\\"
    if state.startswith(prefix):
        state = state[len(prefix):]
    normalized = Path(state).resolve()
    if not normalized.is_relative_to(windows_temp.resolve()):
        raise AssertionError("native_state_outside_owned_probe_temp")
    result["state_directory"] = str(normalized)
    return json.dumps(result).encode("utf-8")


def private_popen(arguments, **kwargs):
    config = Path(arguments[arguments.index("--config") + 1])
    if not config.resolve().is_relative_to(windows_temp.resolve()):
        raise AssertionError("state_outside_owned_probe_temp")
    directories = list(config.parent.glob(".llmgw-*"))
    if len(directories) != 1 or not (directories[0] / "control-token").is_file():
        raise AssertionError("unexpected_probe_state")
    state = str(directories[0]).replace("'", "''")
    script = "$ErrorActionPreference='Stop'; $sid=[Security.Principal.WindowsIdentity]::GetCurrent().User; "
    script += "$d='" + state + "'; "
    script += "foreach ($p in @($d,(Join-Path $d 'control-token'))) { "
    script += "if (Test-Path $p -PathType Container) {$acl=New-Object Security.AccessControl.DirectorySecurity} else {$acl=New-Object Security.AccessControl.FileSecurity}; "
    script += "$acl.SetOwner($sid); $acl.SetAccessRuleProtection($true,$false); "
    script += "$rule=New-Object Security.AccessControl.FileSystemAccessRule($sid,'FullControl','Allow'); $acl.AddAccessRule($rule); Set-Acl -LiteralPath $p -AclObject $acl }"
    subprocess.run([str(powershell), "-NoProfile", "-NonInteractive", "-EncodedCommand",
                    base64.b64encode(script.encode("utf-16le")).decode()], check=True,
                   stdout=subprocess.PIPE, stderr=subprocess.PIPE, timeout=10)
    return subprocess.Popen(arguments, **kwargs)


probe.safe_environment = windows_environment
probe.subprocess = types.SimpleNamespace(Popen=private_popen, check_output=native_check_output,
                                        PIPE=subprocess.PIPE, TimeoutExpired=subprocess.TimeoutExpired)
binary = run / "llmgw.exe"
output = run / "rejection-head.json"
sys.argv = [str(probe_path), "--binary", str(binary), "--output", str(output),
            "--expect", "stream-ineligible"]
code = probe.main()
result = json.loads(output.read_text())
result["native_windows_adapter"] = {"wrapper_sha256": hashlib.sha256(Path(__file__).read_bytes()).hexdigest(),
    "probe_sha256": hashlib.sha256(probe_path.read_bytes()).hexdigest(),
    "changes": ["isolated Windows process environment", "owned temporary directory", "current-user-only protected state ACL", "AST-extracted unchanged free_port avoids unrelated fcntl import", "normalized verbatim Windows doctor path for containment check"],
    "fixture_semantics_changed": False}
output.write_text(json.dumps(result, indent=2) + "\n")
raise SystemExit(code)
