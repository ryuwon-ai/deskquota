#!/usr/bin/env python3
"""Synthetic native auto-compaction audit; no real provider or user settings.

Usage: python3 scripts/probe_compaction.py --binary /path/to/llmgw --output audit.json
Only counters, status, and marker checks enter the artifact, never request bodies.
"""
from __future__ import annotations

import argparse
import base64
import contextlib
import hashlib
import http.client
import json
import os
import platform
import queue
import shutil
import subprocess
import tempfile
import threading
import time
from urllib.parse import urlsplit
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
from pathlib import Path

import probe_pi as p
import verify_clients as v

SUMMARY = "SYNTHETIC_COMPACTION_SUMMARY"
FINAL = "SYNTHETIC_AFTER_COMPACTION_OK"


class Fixture:
    def __init__(self):
        self.rows = []
        self.high_at = 1
        self.summary_at = 2
        outer = self

        class Handler(BaseHTTPRequestHandler):
            protocol_version = "HTTP/1.1"

            def log_message(self, *_args):
                pass

            def handle(self):
                try:
                    super().handle()
                except ConnectionResetError:
                    pass  # Native clients may close keep-alive sockets when exiting.

            def do_POST(self):
                raw = self.rfile.read(int(self.headers.get("Content-Length", "0")))
                body = json.loads(raw)
                n = len(outer.rows) + 1
                observation = {
                    "path": self.path.split("?", 1)[0],
                    "bytes": len(raw),
                    "summary_in_input": SUMMARY in raw.decode(),
                    "anthropic_beta_forwarded": self.headers.get("anthropic-beta") is not None,
                    "anthropic_version_forwarded": self.headers.get("anthropic-version") is not None,
                }
                outer.rows.append(observation)
                text = "SYNTHETIC_FIRST_REPLY" if n < outer.summary_at else SUMMARY if n == outer.summary_at else FINAL
                usage = 120_000 if n == outer.high_at else 10
                path = self.path.split("?", 1)[0]
                mime = "text/event-stream"
                if path.endswith("count_tokens"):
                    mime, response = "application/json", b'{"input_tokens":120000}'
                elif path.endswith("chat/completions"):
                    chunks = [
                        {"choices": [{"index": 0, "delta": {"role": "assistant", "content": text}, "finish_reason": None}]},
                        {"choices": [{"index": 0, "delta": {}, "finish_reason": "stop"}]},
                        {"choices": [], "usage": {"prompt_tokens": usage, "completion_tokens": 10, "total_tokens": usage + 10}},
                    ]
                    response = ("".join("data: " + json.dumps(c) + "\n\n" for c in chunks) + "data: [DONE]\n\n").encode()
                elif path.endswith("responses/compact"):
                    mime = "application/json"
                    response = json.dumps({"object": "response.compaction", "output": [{"type": "compaction", "encrypted_content": "synthetic-opaque"}]}).encode()
                elif path.endswith("responses"):
                    response = v.responses_sse([
                        {"type": "response.created", "response": {"id": f"resp_{n}"}},
                        {"type": "response.output_item.done", "item": {"type": "message", "id": f"msg_{n}", "role": "assistant", "content": [{"type": "output_text", "text": text}]}},
                        {"type": "response.completed", "response": {"id": f"resp_{n}", "usage": {"input_tokens": usage, "output_tokens": 10, "total_tokens": usage + 10}}},
                    ])
                elif path.endswith("messages"):
                    response = v.claude_sse([
                        {"type": "message_start", "message": {"id": f"msg_{n}", "type": "message", "role": "assistant", "content": [], "model": body["model"], "stop_reason": None, "stop_sequence": None, "usage": {"input_tokens": usage, "output_tokens": 0}}},
                        {"type": "content_block_start", "index": 0, "content_block": {"type": "text", "text": ""}},
                        {"type": "content_block_delta", "index": 0, "delta": {"type": "text_delta", "text": text}},
                        {"type": "content_block_stop", "index": 0},
                        {"type": "message_delta", "delta": {"stop_reason": "end_turn", "stop_sequence": None}, "usage": {"output_tokens": 10}},
                        {"type": "message_stop"},
                    ])
                else:
                    self.send_error(404)
                    return
                self.send_response(200)
                self.send_header("Content-Type", mime)
                self.send_header("Content-Length", str(len(response)))
                self.end_headers()
                observation["response_sha256"] = hashlib.sha256(response).hexdigest()
                self.wfile.write(response)

        self.server = ThreadingHTTPServer(("127.0.0.1", 0), Handler)
        self.server.daemon_threads = True
        self.thread = threading.Thread(target=self.server.serve_forever, daemon=True)
        self.thread.start()
        self.base = f"http://127.0.0.1:{self.server.server_port}/team/v1"

    def close(self):
        self.server.shutdown()
        self.server.server_close()
        self.thread.join(timeout=2)


@contextlib.contextmanager
def environment(binary, client, gateway, known=True, input_estimator="utf8_bytes"):
    with tempfile.TemporaryDirectory(prefix="deskquota-compact-") as tmp:
        root = Path(tmp)
        if os.name == "nt":
            # Match the gateway's private-state contract, including on inherited Windows temp ACLs.
            script = "$p='" + str(root).replace("'", "''") + "';$sid=[Security.Principal.WindowsIdentity]::GetCurrent().User;$acl=[Security.AccessControl.DirectorySecurity]::new();$acl.SetOwner($sid);$acl.SetAccessRuleProtection($true,$false);$acl.AddAccessRule([Security.AccessControl.FileSystemAccessRule]::new($sid,'FullControl','ContainerInherit,ObjectInherit','None','Allow'));Set-Acl -LiteralPath $p -AclObject $acl"
            subprocess.run(["powershell.exe", "-NoProfile", "-NonInteractive", "-EncodedCommand", base64.b64encode(script.encode("utf-16le")).decode()], check=True, capture_output=True, timeout=10)
        home, cwd, native = root / "home", root / "work", root / "native"
        for directory in (home, cwd, native):
            directory.mkdir(mode=0o700)
        env = v.isolated_environment(root, home, client, native)
        for name, directory in {"TEMP": root / "tmp", "TMP": root / "tmp", "LOCALAPPDATA": home / "AppData/Local", "APPDATA": home / "AppData/Roaming"}.items():
            directory.mkdir(parents=True, exist_ok=True, mode=0o700)
            env[name] = str(directory)
        if os.name == "nt" and client == "claude":
            bash = Path("C:/Program Files/Git/bin/bash.exe")
            if bash.is_file():
                env["CLAUDE_CODE_GIT_BASH_PATH"] = str(bash)
        if client == "pi":
            env.update({"PI_OFFLINE": "1", "PI_TELEMETRY": "0"})
        fixture = Fixture()
        started = False
        port = p.reserve_loopback_port()
        try:
            if gateway:
                config = root / "config.toml"
                p.write_private(config, f'''listen = "127.0.0.1:{port}"
concurrency = 3
accounting = "actual"
startup_hold_secs = 0
[cache]
ttl_secs = 300
max_history = 3
[upstream]
api_base = "{fixture.base}"
[upstream.auth]
mode = "none"
[quota.rpm]
kind = "known"
value = 18
[quota.tpm]
kind = "{'known' if known else 'unknown'}"
{'value = 450000' if known else ''}
[[models]]
id = "example-model"
max_output_tokens = 16384
input_estimator = "{input_estimator}"
[[roots]]
id = "audit"
endpoints = ["chat/completions", "responses", "messages", "messages/count_tokens"]
models = ["example-model"]
''')
                # Let the native lifecycle create its own tokens and protected Windows ACLs.
                started = True
                subprocess.run([str(binary), "on", "--config", str(config)], env=env, cwd=cwd, capture_output=True, check=True, timeout=15)
            yield root, native, cwd, env, fixture, f"http://127.0.0.1:{port}/r/audit/v1" if gateway else fixture.base
        finally:
            try:
                if started:
                    subprocess.run([str(binary), "off", "--config", str(config)], env=env, cwd=cwd, capture_output=True, check=True, timeout=15)
            finally:
                fixture.close()


def lines(proc, events, ready):
    for line in proc.stdout:
        try:
            value = json.loads(line)
        except ValueError:
            continue
        events.append(value)
        ready.put(value)
    ready.put(None)


def wait_event(ready, predicate, seconds=30):
    end = time.monotonic() + seconds
    while time.monotonic() < end:
        value = ready.get(timeout=max(0.01, end - time.monotonic()))
        if value is None:
            raise RuntimeError("client exited before expected event")
        if predicate(value):
            return value
    raise TimeoutError("expected event not observed")


def pi_case(binary, executable, gateway, input_estimator="utf8_bytes"):
    with environment(binary, "pi", gateway, input_estimator=input_estimator) as (_, native, cwd, env, fixture, base):
        models = json.loads(p.render_models(base))
        models["providers"][p.PROVIDER_NAME]["models"][0].update(contextWindow=128000, maxTokens=16384)
        p.write_private(native / "models.json", json.dumps(models))
        p.write_private(native / "settings.json", json.dumps({"compaction": {"enabled": True, "reserveTokens": 16384, "keepRecentTokens": 1}, "enableAnalytics": False, "enableInstallTelemetry": False}))
        command = [executable, "--offline", "--no-extensions", "--no-skills", "--no-prompt-templates", "--no-themes", "--no-context-files", "--no-tools", "--no-session", "--provider", p.PROVIDER_NAME, "--model", "example-model", "--thinking", "off", "--mode", "rpc"]
        events, ready = [], queue.Queue()
        with tempfile.TemporaryFile() as err:
            proc = subprocess.Popen(command, env=env, cwd=cwd, stdin=subprocess.PIPE, stdout=subprocess.PIPE, stderr=err, text=True, encoding="utf-8")
            reader = threading.Thread(target=lines, args=(proc, events, ready), daemon=True)
            reader.start()
            result = {"client": "pi", "gateway": gateway}
            try:
                proc.stdin.write(json.dumps({"type": "prompt", "message": "Return the first synthetic reply."}) + "\n")
                proc.stdin.flush()
                compact = wait_event(ready, lambda e: e.get("type") == "compaction_end")
                result["auto_compact_completed"] = compact.get("reason") == "threshold" and not compact.get("aborted") and SUMMARY in json.dumps(compact.get("result", {}))
                proc.stdin.write(json.dumps({"type": "prompt", "message": "Continue after the summary."}) + "\n")
                proc.stdin.flush()
                wait_event(ready, lambda e: e.get("type") == "agent_end" and FINAL in json.dumps(e))
                result["continued"] = True
            except (TimeoutError, queue.Empty, RuntimeError) as error:
                result["error"] = type(error).__name__
            finally:
                proc.terminate()
                proc.wait(timeout=10)
                reader.join(timeout=2)
            result["wire"] = fixture.rows
            result["event_types"] = [e.get("type") for e in events if e.get("type") in ("agent_end", "compaction_start", "compaction_end")]
            result["passed"] = bool(result.get("auto_compact_completed") and result.get("continued") and len(fixture.rows) == 3 and fixture.rows[-1]["summary_in_input"])
            return result


def codex_case(binary, executable, gateway, input_estimator="utf8_bytes"):
    with environment(binary, "codex", gateway, input_estimator=input_estimator) as (_, native, cwd, env, fixture, base):
        p.write_private(native / "config.toml", f'''model = "example-model"
model_provider = "llmgw"
model_context_window = 128000
model_auto_compact_token_limit = 100000
approval_policy = "never"
sandbox_mode = "read-only"
[model_providers.llmgw]
name = "llmgw"
base_url = "{base}"
wire_api = "responses"
requires_openai_auth = false
supports_websockets = false
''')
        output = []
        exits = []
        for args in (["exec", "--skip-git-repo-check", "--json", "Return the first synthetic reply."], ["exec", "resume", "--last", "--skip-git-repo-check", "--json", "Continue after the summary."]):
            run = subprocess.run([executable, *args], cwd=cwd, env=env, capture_output=True, timeout=45)
            exits.append(run.returncode)
            output.extend(json.loads(line) for line in run.stdout.splitlines() if line.startswith(b"{"))
        compacted = 0
        for session in native.glob("sessions/**/*.jsonl"):
            for line in session.read_text(encoding="utf-8").splitlines():
                item = json.loads(line)
                compacted += item.get("type") == "compacted"
        continued = FINAL in json.dumps(output)
        return {"client": "codex", "gateway": gateway, "exits": exits, "compacted_entries": compacted, "continued": continued, "wire": fixture.rows, "passed": exits == [0, 0] and compacted > 0 and continued and len(fixture.rows) == 3 and fixture.rows[-1]["summary_in_input"]}


def post(base, endpoint, payload):
    url = urlsplit(base)
    connection = http.client.HTTPConnection(url.hostname, url.port, timeout=10)
    try:
        connection.request("POST", url.path + "/" + endpoint, json.dumps(payload), {"Content-Type": "application/json", "anthropic-beta": "synthetic-compaction", "anthropic-version": "2023-06-01"})
        response = connection.getresponse()
        return response.status, response.read()
    finally:
        connection.close()


def claude_case(binary, executable, gateway, input_estimator="utf8_bytes"):
    with environment(binary, "claude", gateway, input_estimator=input_estimator) as (_, _, cwd, env, fixture, base):
        fixture.high_at, fixture.summary_at = 3, 4
        env.update({
            "ANTHROPIC_BASE_URL": base.removesuffix("/v1"),
            "ANTHROPIC_API_KEY": "synthetic-not-a-real-key",
            "CLAUDE_AUTOCOMPACT_PCT_OVERRIDE": "25",
            "CLAUDE_CODE_AUTO_COMPACT_WINDOW": "128000",
            "CLAUDE_CODE_DISABLE_NONESSENTIAL_TRAFFIC": "1",
            "DISABLE_TELEMETRY": "1", "DISABLE_ERROR_REPORTING": "1",
            "DISABLE_AUTOUPDATER": "1", "CLAUDE_CODE_DISABLE_FEEDBACK_SURVEY": "1",
        })
        command = [executable, "-p", "--input-format", "stream-json", "--output-format", "stream-json", "--verbose", "--no-session-persistence", "--no-chrome", "--disable-slash-commands", "--strict-mcp-config", "--mcp-config", '{"mcpServers":{}}', "--permission-mode", "dontAsk", "--tools", "", "--setting-sources", "user", "--model", "example-model"]
        events, ready = [], queue.Queue()
        with tempfile.TemporaryFile() as err:
            proc = subprocess.Popen(command, env=env, cwd=cwd, stdin=subprocess.PIPE, stdout=subprocess.PIPE, stderr=err, text=True, encoding="utf-8")
            reader = threading.Thread(target=lines, args=(proc, events, ready), daemon=True)
            reader.start()
            result = {"client": "claude", "gateway": gateway}
            try:
                for text in ("First synthetic turn.", "Second synthetic turn.", "Third synthetic turn.", "Continue after the summary."):
                    proc.stdin.write(json.dumps({"type": "user", "message": {"role": "user", "content": text}}) + "\n")
                    proc.stdin.flush()
                    wait_event(ready, lambda e: e.get("type") == "result")
                result["compact_boundary_auto"] = any(e.get("type") == "system" and e.get("subtype") == "compact_boundary" and e.get("compact_metadata", {}).get("trigger") == "auto" for e in events)
                result["continued"] = any(e.get("type") == "result" and e.get("result") == FINAL and not e.get("is_error") for e in events)
            except (TimeoutError, queue.Empty, RuntimeError) as error:
                result["error"] = type(error).__name__
            finally:
                proc.terminate()
                proc.wait(timeout=10)
                reader.join(timeout=2)
            result["event_types"] = [{"type": e.get("type"), "subtype": e.get("subtype")} for e in events]
            result["wire"] = fixture.rows
            result["passed"] = bool(result.get("compact_boundary_auto") and result.get("continued") and fixture.rows[-1]["summary_in_input"])
            return result


def protocol_case(binary, known, input_estimator="utf8_bytes"):
    with environment(binary, "codex", True, known, input_estimator) as (_, _, _, _, fixture, base):
        messages = {"model": "example-model", "messages": [{"role": "user", "content": "synthetic"}], "max_tokens": 16, "stream": True}
        cases = [
            ("count_tokens", "messages/count_tokens", messages, 200),
            ("anthropic_context_management", "messages", dict(messages, context_management={"edits": [{"type": "clear_tool_uses_20250919"}]}), 200),
            ("responses_context_management", "responses", {"model": "example-model", "input": "synthetic", "context_management": [{"type": "compaction", "compact_threshold": 100000}]}, 200),
            ("opaque_compaction_input", "responses", {"model": "example-model", "input": [{"type": "compaction", "encrypted_content": "synthetic-opaque"}]}, 400 if known else 200),
            ("compact_endpoint", "responses/compact", {"model": "example-model", "input": "synthetic"}, 404),
            ("large_text_summary", "messages", dict(messages, messages=[{"role": "user", "content": "word " * 92000}]), 400 if known and input_estimator == "utf8_bytes" else 200),
        ]
        results = []
        for name, endpoint, payload, expected in cases:
            before = len(fixture.rows)
            status, raw = post(base, endpoint, payload)
            row = {"name": name, "status": status, "expected_current_status": expected, "upstream_requests": len(fixture.rows) - before}
            if status == 200:
                row["response_bytes_unchanged"] = hashlib.sha256(raw).hexdigest() == fixture.rows[-1]["response_sha256"]
            else:
                row["error_code"] = json.loads(raw).get("error", {}).get("code")
                control_status, _ = post(fixture.base, endpoint, payload)
                row["direct_control_status"] = control_status
            row["current_behavior_confirmed"] = status == expected and (row.get("response_bytes_unchanged") if status == 200 else row["upstream_requests"] == 0 and row["direct_control_status"] == 200)
            results.append(row)
        return {"client": "protocol", "known_tpm": known, "cases": results, "passed": all(row["current_behavior_confirmed"] for row in results)}


def cache_case(binary, input_estimator="utf8_bytes"):
    with environment(binary, "pi", True, input_estimator=input_estimator) as (_, _, _, _, fixture, base):
        payload = {"model": "example-model", "messages": [{"role": "user", "content": "synthetic"}], "max_tokens": 16, "stream": True, "stream_options": {"include_usage": True}}
        first_status, first = post(base, "chat/completions", payload)
        second_status, second = post(base, "chat/completions", payload)
        usage_preserved = b'"prompt_tokens": 120000' in second
        return {"client": "exact_cache", "statuses": [first_status, second_status], "upstream_requests": len(fixture.rows), "response_bytes_unchanged": first == second, "context_usage_preserved": usage_preserved, "passed": first_status == second_status == 200 and len(fixture.rows) == 1 and first == second and usage_preserved}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--binary", type=Path, required=True)
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument("--clients", nargs="+", choices=["pi", "codex", "claude"], default=["pi", "codex", "claude"])
    parser.add_argument("--input-estimator", choices=["utf8_bytes", "cl100k_base", "o200k_base"], default="utf8_bytes")
    args = parser.parse_args()
    report = {"started_at": v.now(), "platform": platform.platform(), "python": platform.python_version(), "probe_sha256": p.sha256(Path(__file__)), "binary_sha256": p.sha256(args.binary), "synthetic_only": True, "gateway_settings": {"rpm": 18, "tpm": 450000, "concurrency": 3, "accounting": "actual", "startup_hold_secs": 0, "exact_cache": True}, "cases": [], "known_limitations": ["responses/compact returns 404", "known TPM rejects opaque compaction items", "known TPM rejects byte estimates above the configured budget"]}
    report["gateway_settings"]["input_estimator"] = args.input_estimator
    report["known_limitations"][-1] = "known TPM rejects configured input plus output estimates above the budget"
    for client, case in (("pi", pi_case), ("codex", codex_case), ("claude", claude_case)):
        if client not in args.clients:
            continue
        executable = shutil.which(client)
        if not executable:
            report["cases"].append({"client": client, "error": "executable_not_found", "passed": False})
            continue
        report[client + "_version"] = v.installed_version(client, Path(executable))[0]
        for gateway in (False, True):
            try:
                result = case(args.binary, executable, gateway, args.input_estimator)
            except Exception as error:
                result = {"client": client, "gateway": gateway, "error": type(error).__name__, "passed": False}
            report["cases"].append(result)
            v.write(args.output, report)
            print(json.dumps(result), flush=True)
    for known in (True, False):
        result = protocol_case(args.binary, known, args.input_estimator)
        report["cases"].append(result)
        print(json.dumps(result), flush=True)
    result = cache_case(args.binary, args.input_estimator)
    report["cases"].append(result)
    print(json.dumps(result), flush=True)
    report["finished_at"] = v.now()
    v.write(args.output, report)
    return 0 if all(case["passed"] for case in report["cases"]) else 1


if __name__ == "__main__":
    raise SystemExit(main())
