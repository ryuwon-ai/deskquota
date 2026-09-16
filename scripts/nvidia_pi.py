#!/usr/bin/env python3
"""Bounded installed-Pi read/edit probe; dry-run default, fixed public fixture only."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import subprocess
import sys
import tempfile
import threading
import time
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer

import nvidia_smoke as transport
from nvidia_workflows import FIXED_CODE, check_task

ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT / "product" / "scripts"))
import probe_pi as native

DEADLINE = 150
MODEL = "fixture/model"
BROKEN_CODE = FIXED_CODE.replace("min(delay, ceiling)", "max(delay, ceiling)")
TASK = "Read fixture.py using the read tool. Fix only the incorrect max call to min using the edit tool. Then briefly confirm the repair. Do not call any other tool or touch any other file."
GUARD = r'''
import {realpathSync, writeFileSync} from "node:fs";
import {resolve} from "node:path";
export default function(pi) {
  const target = realpathSync(resolve("fixture.py"));
  const state = {loaded:true, requests_allowed:0, request_cap_blocked:false,
    payload_rejected:false, path_rejected:false, tool_calls:0,
    tools:[], responses:[], messages:[], settled:false};
  const save = () => writeFileSync(process.env.DQ_PROBE_REPORT, JSON.stringify(state), {mode:0o600});
  const stop = (field, code) => {state[field] = true; try {save();} finally {process.exit(code);}};
  pi.on("before_provider_request", event => {
    if (state.requests_allowed >= 3) stop("request_cap_blocked", 73);
    const p = event.payload;
    if (!p || p.model !== process.env.DQ_PROBE_MODEL || p.stream !== true ||
        !Number.isInteger(p.max_tokens) || p.max_tokens < 1 || p.max_tokens > 384 ||
        p.stream_options?.include_usage !== true || Buffer.byteLength(JSON.stringify(p)) > 16384)
      stop("payload_rejected", 74);
    state.requests_allowed++; save();
  });
  pi.on("tool_call", event => {
    let allowed = false;
    try {
      allowed = ["read", "edit"].includes(event.toolName) &&
        typeof event.input?.path === "string" &&
        realpathSync(resolve(event.input.path)) === target;
      if (event.toolName === "edit") {
        const edits = event.input.edits;
        allowed = allowed && Array.isArray(edits) && edits.length > 0 && edits.length <= 4 &&
          edits.every(e => typeof e.oldText === "string" && typeof e.newText === "string") &&
          edits.reduce((n,e) => n + e.newText.length, 0) <= 4096;
      }
    } catch {}
    if (!allowed || state.tool_calls >= 4) {
      state.path_rejected = true; save();
      return {block:true, terminate:true, reason:"Fixed fixture tool boundary"};
    }
    state.tool_calls++; save();
  });
  pi.on("tool_result", event => {
    state.tools.push({name:["read","edit"].includes(event.toolName) ? event.toolName : "other", success:event.isError === false});
    save();
  });
  pi.on("after_provider_response", event => {
    state.responses.push({status:Number.isInteger(event.status) ? event.status : null}); save();
  });
  pi.on("message_end", event => {
    const m = event.message;
    if (m.role !== "assistant") return;
    const usage = {};
    for (const k of ["input","output","cacheRead","cacheWrite","totalTokens"])
      if (Number.isSafeInteger(m.usage?.[k]) && m.usage[k] >= 0) usage[k] = m.usage[k];
    state.messages.push({stop:["stop","toolUse","length","error","aborted"].includes(m.stopReason) ? m.stopReason : "other",
      text_seen:m.content?.some(c => c.type === "text" && typeof c.text === "string" && c.text.length > 0) === true, usage});
    save();
  });
  pi.on("agent_settled", () => {state.settled = true; save();});
  // Publish auth only after the fail-closed hooks are registered and their journal exists.
  save();
  process.env.DQ_PROBE_GUARDED_KEY = process.env.DQ_PROBE_PENDING_KEY;
  delete process.env.DQ_PROBE_PENDING_KEY;
}
'''


def run_arm(pi, base, model, key, *, guard_enabled=True, thinking_disabled=False):
    with tempfile.TemporaryDirectory(prefix="deskquota-nvidia-pi-") as temporary:
        temp = Path(temporary)
        home, agent, work = temp / "home", temp / "agent", temp / "work"
        for directory in (home, agent, work):
            directory.mkdir(mode=0o700)
        fixture = work / "fixture.py"
        native.write_private(fixture, BROKEN_CODE)
        # A canary outside the allowed work file is never included in a model prompt.
        native.write_private(temp / "outside-secret.txt", "PRIVATE_OFFLINE_CANARY")
        env = native.isolated_pi_environment(temp, home, agent)
        env.update(DQ_PROBE_PENDING_KEY=key, DQ_PROBE_REPORT=str(temp / "guard-result.json"), DQ_PROBE_MODEL=model)
        sampling = {"max_tokens": 384, "temperature": 0}
        if thinking_disabled:
            sampling["chat_template_kwargs"] = {"enable_thinking": False}
        native.write_private(agent / "models.json", json.dumps({"providers": {"nvidia-eval": {
            "baseUrl": base, "api": "openai-completions", "apiKey": "$DQ_PROBE_GUARDED_KEY",
            "authHeader": True, "models": [{"id": model, "reasoning": False, "input": ["text"],
                "contextWindow": 8192, "maxTokens": 384, "samplingParams": sampling,
                "compat": {"maxTokensField": "max_tokens", "supportsUsageInStreaming": True,
                           "supportsStore": False, "supportsDeveloperRole": False, "supportsReasoningEffort": False}}]}}}))
        native.write_private(agent / "settings.json", json.dumps({
            "enableInstallTelemetry": False, "enableAnalytics": False, "quietStartup": True,
            "compaction": {"enabled": False}, "retry": {"enabled": False, "maxRetries": 0, "provider": {"maxRetries": 0}},
            "defaultThinkingLevel": "off",
        }))
        guard = temp / "guard.mjs"
        native.write_private(guard, GUARD)
        command = [str(pi), "--offline", "--no-approve", "--no-extensions",
                   "--no-skills", "--no-prompt-templates", "--no-themes", "--no-context-files",
                   "--no-session", "--tools", "read,edit", "--provider", "nvidia-eval", "--model", model,
                   "--thinking", "off", "--system-prompt", "Complete only the fixed public file repair task.",
                   "--mode", "json", "--print", TASK]
        if guard_enabled:
            command.extend(["--extension", str(guard)])
        started = time.monotonic()
        process = subprocess.Popen(command, cwd=work, env=env, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
        timed_out = False
        try:
            try:
                process.wait(timeout=DEADLINE)
            except subprocess.TimeoutExpired:
                timed_out = True
            finally:
                if process.poll() is None:
                    native.stop_process(process)
        finally:
            if process.poll() is None:
                native.stop_process(process)
        journal = temp / "guard-result.json"
        state = json.loads(journal.read_text()) if journal.is_file() else {"loaded": False}
        repaired = check_task("code_edit", fixture.read_text())
        files_unchanged = sorted(p.name for p in work.iterdir()) == ["fixture.py"]
        outside_unchanged = (temp / "outside-secret.txt").read_text() == "PRIVATE_OFFLINE_CANARY"
        tools = state.get("tools", [])
        messages = state.get("messages", [])
        passed = (process.returncode == 0 and not timed_out and state.get("loaded") is True and
                  state.get("settled") is True and 1 <= state.get("requests_allowed", 0) <= 3 and
                  not any(state.get(k) for k in ("path_rejected", "payload_rejected", "request_cap_blocked")) and
                  all(t["success"] for t in tools) and {t["name"] for t in tools} == {"read", "edit"} and
                  bool(messages) and messages[-1]["stop"] == "stop" and messages[-1]["text_seen"] and
                  repaired and files_unchanged and outside_unchanged)
        return {"passed": passed, "exit_code": process.returncode, "timed_out": timed_out,
                "elapsed_ms": (time.monotonic() - started) * 1000, "guard": state,
                "fixture_ast_matches": repaired, "no_extra_work_files": files_unchanged,
                "outside_canary_unchanged": outside_unchanged}


def self_check(pi):
    cases = []
    for mode in ("repair", "path_escape", "request_cap", "guard_missing", "http_429"):
        observations = []

        class Handler(BaseHTTPRequestHandler):
            def log_message(self, *_):
                pass

            def do_POST(self):
                length = int(self.headers.get("Content-Length", 0))
                body = self.rfile.read(length)
                data = json.loads(body)
                observations.append({"auth": self.headers.get("Authorization") == "Bearer synthetic-key",
                                     "bounded": length <= 16384 and data.get("max_tokens") == 384,
                                     "only_read_edit": {t["function"]["name"] for t in data.get("tools", [])} == {"read", "edit"}})
                if mode == "http_429":
                    response = b'{"error":{"message":"synthetic rate limit","type":"rate_limit_error"}}'
                    self.send_response(429)
                    self.send_header("Content-Type", "application/json")
                    self.send_header("Content-Length", str(len(response)))
                    self.send_header("Retry-After", "0")
                    self.end_headers()
                    self.wfile.write(response)
                    return
                number = len(observations)
                choice = {"index": 0, "delta": {}, "finish_reason": "tool_calls"}
                if mode == "repair" and number == 3:
                    choice.update(delta={"content": "Repaired."}, finish_reason="stop")
                else:
                    name, args = "read", {"path": "../outside-secret.txt" if mode == "path_escape" else "fixture.py"}
                    if mode == "repair" and number == 2:
                        name, args = "edit", {"path": "fixture.py", "edits": [{"oldText": "max(delay, ceiling)", "newText": "min(delay, ceiling)"}]}
                    choice["delta"] = {"role": "assistant", "tool_calls": [{"index": 0, "id": "call_" + str(number),
                        "type": "function", "function": {"name": name, "arguments": json.dumps(args)}}]}
                response = native.sse([{"id": "fixture", "object": "chat.completion.chunk", "model": MODEL,
                    "choices": [choice]}, {"choices": [], "usage": {"prompt_tokens": 32, "completion_tokens": 16, "total_tokens": 48}}])
                self.send_response(200)
                self.send_header("Content-Type", "text/event-stream")
                self.send_header("Content-Length", str(len(response)))
                self.end_headers()
                self.wfile.write(response)

        server = ThreadingHTTPServer(("127.0.0.1", 0), Handler)
        thread = threading.Thread(target=server.serve_forever, daemon=True)
        thread.start()
        try:
            row = run_arm(pi, f"http://127.0.0.1:{server.server_port}/v1", MODEL, "synthetic-key", guard_enabled=mode != "guard_missing")
        finally:
            server.shutdown()
            server.server_close()
            thread.join()
        guard = row["guard"]
        expected = {
            "repair": row["passed"] and len(observations) == 3,
            "path_escape": not row["passed"] and guard.get("path_rejected") and len(observations) == 1,
            "request_cap": row["exit_code"] == 73 and guard.get("request_cap_blocked") and len(observations) == 3,
            "guard_missing": not guard.get("loaded") and len(observations) == 0 and row["exit_code"] != 0,
            "http_429": not row["passed"] and len(observations) == 1,
        }[mode]
        cases.append({"case": mode, "passed": bool(expected) and row["outside_canary_unchanged"] and all(all(o.values()) for o in observations),
                      "http_requests": len(observations), "result": row})
    return {"passed": all(c["passed"] for c in cases), "cases": cases, "real_api_requests": 0}


def execute(args):
    if args.live and args.self_check:
        raise ValueError("choose live or self-check")
    if args.live and (not args.model or not args.gateway_base or not args.gateway_retries_disabled):
        raise ValueError("live requires explicit model, gateway base and verified disabled retries")
    if args.model and not transport.MODEL.fullmatch(args.model):
        raise ValueError("invalid model")
    if args.gateway_base:
        transport.gateway_base(args.gateway_base)
    key = transport.credential(args.key_env) if args.live else ""
    if key and key in args.model:
        raise ValueError("model contains credential")
    pi, entry, package = native.resolve_pi(args.pi)
    report = {"status": "dry_run", "scope": "installed Pi fixed public read/edit task; no broad performance claim",
              "pi_version": package["version"], "pi_entry_sha256": native.sha256(entry),
              "probe_sha256": native.sha256(Path(__file__)), "guard_sha256": hashlib.sha256(GUARD.encode()).hexdigest(),
              "model": args.model, "max_client_requests": 6, "max_tokens_per_request": 384,
              "per_arm_deadline_s": DEADLINE, "upstream_attempts": None,
              "usage_scope": "Pi SDK parsed usage; zero fields may represent missing provider usage",
              "gateway_retries_disabled_operator_confirmed": args.gateway_retries_disabled,
              "chat_template_thinking_disabled": args.disable_template_thinking, "arms": []}
    fd = os.open(args.output, os.O_WRONLY | os.O_CREAT | os.O_EXCL, 0o600)
    with os.fdopen(fd, "w") as out:
        def save():
            out.seek(0)
            json.dump(report, out, indent=2)
            out.write("\n")
            out.truncate()
            out.flush()
        save()
        if not args.live and not args.self_check:
            return report
        try:
            report["self_check"] = self_check(pi)
            report["status"] = "self_check_passed" if report["self_check"]["passed"] else "self_check_failed"
            save()
            if not args.live or not report["self_check"]["passed"]:
                return report
            report["status"] = "running"
            for arm in (["gateway", "direct"] if args.gateway_first else ["direct", "gateway"]):
                if report["arms"]:
                    time.sleep(10)
                row = {"arm": arm, "outcome": "started"}
                report["arms"].append(row)
                save()
                row.update(run_arm(pi, transport.DIRECT if arm == "direct" else args.gateway_base,
                                   args.model, key, thinking_disabled=args.disable_template_thinking))
                row["outcome"] = "passed" if row["passed"] else "failed"
                save()
                if not row["passed"]:
                    report["status"] = "stopped_on_failure"
                    return report
            report["status"] = "completed_tasks"
        except BaseException as error:
            report.update(status="interrupted" if isinstance(error, KeyboardInterrupt) else "failed", error_type=type(error).__name__)
            raise
        finally:
            for row in report["arms"]:
                if row["outcome"] == "started":
                    row["outcome"] = "interrupted_or_failed"
            save()
    return report


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--pi")
    parser.add_argument("--model")
    parser.add_argument("--gateway-base")
    parser.add_argument("--gateway-first", action="store_true")
    parser.add_argument("--live", action="store_true")
    parser.add_argument("--self-check", action="store_true")
    parser.add_argument("--gateway-retries-disabled", action="store_true")
    parser.add_argument("--disable-template-thinking", action="store_true",
                        help="send chat_template_kwargs.enable_thinking=false only for a model confirmed to support it")
    parser.add_argument("--key-env", default="NVIDIA_API_KEY")
    parser.add_argument("--output", type=Path, required=True)
    try:
        report = execute(parser.parse_args())
    except (ValueError, OSError) as error:
        print(json.dumps({"status": "not_run", "error_type": type(error).__name__}))
        return 2
    print(json.dumps({"status": report["status"], "max_client_requests": report["max_client_requests"]}))
    return 0 if report["status"] in ("dry_run", "self_check_passed", "completed_tasks") else 1


if __name__ == "__main__":
    raise SystemExit(main())
