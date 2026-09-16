#!/usr/bin/env python3
"""Windows release acceptance with synthetic loopback traffic; Python 3.11+ stdlib only."""
import argparse
import hashlib
import http.client
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
import json
import math
import os
from pathlib import Path
import socket
import subprocess
import threading
import time
import zipfile


def sha(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def run(argv, *, env=None, check=True):
    if Path(argv[0]).name.lower() == "powershell.exe":
        # Python inherits PS7 module paths; PS5 must construct its own defaults.
        env = {k: v for k, v in (os.environ if env is None else env).items()
               if k.upper() != "PSMODULEPATH"}
    result = subprocess.run([str(v) for v in argv], env=env, capture_output=True,
                            text=True, encoding="utf-8", errors="replace", timeout=30)
    if check and result.returncode:
        # This probe runs only owned synthetic inputs; preserve the actual failure.
        raise RuntimeError(f"{Path(argv[0]).name} exited {result.returncode}: {result.stderr or result.stdout}")
    return result


def path_registry():
    import winreg
    values = []
    for hive, key in [(winreg.HKEY_CURRENT_USER, "Environment"),
                      (winreg.HKEY_LOCAL_MACHINE, r"SYSTEM\CurrentControlSet\Control\Session Manager\Environment")]:
        with winreg.OpenKey(hive, key) as opened:
            values.append(winreg.QueryValueEx(opened, "Path"))
    return values


def resources(pid, env):
    code = (f"$p=Get-Process -Id {pid}; "
            "@{cpu_seconds=$p.TotalProcessorTime.TotalSeconds;working_set_bytes=$p.WorkingSet64;"
            "private_bytes=$p.PrivateMemorySize64;handles=$p.HandleCount;threads=$p.Threads.Count}"
            " | ConvertTo-Json -Compress")
    return json.loads(run(["powershell.exe", "-NoProfile", "-NonInteractive", "-Command", code], env=env).stdout)


def verify(binary, work, product):
    registry_before = path_registry()
    work.mkdir(parents=True, exist_ok=False)
    archive = work / "llmgw-windows-x64.zip"
    with zipfile.ZipFile(archive, "w", zipfile.ZIP_DEFLATED, strict_timestamps=False) as z:
        z.write(binary, "llmgw.exe")
        for name in ["README.md", "docs/installation.md", "docs/runtime-contract.md", "docs/client-compatibility.md"]:
            z.write(product / name, name)
    manifest = work / "checksums.sha256"
    manifest.write_text(f"{sha(archive)}  {archive.name}\n", encoding="ascii")
    install_dir = work / "installed space"
    installer = ["powershell.exe", "-NoProfile", "-NonInteractive", "-File", product / "packaging/install.ps1",
                 "-Archive", archive, "-ChecksumManifest", manifest, "-InstallDir", install_dir]
    first = run(installer)
    installed = install_dir / "llmgw.exe"
    assert sha(installed) == sha(binary)
    run(installer)
    assert sha(installed) == sha(binary)
    manifest.write_text(f"{'0' * 64}  {archive.name}\n", encoding="ascii")
    bad = run(installer, check=False)
    assert bad.returncode != 0 and "checksum_mismatch" in bad.stderr
    assert sha(installed) == sha(binary)
    manifest.write_text(f"{sha(archive)}  {archive.name}\n", encoding="ascii")
    assert path_registry() == registry_before

    env = {k: v for k, v in os.environ.items() if k.upper() in
           {"SYSTEMROOT", "WINDIR", "COMSPEC", "PATHEXT", "TEMP", "TMP"}}
    env["PATH"] = str(Path(os.environ["SystemRoot"]) / "System32") + ";" + os.environ["SystemRoot"]
    for key, sub in [("USERPROFILE", "home"), ("APPDATA", "home/AppData/Roaming"),
                     ("LOCALAPPDATA", "home/AppData/Local")]:
        env[key] = str(work / sub)
        Path(env[key]).mkdir(parents=True, exist_ok=True)
    run([installed, "--help"], env=env)
    unavailable = {name: run(["where.exe", name], env=env, check=False).returncode != 0
                   for name in ("cargo", "rustc", "gcc", "node", "python", "docker")}
    assert all(unavailable.values()), unavailable

    attempts = []
    answer = {"id": "synthetic", "choices": [{"index": 0, "message": {"role": "assistant", "content": "synthetic result"},
              "finish_reason": "stop"}], "usage": {"prompt_tokens": 20, "completion_tokens": 3}}

    class Upstream(BaseHTTPRequestHandler):
        protocol_version = "HTTP/1.1"

        def log_message(self, *_args):
            pass

        def do_POST(self):
            payload = json.loads(self.rfile.read(int(self.headers["Content-Length"])))
            attempts.append({"authorization": self.headers.get("Authorization"),
                             "legacy_header": self.headers.get("X-LLMGW-Token")})
            if payload["stream"]:
                events = [{"choices": [{"index": 0, "delta": {"content": "synthetic result"}, "finish_reason": None}]},
                          {"choices": [{"index": 0, "delta": {}, "finish_reason": "stop"}]},
                          {"choices": [], "usage": answer["usage"]}]
                response = b"".join(b"data: " + json.dumps(v).encode() + b"\n\n" for v in events) + b"data: [DONE]\n\n"
            else:
                response = json.dumps(answer).encode()
            time.sleep(.1)
            self.send_response(200)
            self.send_header("Content-Type", "text/event-stream" if payload["stream"] else "application/json")
            self.send_header("Content-Length", str(len(response)))
            self.end_headers()
            self.wfile.write(response)
            self.wfile.flush()

    upstream = ThreadingHTTPServer(("127.0.0.1", 0), Upstream)
    threading.Thread(target=upstream.serve_forever, daemon=True).start()
    config = work / "gateway config.toml"
    config.write_text(f'''listen = "127.0.0.1:0"
startup_hold_secs = 0
concurrency = 3
[cache]
[upstream]
api_base = "http://127.0.0.1:{upstream.server_port}/v1"
[upstream.auth]
mode = "forward"
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
    cli = [installed, "--config", config]
    idle_sockets = []
    connection = None
    try:
        run(cli + ["on"], env=env)
        status = json.loads(run(cli + ["status", "--json"], env=env).stdout)
        assert status["state"] == "running"
        pid = status["identity"]["pid"]
        host, port = status["identity"]["address"].rsplit(":", 1)
        before = resources(pid, env)
        idle_sockets = [socket.create_connection((host, int(port)), timeout=3) for _ in range(64)]
        idle_start = resources(pid, env)
        time.sleep(2)
        idle_end = resources(pid, env)
        for s in idle_sockets:
            s.close()
        idle_sockets.clear()
        connection = http.client.HTTPConnection(host, int(port), timeout=5)
        samples = {}
        for streaming in (False, True):
            body = json.dumps({"model": "synthetic", "messages": [{"role": "user", "content": "synthetic classifier"}],
                               "stream": streaming, "max_tokens": 64, "temperature": 0}).encode()
            durations = []
            expected = None
            for _ in range(41):
                started = time.perf_counter()
                connection.request("POST", "/r/one/v1/chat/completions", body,
                                   {"Content-Type": "application/json", "Authorization": "Bearer synthetic-windows"})
                response = connection.getresponse()
                received = response.read()
                assert response.status == 200, received
                durations.append((time.perf_counter() - started) * 1000)
                if expected is None:
                    expected = received
                assert received == expected
            hits = sorted(durations[1:])
            samples["sse" if streaming else "json"] = {
                "miss_ms": durations[0], "hit_count": len(hits), "hit_p50_ms": hits[math.ceil(.5 * len(hits)) - 1],
                "hit_p95_ms": hits[math.ceil(.95 * len(hits)) - 1], "hit_p99_ms": hits[math.ceil(.99 * len(hits)) - 1]}
        connection.close()
        connection = None
        status = json.loads(run(cli + ["status", "--json"], env=env).stdout)
        runtime = status["runtime"]
        assert len(attempts) == 2, attempts
        assert all(a == {"authorization": "Bearer synthetic-windows", "legacy_header": None} for a in attempts)
        assert runtime["exact_cache"]["hits"] == 80, runtime["exact_cache"]
        assert runtime["admission"]["accounting"] == "actual"
        assert runtime["admission"]["rpm_debited"] == "2"
        assert runtime["admission"]["tpm_debited"] == "46"
        after = resources(pid, env)
        run(cli + ["off"], env=env)
        run(cli + ["off"], env=env)
        assert json.loads(run(cli + ["status", "--json"], env=env).stdout)["state"] == "stopped"
        assert path_registry() == registry_before
        return {"binary_sha256": sha(binary), "binary_bytes": binary.stat().st_size,
                "install_reinstall_bad_checksum": "passed", "path_registry_unchanged": True,
                "installer_preview": "preview only" in first.stdout, "unavailable_on_runtime_path": unavailable,
                "cache": {"requests": 82, "upstream_attempts": len(attempts), "hits": 80, "failures": 0, "samples": samples},
                "usage": {"rpm_debited": "2", "tpm_debited": "46", "accounting": "actual"},
                "resources": {"before": before, "idle_64_start": idle_start, "idle_64_end": idle_end, "after": after},
                "lifecycle": "on/status/off/idempotent-off passed", "real_provider_called": False,
                "measurement_scope": "synthetic 100ms loopback fixture; cache replay check, not provider or comparative benchmark"}
    finally:
        if connection:
            connection.close()
        for s in idle_sockets:
            s.close()
        run(cli + ["off"], env=env, check=False)
        upstream.shutdown()
        upstream.server_close()


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--binary", type=Path, required=True)
    parser.add_argument("--work", type=Path, required=True, help="New directory, outside OneDrive")
    parser.add_argument("--output", type=Path, required=True)
    args = parser.parse_args()
    if os.name != "nt":
        parser.error("Run this acceptance check on Windows")
    result = verify(args.binary.resolve(), args.work.resolve(), Path(__file__).resolve().parents[1])
    args.output.write_text(json.dumps(result, indent=2) + "\n", encoding="utf-8")
    print(json.dumps(result, indent=2))
