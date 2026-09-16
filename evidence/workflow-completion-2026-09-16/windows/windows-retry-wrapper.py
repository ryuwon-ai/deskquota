"""Windows environment and private-state adapter for the unchanged eleven-case probe."""
import base64
import hashlib
import importlib.util
import json
import os
from pathlib import Path
import subprocess
import shutil
import sys
import tempfile
import types
import traceback

owned = Path(os.environ["LOCALAPPDATA"]) / "deskquota-windows-lab-20260915"
run = owned / "workflow-20260916"
mode = sys.argv[1]
assert mode in ("default", "bpe")
trial = sys.argv[2] if len(sys.argv) > 2 else mode
assert trial in ("default", "bpe", "default-diagnostic", "default-final", "bpe-final")
probe_path = run / "source/scripts/probe-retry-directives.py"
windows_temp = run / ("retry-temp-" + trial)
windows_temp.mkdir(exist_ok=False)
tempfile.tempdir = str(windows_temp)
system = Path(os.environ["SystemRoot"])
powershell = system / "System32/WindowsPowerShell/v1.0/powershell.exe"
env_home = windows_temp / "home"
env_temporary = env_home / "tmp"
env_temporary.mkdir(parents=True)
env_appdata = env_home / "AppData/Local"
env_appdata.mkdir(parents=True)
native_environment = {"SystemRoot": str(system), "WINDIR": str(system),
    "PATH": str(system / "System32") + ";" + str(system), "HOME": str(env_home),
    "USERPROFILE": str(env_home), "LOCALAPPDATA": str(env_appdata),
    "TEMP": str(env_temporary), "TMP": str(env_temporary)}


def native_check_output(arguments, **kwargs):
    kwargs["env"] = native_environment
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
        raise AssertionError("state_outside_owned_probe_temp")
    result["state_directory"] = str(normalized)
    return json.dumps(result).encode("utf-8")


def private_popen(arguments, **kwargs):
    config = Path(arguments[arguments.index("--config") + 1])
    if not config.resolve().is_relative_to(windows_temp.resolve()):
        raise AssertionError("config_outside_owned_probe_temp")
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
        base64.b64encode(script.encode("utf-16le")).decode()], env=native_environment,
        check=True, stdout=subprocess.PIPE, stderr=subprocess.PIPE, timeout=10)
    kwargs["env"] = native_environment
    return subprocess.Popen(arguments, **kwargs)


spec = importlib.util.spec_from_file_location("retry_directives", probe_path)
probe = importlib.util.module_from_spec(spec)
spec.loader.exec_module(probe)
probe.fixture.subprocess = types.SimpleNamespace(Popen=private_popen,
    check_output=native_check_output, PIPE=subprocess.PIPE, TimeoutExpired=subprocess.TimeoutExpired)
original_setup = probe.fixture.Runner.setup

def observed_setup(self):
    try:
        return original_setup(self)
    except Exception as error:
        detail = {"setup_error": type(error).__name__,
            "frames": [{"file": Path(frame.filename).name, "line": frame.lineno, "function": frame.name}
                for frame in traceback.extract_tb(error.__traceback__)]}
        if isinstance(error, probe.fixture.CheckFailed): detail["fixed_check"] = str(error)
        if isinstance(error, subprocess.CalledProcessError):
            detail["command_exit"] = error.returncode
            detail["stderr"] = (error.stderr or b"").decode("utf-8", errors="replace")[:1500]
        print(json.dumps(detail), file=sys.stderr)
        raise

probe.fixture.Runner.setup = observed_setup
# The Unix harness clears os.environ, but Windows socket initialization requires
# SystemRoot. Keep the real parent and every child in the same minimal native
# environment, while the unchanged harness manipulates an isolated shadow map.
os.environ.clear()
os.environ.update(native_environment)
probe.os = types.SimpleNamespace(environ=dict(native_environment))
binary = run / ("llmgw-" + mode + ".exe")
output = run / (trial + "-retry-directives.json")
sys.argv = [str(probe_path), "--binary", str(binary), "--output", str(output), "--expect", "retry-veto"]
code = probe.main()
result = json.loads(output.read_text(encoding="utf-8"))
result["native_windows_adapter"] = {"wrapper_sha256": hashlib.sha256(Path(__file__).read_bytes()).hexdigest(),
    "changes": ["isolated Windows process environment with system-only PATH",
        "owned temporary root", "current-user-only protected state ACL",
        "normalized verbatim doctor state path", "retained SystemRoot for native socket initialization; isolated harness environment map"], "fixture_semantics_changed": False,
    "python_utf8_mode": bool(sys.flags.utf8_mode), "mode": mode}
# The unchanged original probe's inherited limitation is retained as source metadata,
# but this run explicitly covers Windows through the reviewed adapter.
result["windows_executed"] = True
result["source_limitations"] = result.pop("limitations")
result["limitations"] = ["Native Windows loopback contract checks with a platform adapter",
    "No real provider, client SDK, paid API or performance comparison",
    "Unlimited RPM and unknown TPM isolate retry policy from accounting"]
output.write_text(json.dumps(result, indent=2) + "\n", encoding="utf-8")
if code == 0:
    shutil.rmtree(windows_temp)
raise SystemExit(code)
