"""Owned native CLI lifecycle checks across optional BPE build capabilities."""
import hashlib
import json
import os
from pathlib import Path
import shutil
import subprocess
import time

run = Path(os.environ["LOCALAPPDATA"]) / "deskquota-windows-lab-20260915/workflow-20260916"
base = run / "native-lifecycle"
base.mkdir(exist_ok=False)
system = Path(os.environ["SystemRoot"])
home = base / "home"
temporary = home / "tmp"
temporary.mkdir(parents=True)
appdata = home / "AppData/Local"
appdata.mkdir(parents=True)
environment = {"SystemRoot": str(system), "WINDIR": str(system),
    "PATH": str(system / "System32") + ";" + str(system), "HOME": str(home),
    "USERPROFILE": str(home), "LOCALAPPDATA": str(appdata), "TEMP": str(temporary), "TMP": str(temporary)}
binaries = {mode: run / ("llmgw-" + mode + ".exe") for mode in ("default", "bpe")}
result = {"check": "windows_native_build_capabilities", "passed": False,
    "script_sha256": hashlib.sha256(Path(__file__).read_bytes()).hexdigest(),
    "binaries": {mode: {"sha256": hashlib.sha256(binary.read_bytes()).hexdigest(),
        "bytes": binary.stat().st_size} for mode, binary in binaries.items()},
    "restricted_path": environment["PATH"], "commands": [], "checks": {}}


def require(value, name):
    result["checks"][name] = bool(value)
    if not value:
        raise AssertionError(name)


def config(name, bpe):
    path = base / (name + ".toml")
    text = ("listen = \"127.0.0.1:0\"\nstartup_hold_secs = 0\nconcurrency = 1\n"
        "[upstream]\napi_base = \"http://127.0.0.1:9/v1\"\n[upstream.auth]\nmode = \"none\"\n"
        "[quota.rpm]\nkind = \"known\"\nvalue = 18\n[quota.tpm]\nkind = \"known\"\nvalue = 450000\n"
        "[[models]]\nid = \"synthetic\"\nmax_output_tokens = 64\n")
    if bpe:
        text += "input_estimator = \"cl100k_base\"\ninput_token_overhead = 32\n"
    text += "[[roots]]\nid = \"one\"\nendpoints = [\"chat/completions\"]\nmodels = [\"synthetic\"]\n"
    path.write_text(text, encoding="utf-8")
    return path


def cli(mode, cfg, operation, expect=0):
    args = [str(binaries[mode]), operation, "--config", str(cfg)]
    if operation in ("status", "doctor"):
        args.append("--json")
    start = time.monotonic()
    call = subprocess.run(args, env=environment, stdin=subprocess.DEVNULL,
        capture_output=True, timeout=15)
    row = {"mode": mode, "operation": operation, "config": cfg.name,
        "exit": call.returncode, "elapsed_ms": (time.monotonic() - start) * 1000}
    value = None
    if call.returncode == 0:
        if call.stdout:
            try:
                value = json.loads(call.stdout)
                if "state" in value: row["state"] = value["state"]
                if "identity" in value:
                    row["identity_sha256"] = hashlib.sha256(json.dumps(value["identity"], sort_keys=True).encode()).hexdigest()
            except json.JSONDecodeError:
                pass
    else:
        row["stderr"] = call.stderr.decode("utf-8", errors="replace")[:2000]
    result["commands"].append(row)
    if expect is not None:
        require(call.returncode == expect, mode + "_" + cfg.stem + "_" + operation + "_exit")
    return call, value


configs = []
try:
    for mode in ("default", "bpe"):
        cfg = config("ordinary-" + mode, mode == "bpe")
        configs.append((mode, cfg))
        cli(mode, cfg, "doctor")
        cli(mode, cfg, "on")
        _, first = cli(mode, cfg, "status")
        require(first["state"] == "running", mode + "_ordinary_running")
        cli(mode, cfg, "on")
        _, again = cli(mode, cfg, "status")
        require(first["identity"] == again["identity"], mode + "_idempotent_on_identity")
        cli(mode, cfg, "off")
        _, stopped = cli(mode, cfg, "status")
        require(stopped["state"] == "stopped", mode + "_ordinary_stopped")
    cfg = config("cross-capability", True)
    configs.append(("bpe", cfg))
    cli("bpe", cfg, "on")
    _, full = cli("bpe", cfg, "status")
    _, narrow = cli("default", cfg, "status")
    require(full["identity"] == narrow["identity"] and narrow["state"] == "running",
        "default_can_inspect_bpe_worker")
    rejected, _ = cli("default", cfg, "restart", expect=None)
    require(rejected.returncode != 0 and b"--features bpe" in rejected.stderr,
        "default_restart_reports_missing_bpe")
    _, after = cli("bpe", cfg, "status")
    require(full["identity"] == after["identity"] and after["state"] == "running",
        "default_restart_preserves_healthy_bpe_identity")
    cli("default", cfg, "off")
    _, stopped = cli("default", cfg, "status")
    require(stopped["state"] == "stopped", "default_can_stop_bpe_worker")
    rejected, _ = cli("default", cfg, "on", expect=None)
    require(rejected.returncode != 0 and b"--features bpe" in rejected.stderr,
        "default_new_on_rejects_bpe_config")
    _, stopped = cli("bpe", cfg, "status")
    require(stopped["state"] == "stopped", "rejected_start_created_no_worker")
    result["passed"] = all(result["checks"].values())
except Exception as error:
    result["error_class"] = type(error).__name__
    if isinstance(error, AssertionError): result["check_failed"] = str(error)
finally:
    for mode, cfg in configs:
        try:
            cli(mode, cfg, "off")
        except Exception as error:
            result.setdefault("cleanup_errors", []).append(type(error).__name__)
    result["passed"] &= not result.get("cleanup_errors")
    if result["passed"]:
        shutil.rmtree(base)
    result["owned_lifecycle_temp_removed"] = not base.exists()
    (run / "native-modes.json").write_text(json.dumps(result, indent=2) + "\n", encoding="utf-8")
print(json.dumps({"passed": result["passed"], "checks": result["checks"], "error": result.get("check_failed")}))
raise SystemExit(0 if result["passed"] else 1)
