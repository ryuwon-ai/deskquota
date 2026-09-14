#!/usr/bin/env python3
"""Observe an installed Pi CLI against loopback HTTP fixtures, without a gateway.

Uses a temporary PI_CODING_AGENT_DIR and working directory, no real auth, no
tools/extensions/context files, and offline startup. Records no request bodies,
authorization values, or raw CLI event streams. This is not an LLM benchmark.
"""

import argparse
import datetime
import hashlib
import json
import os
import platform
import shutil
import subprocess
import tempfile
import threading
import time
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
CASES = {
    "success-default": {"response": "success", "retry": None, "expected_attempts": 1, "expected_exit": 0},
    "429-default": {"response": "429", "retry": None, "expected_attempts": 4, "expected_exit": 1},
    "429-retry-disabled": {"response": "429", "retry": {"enabled": False}, "expected_attempts": 1, "expected_exit": 1},
    "429-retry-after-recovery": {"response": "429-then-success", "retry": None, "retry_after": "5", "expected_attempts": 2, "expected_exit": 0},
    "429-two-retry-layers": {"response": "429", "retry": {"enabled": True, "maxRetries": 1, "baseDelayMs": 50, "provider": {"maxRetries": 1}}, "retry_after_ms": "25", "expected_attempts": 4, "expected_exit": 1},
    "429-insufficient-quota": {"response": "quota", "retry": None, "expected_attempts": 1, "expected_exit": 1},
    "truncated-stream-recovery": {"response": "truncated-then-success", "retry": None, "expected_attempts": 2, "expected_exit": 0},
    "session-affinity-enabled": {"response": "success", "retry": None, "affinity": True, "expected_attempts": 1, "expected_exit": 0},
}


def run_case(pi, name, case):
    observations = []
    started = time.monotonic()

    class Handler(BaseHTTPRequestHandler):
        protocol_version = "HTTP/1.1"

        def log_message(self, *_args):
            pass

        def do_POST(self):
            length = int(self.headers.get("Content-Length", "0"))
            if length > 1024 * 1024:
                self.send_error(413)
                return
            payload = json.loads(self.rfile.read(length))
            header_names = ["session_id", "x-client-request-id", "x-session-affinity", "x-session-id"]
            observations.append({
                "attempt": len(observations) + 1,
                "at_seconds": round(time.monotonic() - started, 6),
                "method": self.command, "path": self.path,
                "body_bytes": length,
                "model": payload.get("model"), "stream": payload.get("stream"),
                "max_tokens": payload.get("max_tokens"),
                "max_completion_tokens": payload.get("max_completion_tokens"),
                "tool_count": len(payload.get("tools", [])),
                "include_usage": payload.get("stream_options", {}).get("include_usage"),
                "configured_root_header_received": self.headers.get("x-research-root") == "fixture-root",
                "session_header_fingerprints": {
                    key: hashlib.sha256(self.headers[key].encode()).hexdigest()[:16]
                    for key in header_names if self.headers.get(key)
                },
            })
            index = len(observations)
            response = case["response"]
            is_error = response in {"429", "quota"} or response == "429-then-success" and index == 1
            if is_error:
                code = "insufficient_quota" if response == "quota" else "rate_limit_error"
                body = json.dumps({"error": {"message": code + " fixture", "type": code, "code": code}}).encode()
                self.send_response(429)
                self.send_header("Content-Type", "application/json")
                self.send_header("Content-Length", str(len(body)))
                if "retry_after" in case:
                    self.send_header("Retry-After", case["retry_after"])
                if "retry_after_ms" in case:
                    self.send_header("retry-after-ms", case["retry_after_ms"])
                self.end_headers()
                self.wfile.write(body)
                self.wfile.flush()
                return
            truncated = response == "truncated-then-success" and index == 1
            content = "partial-fixture" if truncated else "fixture-ok"
            chunk = {"id": "chatcmpl-fixture", "object": "chat.completion.chunk", "created": 0, "model": "fixture", "choices": [{"index": 0, "delta": {"role": "assistant", "content": content}, "finish_reason": None}]}
            chunks = ["data: " + json.dumps(chunk) + "\n\n"]
            if not truncated:
                chunks.append("data: " + json.dumps({**chunk, "choices": [{"index": 0, "delta": {}, "finish_reason": "stop"}], "usage": {"prompt_tokens": 1, "completion_tokens": 1, "total_tokens": 2}}) + "\n\n")
            chunks.append("data: [DONE]\n\n")
            body = "".join(chunks).encode()
            self.send_response(200)
            self.send_header("Content-Type", "text/event-stream")
            self.send_header("Content-Length", str(len(body)))
            self.end_headers()
            self.wfile.write(body)
            self.wfile.flush()

    server = ThreadingHTTPServer(("127.0.0.1", 0), Handler)
    thread = threading.Thread(target=server.serve_forever, kwargs={"poll_interval": 0.05}, daemon=True)
    thread.start()
    try:
        with tempfile.TemporaryDirectory(prefix="pi-client-probe-") as temp:
            temp_path = Path(temp)
            agent_dir = temp_path / "agent"
            cwd = temp_path / "work"
            agent_dir.mkdir()
            cwd.mkdir()
            settings = {"enableInstallTelemetry": False, "enableAnalytics": False, "compaction": {"enabled": False}, "defaultThinkingLevel": "off"}
            if case["retry"] is not None:
                settings["retry"] = case["retry"]
            provider = {"baseUrl": f"http://127.0.0.1:{server.server_port}/v1", "api": "openai-completions", "apiKey": "fixture-only", "headers": {"x-research-root": "fixture-root"}, "models": [{"id": "fixture", "reasoning": False, "input": ["text"], "contextWindow": 4096, "maxTokens": 16, "cost": {"input": 0, "output": 0, "cacheRead": 0, "cacheWrite": 0}}]}
            if case.get("affinity"):
                provider["compat"] = {"sendSessionAffinityHeaders": True}
            (agent_dir / "models.json").write_text(json.dumps({"providers": {"research-fixture": provider}}))
            (agent_dir / "settings.json").write_text(json.dumps(settings))
            env = {"PATH": os.environ.get("PATH", "/usr/bin:/bin"), "LANG": "en_US.UTF-8", "PI_CODING_AGENT_DIR": str(agent_dir), "PI_OFFLINE": "1", "PI_TELEMETRY": "0", "NO_COLOR": "1"}
            command = [str(pi), "--offline", "--no-approve", "--no-tools", "--no-extensions", "--no-skills", "--no-prompt-templates", "--no-themes", "--no-context-files", "--no-session", "--provider", "research-fixture", "--model", "fixture", "--thinking", "off", "--system-prompt", "Return the fixed fixture response.", "--print", "fixture"]
            run = subprocess.run(command, cwd=cwd, env=env, capture_output=True, text=True, timeout=25)
            result = {
                "case": name, "fixture": case,
                "attempts": len(observations), "exit_code": run.returncode,
                "stdout_is_fixture_ok": run.stdout.strip() == "fixture-ok",
                "stderr_present": bool(run.stderr.strip()),
                "stderr_mentions_quota": "insufficient_quota" in run.stderr,
                "attempt_observations": observations,
                "intervals_seconds": [round(b["at_seconds"] - a["at_seconds"], 6) for a, b in zip(observations, observations[1:])],
                "expected_count_and_exit_met": len(observations) == case["expected_attempts"] and run.returncode == case["expected_exit"],
            }
            # Raw stdout/stderr and request bodies deliberately do not leave this process.
            return result
    finally:
        server.shutdown()
        server.server_close()
        thread.join(timeout=2)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--pi", default=shutil.which("pi"))
    parser.add_argument("--case", choices=list(CASES), action="append")
    parser.add_argument("--output", type=Path, required=True)
    args = parser.parse_args()
    pi = Path(args.pi).resolve()
    package_dir = pi.parent.parent
    package = json.loads((package_dir / "package.json").read_text())
    selected = args.case or list(CASES)
    result = {
        "started_at": datetime.datetime.now(datetime.timezone.utc).isoformat(),
        "scope": "Installed Pi print mode + loopback fake OpenAI Chat Completions; no gateway or LLM",
        "package": {"name": package["name"], "version": package["version"], "entry_path": str(pi)},
        "system": platform.platform(),
        "node_version": subprocess.check_output(["node", "--version"], text=True).strip(),
        "script_sha256": hashlib.sha256(Path(__file__).read_bytes()).hexdigest(),
        "source_sha256": {
            str(path.relative_to(package_dir)): hashlib.sha256(path.read_bytes()).hexdigest()
            for path in [package_dir / "package.json", package_dir / "dist/core/agent-session.js", package_dir / "dist/core/settings-manager.js", package_dir / "node_modules/@earendil-works/pi-ai/dist/api/openai-completions.js", package_dir / "node_modules/@earendil-works/pi-ai/dist/utils/retry.js", package_dir / "node_modules/@earendil-works/pi-ai/dist/utils/provider-retry.js"]
        },
        "cases": [],
    }
    args.output.parent.mkdir(parents=True, exist_ok=True)
    for name in selected:
        row = run_case(pi, name, CASES[name])
        result["cases"].append(row)
        args.output.write_text(json.dumps(result, indent=2) + "\n")
        print(json.dumps({key: row[key] for key in ["case", "attempts", "exit_code", "intervals_seconds", "expected_count_and_exit_met"]}), flush=True)
    result["finished_at"] = datetime.datetime.now(datetime.timezone.utc).isoformat()
    args.output.write_text(json.dumps(result, indent=2) + "\n")
    return 0 if all(row["expected_count_and_exit_met"] for row in result["cases"]) else 1


if __name__ == "__main__":
    raise SystemExit(main())
