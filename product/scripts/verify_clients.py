#!/usr/bin/env python3
"""Verify installed native clients through reviewed llmgw profiles.

Every child runs with an allowlisted environment and isolated native config.
Runtime fields become verified only after the installed client exercises the
gateway wire fixture; parsing or a successful CLI exit is recorded separately.
"""

from __future__ import annotations

import argparse
import datetime as dt
import json
import os
import re
import shlex
import subprocess
import tempfile
import threading
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
from pathlib import Path
from typing import Any

import probe_pi


ROOT = Path(__file__).resolve().parents[1]
CODEX_COMPLETION_MARKER = "TASK4_CODEX_COMPLETION_OK"
CODEX_TOOL_MARKER = "TASK4_CODEX_TOOL_OK"
CODEX_TOOL_CONTENT = "TASK4_CODEX_SYNTHETIC_READ_RESULT"
CODEX_TOOL_CALL_ID = "call_task4_codex_read"
CLAUDE_TOOL_MARKER = "TASK4_CLAUDE_TOOL_OK"


def now() -> str:
    return dt.datetime.now(dt.timezone.utc).isoformat()


def write(path: Path, value: dict[str, Any]) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(json.dumps(value, indent=2, sort_keys=True) + "\n", encoding="utf-8")


def isolated_environment(
    root: Path, home: Path, client: str, native_config: Path
) -> dict[str, str]:
    """Return a small child environment with no inherited auth/helper config."""
    tmp = root / "tmp"
    xdg_config = root / "xdg-config"
    xdg_cache = root / "xdg-cache"
    xdg_data = root / "xdg-data"
    for directory in (tmp, xdg_config, xdg_cache, xdg_data):
        directory.mkdir(parents=True, exist_ok=True, mode=0o700)
    environment = {
        "PATH": os.environ.get("PATH", "/opt/homebrew/bin:/usr/local/bin:/usr/bin:/bin"),
        "LANG": "en_US.UTF-8",
        "LC_ALL": "en_US.UTF-8",
        "HOME": str(home),
        "USERPROFILE": str(home),
        "TMPDIR": str(tmp),
        "XDG_CONFIG_HOME": str(xdg_config),
        "XDG_CACHE_HOME": str(xdg_cache),
        "XDG_DATA_HOME": str(xdg_data),
        "NO_COLOR": "1",
        "DO_NOT_TRACK": "1",
        "OTEL_SDK_DISABLED": "true",
    }
    for name in ("SystemRoot", "WINDIR", "ComSpec", "PATHEXT"):
        if name in os.environ:
            environment[name] = os.environ[name]
    environment[
        {"pi": "PI_CODING_AGENT_DIR", "claude": "CLAUDE_CONFIG_DIR", "codex": "CODEX_HOME"}[
            client
        ]
    ] = str(native_config)
    return environment


def responses_sse(events: list[dict[str, Any]]) -> bytes:
    return "".join(
        f"event: {event['type']}\ndata: {json.dumps(event, separators=(',', ':'))}\n\n"
        for event in events
    ).encode()


def codex_tool_result_matches(payload: dict[str, Any]) -> bool:
    for item in payload.get("input", []):
        if not isinstance(item, dict):
            continue
        if item.get("type") != "function_call_output" or item.get("call_id") != CODEX_TOOL_CALL_ID:
            continue
        output = item.get("output")
        if not isinstance(output, str) or "Process exited with code 0" not in output:
            continue
        return bool(output.splitlines()) and output.splitlines()[-1].strip() == CODEX_TOOL_CONTENT
    return False


def claude_tool_result_matches(payload: dict[str, Any]) -> bool:
    for message in payload.get("messages", []):
        if not isinstance(message, dict) or not isinstance(message.get("content"), list):
            continue
        for block in message["content"]:
            if (
                not isinstance(block, dict)
                or block.get("type") != "tool_result"
                or block.get("tool_use_id") != "toolu_task4_read"
                or block.get("is_error") is True
            ):
                continue
            content = block.get("content")
            texts = [content] if isinstance(content, str) else [
                part.get("text", "")
                for part in content or []
                if isinstance(part, dict) and part.get("type") == "text"
            ]
            for text_value in texts:
                for line in text_value.splitlines():
                    normalized = re.sub(r"^\s*\d+[→\t]\s?", "", line).strip()
                    if normalized == CODEX_TOOL_CONTENT:
                        return True
    return False


def json_lines_contain_tool_call(stdout: bytes, name: str, argument: str) -> bool:
    def visit(value: Any) -> bool:
        if isinstance(value, dict):
            if value.get("name") == name:
                tool_input = value.get("input") or value.get("arguments")
                if isinstance(tool_input, dict) and argument in tool_input.values():
                    return True
                if isinstance(tool_input, str) and argument in tool_input:
                    return True
            if (
                name == "exec_command"
                and value.get("type") == "command_execution"
                and isinstance(value.get("command"), str)
                and argument in value["command"]
            ):
                return True
            return any(visit(child) for child in value.values())
        if isinstance(value, list):
            return any(visit(child) for child in value)
        return False

    for line in stdout.splitlines():
        try:
            value = json.loads(line)
        except (UnicodeDecodeError, json.JSONDecodeError):
            continue
        if visit(value):
            return True
    return False


class CodexFixture:
    def __init__(self, tool_path: Path):
        self.tool_path = tool_path
        self.observations: list[dict[str, Any]] = []
        outer = self

        class Handler(BaseHTTPRequestHandler):
            protocol_version = "HTTP/1.1"

            def log_message(self, *_args: Any) -> None:
                pass

            def do_POST(self) -> None:
                length = int(self.headers.get("Content-Length", "0"))
                body = self.rfile.read(length)
                try:
                    payload = json.loads(body)
                except (UnicodeDecodeError, json.JSONDecodeError):
                    self.send_error(400)
                    return
                attempt = len(outer.observations) + 1
                observation = {
                    "attempt": attempt,
                    "path_matches": self.path.split("?", 1)[0] == "/team/v1/responses",
                    "model_matches": payload.get("model") == "example-model",
                    "stream_enabled": payload.get("stream") is True,
                    "local_data_token_absent": self.headers.get("x-llmgw-token") is None,
                    "authorization_absent": self.headers.get("authorization") is None,
                    "tool_output_matches": codex_tool_result_matches(payload),
                }
                outer.observations.append(observation)
                if attempt == 1:
                    arguments = json.dumps(
                        {"cmd": f"cat {shlex.quote(str(outer.tool_path))}"}, separators=(",", ":")
                    )
                    events = [
                        {"type": "response.created", "response": {"id": "resp-task4-1"}},
                        {
                            "type": "response.output_item.done",
                            "item": {
                                "type": "function_call",
                                "call_id": CODEX_TOOL_CALL_ID,
                                "name": "exec_command",
                                "arguments": arguments,
                            },
                        },
                        {
                            "type": "response.completed",
                            "response": {
                                "id": "resp-task4-1",
                                "usage": {
                                    "input_tokens": 1,
                                    "input_tokens_details": None,
                                    "output_tokens": 1,
                                    "output_tokens_details": None,
                                    "total_tokens": 2,
                                },
                            },
                        },
                    ]
                else:
                    events = [
                        {"type": "response.created", "response": {"id": "resp-task4-2"}},
                        {
                            "type": "response.output_item.done",
                            "item": {
                                "type": "message",
                                "role": "assistant",
                                "id": "msg-task4-2",
                                "content": [{"type": "output_text", "text": CODEX_TOOL_MARKER}],
                            },
                        },
                        {
                            "type": "response.completed",
                            "response": {
                                "id": "resp-task4-2",
                                "usage": {
                                    "input_tokens": 1,
                                    "input_tokens_details": None,
                                    "output_tokens": 1,
                                    "output_tokens_details": None,
                                    "total_tokens": 2,
                                },
                            },
                        },
                    ]
                response = responses_sse(events)
                self.send_response(200)
                self.send_header("Content-Type", "text/event-stream")
                self.send_header("Content-Length", str(len(response)))
                self.end_headers()
                self.wfile.write(response)

        self.server = ThreadingHTTPServer(("127.0.0.1", 0), Handler)
        self.server.daemon_threads = True
        self.thread = threading.Thread(target=self.server.serve_forever, daemon=True)
        self.thread.start()

    @property
    def port(self) -> int:
        return int(self.server.server_address[1])

    def close(self) -> None:
        self.server.shutdown()
        self.server.server_close()
        self.thread.join(timeout=2)


def claude_sse(events: list[dict[str, Any]]) -> bytes:
    return "".join(
        f"event: {event['type']}\ndata: {json.dumps(event, separators=(',', ':'))}\n\n"
        for event in events
    ).encode()


class ClaudeFixture:
    def __init__(self, tool_path: Path):
        self.tool_path = tool_path
        self.observations: list[dict[str, Any]] = []
        outer = self

        class Handler(BaseHTTPRequestHandler):
            protocol_version = "HTTP/1.1"

            def log_message(self, *_args: Any) -> None:
                pass

            def do_POST(self) -> None:
                length = int(self.headers.get("Content-Length", "0"))
                body = self.rfile.read(length)
                try:
                    payload = json.loads(body)
                except (UnicodeDecodeError, json.JSONDecodeError):
                    self.send_error(400)
                    return
                attempt = len(outer.observations) + 1
                observation = {
                    "attempt": attempt,
                    "path_matches": self.path.split("?", 1)[0] == "/team/v1/messages",
                    "model_matches": payload.get("model") == "example-model",
                    "stream_enabled": payload.get("stream") is True,
                    "local_data_token_absent": self.headers.get("x-llmgw-token") is None,
                    "x_api_key_absent": self.headers.get("x-api-key") is None,
                    "authorization_absent": self.headers.get("authorization") is None,
                    "tool_result_matches": claude_tool_result_matches(payload),
                }
                outer.observations.append(observation)
                start = {
                    "type": "message_start",
                    "message": {
                        "id": f"msg-task4-{attempt}",
                        "type": "message",
                        "role": "assistant",
                        "content": [],
                        "model": "example-model",
                        "stop_reason": None,
                        "stop_sequence": None,
                        "usage": {"input_tokens": 1, "output_tokens": 0},
                    },
                }
                if attempt == 1:
                    events = [
                        start,
                        {
                            "type": "content_block_start",
                            "index": 0,
                            "content_block": {
                                "type": "tool_use",
                                "id": "toolu_task4_read",
                                "name": "Read",
                                "input": {},
                            },
                        },
                        {
                            "type": "content_block_delta",
                            "index": 0,
                            "delta": {
                                "type": "input_json_delta",
                                "partial_json": json.dumps({"file_path": str(outer.tool_path)}),
                            },
                        },
                        {"type": "content_block_stop", "index": 0},
                        {
                            "type": "message_delta",
                            "delta": {"stop_reason": "tool_use", "stop_sequence": None},
                            "usage": {"output_tokens": 1},
                        },
                        {"type": "message_stop"},
                    ]
                else:
                    events = [
                        start,
                        {
                            "type": "content_block_start",
                            "index": 0,
                            "content_block": {"type": "text", "text": ""},
                        },
                        {
                            "type": "content_block_delta",
                            "index": 0,
                            "delta": {"type": "text_delta", "text": "TASK4_CLAUDE_TOOL_OK"},
                        },
                        {"type": "content_block_stop", "index": 0},
                        {
                            "type": "message_delta",
                            "delta": {"stop_reason": "end_turn", "stop_sequence": None},
                            "usage": {"output_tokens": 1},
                        },
                        {"type": "message_stop"},
                    ]
                response = claude_sse(events)
                self.send_response(200)
                self.send_header("Content-Type", "text/event-stream")
                self.send_header("Content-Length", str(len(response)))
                self.end_headers()
                self.wfile.write(response)

        self.server = ThreadingHTTPServer(("127.0.0.1", 0), Handler)
        self.server.daemon_threads = True
        self.thread = threading.Thread(target=self.server.serve_forever, daemon=True)
        self.thread.start()

    @property
    def port(self) -> int:
        return int(self.server.server_address[1])

    def close(self) -> None:
        self.server.shutdown()
        self.server.server_close()
        self.thread.join(timeout=2)


def installed_version(client: str, executable: Path) -> tuple[str | None, str | None]:
    try:
        with tempfile.TemporaryDirectory(prefix=f"llmgw-{client}-version-") as temporary:
            root = Path(temporary)
            home = root / "home"
            cwd = root / "empty-work"
            home.mkdir(mode=0o700)
            cwd.mkdir(mode=0o700)
            native = {
                "pi": home / ".pi" / "agent",
                "claude": home / ".claude",
                "codex": home / ".codex",
            }[client]
            native.mkdir(parents=True, mode=0o700)
            environment = isolated_environment(root, home, client, native)
            completed = subprocess.run(
                [str(executable), "--version"],
                cwd=cwd,
                env=environment,
                capture_output=True,
                text=True,
                timeout=10,
                check=False,
            )
    except (OSError, subprocess.TimeoutExpired) as error:
        return None, str(error)
    if completed.returncode != 0:
        return None, f"--version exited {completed.returncode}"
    words = completed.stdout.split()
    version = next((word.strip("()") for word in words if word[:1].isdigit()), None)
    return version, None if version else "--version output contained no version"


def verify_pi(binary: Path, output: Path, executable: str | None) -> int:
    pi, entry, package = probe_pi.resolve_pi(executable)
    result: dict[str, Any] = {
        "schema_version": 1,
        "client": "pi",
        "started_at": now(),
        "binary": str(binary),
        "binary_sha256": probe_pi.sha256(binary),
        "installed_executable": str(pi),
        "installed_entry": str(entry),
        "installed_version": package["version"],
        "override_scope": "temporary HOME with PI_CODING_AGENT_DIR under it",
        "protocol": "OpenAI Chat Completions SSE",
        "root": probe_pi.ROOT_ID,
        "model": probe_pi.MODEL_ID,
        "listing": {"status": "pending"},
        "selection": {"status": "pending"},
        "inference": {"status": "pending"},
        "tools": {"status": "pending"},
        "gateway_off": {"status": "pending"},
        "passed": False,
    }
    write(output, result)
    completion = probe_pi.run_flow(pi, binary, "completion", False, native_connect=True)
    tool = probe_pi.run_flow(pi, binary, "read_tool", False, native_connect=True)
    negative = probe_pi.run_flow(pi, binary, "completion", True, native_connect=True)
    adapter_ok = all(
        case["adapter"][key]
        for case in (completion, tool, negative)
        for key in (
            "preview_succeeded",
            "apply_succeeded",
            "connected_model_listed",
            "disconnect_succeeded",
            "actual_reload_succeeded",
            "unrelated_user_addition_preserved",
            "managed_provider_removed",
        )
    )
    negative_ok = (
        negative["upstream_attempts"] == 0
        and negative["assistant_failure_observed"]
        and not negative["output_marker_observed"]
    )
    result.update(
        {
            "listing": {
                "status": "verified" if adapter_ok else "failed",
                "connected_model_listed": all(
                    case["adapter"]["connected_model_listed"]
                    for case in (completion, tool, negative)
                ),
                "installed_client_reload_after_disconnect": adapter_ok,
            },
            "selection": {
                "status": "verified" if completion["gateway_root_route_exercised"] else "failed",
                "selected_model": probe_pi.MODEL_ID,
            },
            "inference": {
                "status": "verified" if completion["passed"] else "failed",
                "upstream_attempts": completion["upstream_attempts"],
            },
            "tools": {
                "status": "verified" if tool["passed"] else "failed",
                "exact_read_call": tool["tool_events"]["exact_read_arguments"],
                "exact_result": tool["tool_events"]["result_contains_exact_synthetic_content"],
                "subsequent_model_request": tool["upstream_attempts"] == 2,
            },
            "wire": {
                "local_header_accepted": all(
                    observation["custom_header_received"]
                    for case in (completion, tool)
                    for observation in case["wire_observations"]
                ),
                "local_token_stripped_upstream": all(
                    observation["local_data_token_absent"]
                    for case in (completion, tool)
                    for observation in case["wire_observations"]
                ),
                "authorization_stripped_upstream": all(
                    observation["authorization_absent"]
                    for case in (completion, tool)
                    for observation in case["wire_observations"]
                ),
                "path": f"/r/{probe_pi.ROOT_ID}/v1/chat/completions -> {probe_pi.UPSTREAM_PATH}",
            },
            "disconnect": {
                "status": "verified" if adapter_ok else "failed",
                "unrelated_user_addition_preserved": adapter_ok,
                "actual_client_reload": adapter_ok,
            },
            "gateway_off": {
                "status": "expected_failure_verified" if negative_ok else "failed",
                "upstream_attempts": negative["upstream_attempts"],
                "counted_as_positive": False,
            },
            "cleanup": {
                "completion_gateway": completion["gateway_cleanup"],
                "tool_gateway": tool["gateway_cleanup"],
                "negative_gateway": negative["gateway_cleanup"],
                "temporary_directories": "TemporaryDirectory reaped",
            },
        }
    )
    result["passed"] = completion["passed"] and tool["passed"] and negative_ok and adapter_ok
    result["finished_at"] = now()
    write(output, result)
    return 0 if result["passed"] else 1


def verify_claude(binary: Path, output: Path, executable: Path) -> int:
    version, version_error = installed_version("claude", executable)
    result: dict[str, Any] = {
        "schema_version": 1,
        "client": "claude",
        "started_at": now(),
        "binary": str(binary),
        "binary_sha256": probe_pi.sha256(binary),
        "installed_executable": str(executable),
        "installed_version": version,
        "version_error": version_error,
        "override_scope": "temporary HOME, USERPROFILE, XDG paths, CLAUDE_CONFIG_DIR, and cwd",
        "protocol": "Anthropic Messages SSE",
        "root": "claude-work",
        "model": "example-model",
        "listing": {"status": "not_requested"},
        "selection": {"status": "pending"},
        "inference": {"status": "pending"},
        "tools": {"status": "pending"},
        "gateway_off": {"status": "pending"},
        "disconnect": {"status": "pending"},
        "passed": False,
    }
    write(output, result)
    if version != "2.1.63":
        reason = "installed version does not match the supported Claude Code 2.1.63 profile"
        for field in ("selection", "inference", "tools"):
            result[field] = {"status": "unsupported", "reason": reason}
        result["gateway_off"] = {"status": "not_run"}
        result["disconnect"] = {"status": "not_run"}
        result["finished_at"] = now()
        write(output, result)
        return 2

    cleanup = "not_started"
    with tempfile.TemporaryDirectory(prefix="llmgw-claude-wire-") as temporary:
        root = Path(temporary)
        home = root / "client-home"
        claude_dir = home / ".claude"
        gateway_dir = root / "gateway"
        cwd = root / "empty-work"
        for directory in (claude_dir, gateway_dir, cwd):
            directory.mkdir(parents=True, mode=0o700)
        environment = isolated_environment(root, home, "claude", claude_dir)
        environment.update(
            {
                "CLAUDE_CODE_DISABLE_NONESSENTIAL_TRAFFIC": "1",
                "DISABLE_TELEMETRY": "1",
                "DISABLE_ERROR_REPORTING": "1",
                "DISABLE_AUTOUPDATER": "1",
                "CLAUDE_CODE_DISABLE_FEEDBACK_SURVEY": "1",
                "CLAUDE_CODE_DISABLE_BUG_COMMAND": "1",
            }
        )
        settings = claude_dir / "settings.json"
        initial = {"env": {"KEEP": "yes"}, "permissions": {"allow": []}}
        probe_pi.write_private(settings, json.dumps(initial, separators=(",", ":")))
        tool_path = cwd / "synthetic-read.txt"
        probe_pi.write_private(tool_path, CODEX_TOOL_CONTENT)
        fixture = ClaudeFixture(tool_path)
        gateway_port = probe_pi.reserve_loopback_port()
        config = gateway_dir / "config.toml"
        probe_pi.write_private(
            config,
            f'''listen = "127.0.0.1:{gateway_port}"
concurrency = 1
cancel_policy = "drain"
accounting = "reserved"

[upstream]
api_base = "http://127.0.0.1:{fixture.port}/team/v1"

[upstream.auth]
mode = "none"

[quota.rpm]
kind = "unlimited"

[quota.tpm]
kind = "unknown"

[[models]]
id = "example-model"

[[roots]]
id = "claude-work"
endpoints = ["messages"]
models = ["example-model"]
''',
        )
        state_dir = Path(
            json.loads(
                subprocess.check_output(
                    [str(binary), "doctor", "--config", str(config), "--json"],
                    cwd=cwd,
                    env=environment,
                    timeout=10,
                )
            )["state_directory"]
        )
        state_dir.mkdir(mode=0o700)

        control_token = "task4-claude-control-" + os.urandom(18).hex()

        probe_pi.write_private(state_dir / "control-token", control_token)
        gateway = subprocess.Popen(
            [str(binary), "run", "--config", str(config)],
            cwd=cwd,
            env=environment,
            stdout=subprocess.DEVNULL,
            stderr=subprocess.DEVNULL,
        )
        try:
            probe_pi.wait_for_gateway(gateway, gateway_port, control_token)
            connect = [
                str(binary),
                "--config",
                str(config),
                "connect",
                "claude",
                "--client-executable",
                str(executable),
                "--client-home",
                str(home),
                "--root",
                "claude-work",
                "--model",
                "example-model",
            ]
            preview = subprocess.run(
                connect, cwd=cwd, env=environment, capture_output=True, text=True, timeout=10
            )
            reviewed_hash = next(
                (
                    line.removeprefix("preview hash: ")
                    for line in preview.stdout.splitlines()
                    if line.startswith("preview hash: ")
                ),
                None,
            )
            preview_ok = (
                preview.returncode == 0
                and reviewed_hash is not None


            )
            applied = (
                subprocess.run(
                    [*connect, "--apply-hash", str(reviewed_hash)],
                    cwd=cwd,
                    env=environment,
                    capture_output=True,
                    text=True,
                    timeout=10,
                )
                if preview_ok
                else None
            )
            command = [
                str(executable),
                "-p",
                "--output-format",
                "stream-json",
                "--verbose",
                "--no-session-persistence",
                "--no-chrome",
                "--disable-slash-commands",
                "--strict-mcp-config",
                "--mcp-config",
                '{"mcpServers":{}}',
                "--permission-mode",
                "dontAsk",
                "--tools",
                "Read",
                "--setting-sources",
                "user",
                "--model",
                "example-model",
                f"Read only the exact synthetic file {tool_path} and report the fixture result.",
            ]
            header_only_environment = environment.copy()
            header_only_environment.pop("ANTHROPIC_API_KEY", None)
            try:
                header_only = subprocess.run(
                    command,
                    cwd=cwd,
                    env=header_only_environment,
                    capture_output=True,
                    timeout=20,
                )
                header_only_timed_out = False
                header_only_exit = header_only.returncode
                header_only_ok = (
                    header_only.returncode == 0
                    and CLAUDE_TOOL_MARKER.encode() in header_only.stdout
                    and len(fixture.observations) == 2
                    and fixture.observations[1]["tool_result_matches"]
                )
            except subprocess.TimeoutExpired:
                header_only_timed_out = True
                header_only_exit = None
                header_only_ok = False
            header_only_calls = len(fixture.observations)
            fixture.observations.clear()
            active_environment = header_only_environment if header_only_ok else environment
            inference = subprocess.run(
                command, cwd=cwd, env=active_environment, capture_output=True, timeout=20
            )
            observed_read = json_lines_contain_tool_call(
                inference.stdout, "Read", str(tool_path)
            )
            tool_ok = (
                inference.returncode == 0
                and CLAUDE_TOOL_MARKER.encode() in inference.stdout
                and len(fixture.observations) == 2
                and fixture.observations[1]["tool_result_matches"]
                and observed_read
            )
            wire_ok = len(fixture.observations) == 2 and all(
                observation[key]
                for observation in fixture.observations
                for key in (
                    "path_matches",
                    "model_matches",
                    "stream_enabled",
                    "local_data_token_absent",
                    "x_api_key_absent",
                    "authorization_absent",
                )
            )
            cleanup = probe_pi.stop_gateway(gateway, gateway_port, control_token)
            attempts_before = len(fixture.observations)
            try:
                negative = subprocess.run(
                    command, cwd=cwd, env=active_environment, capture_output=True, timeout=8
                )
                negative_failed = negative.returncode != 0
                negative_timed_out = False
            except subprocess.TimeoutExpired:
                negative_failed = True
                negative_timed_out = True
            negative_ok = negative_failed and len(fixture.observations) == attempts_before
            current = json.loads(settings.read_text(encoding="utf-8"))
            current["userAddedAfterConnect"] = True
            probe_pi.write_private(settings, json.dumps(current, indent=2))
            disconnected = subprocess.run(
                [str(binary), "--config", str(config), "disconnect", "claude"],
                cwd=cwd,
                env=environment,
                capture_output=True,
                text=True,
                timeout=10,
            )
            restored = json.loads(settings.read_text(encoding="utf-8"))
            reloaded = subprocess.run(
                [str(executable), "mcp", "list"],
                cwd=cwd,
                env=environment,
                capture_output=True,
                timeout=10,
            )
            disconnect_ok = (
                disconnected.returncode == 0
                and reloaded.returncode == 0
                and restored.get("userAddedAfterConnect") is True
                and restored.get("env") == {"KEEP": "yes"}
                and restored.get("permissions") == {"allow": []}

            )
            profile_ok = preview_ok and applied is not None and applied.returncode == 0
            result["profile_load"] = {
                "status": "verified" if profile_ok and wire_ok else "failed",
                "reviewed_connect": preview_ok,
                "installed_client_used_base_model_and_header": wire_ok,
                "without_driver_auth_environment": {
                    "status": "verified" if header_only_ok else "unsupported",
                    "exit_code": header_only_exit,
                    "timed_out": header_only_timed_out,
                    "upstream_attempts": header_only_calls,
                },
            }
            result["selection"] = {
                "status": "verified" if wire_ok else "failed",
                "selected_model": "example-model",
                "root": "claude-work",
            }
            result["inference"] = {
                "status": "verified" if inference.returncode == 0 and wire_ok else "failed",
                "upstream_attempts": len(fixture.observations),
            }
            result["tools"] = {
                "status": "verified" if tool_ok else "failed",
                "observed_read_tool_call": observed_read,
                "requested_path_matches": observed_read,
                "successful_exact_synthetic_result_in_followup": (
                    fixture.observations[1]["tool_result_matches"]
                    if len(fixture.observations) > 1
                    else False
                ),
                "subsequent_model_request": len(fixture.observations) == 2,
            }
            result["wire"] = {
                "status": "verified" if wire_ok else "failed",
                "path": "/r/claude-work/v1/messages -> /team/v1/messages",
                "local_token_stripped_upstream": wire_ok,
                "x_api_key_stripped_upstream": wire_ok,
                "authorization_absent_upstream": wire_ok,
            }
            result["gateway_off"] = {
                "status": "expected_failure_verified" if negative_ok else "failed",
                "upstream_attempts": len(fixture.observations) - attempts_before,
                "client_timed_out_after_gateway_off": negative_timed_out,
                "counted_as_positive": False,
            }
            result["disconnect"] = {
                "status": "verified" if disconnect_ok else "failed",
                "unrelated_user_addition_preserved": restored.get("userAddedAfterConnect") is True,
                "managed_env_removed": restored.get("env") == {"KEEP": "yes"},
                "installed_client_reload": reloaded.returncode == 0,
            }
            result["passed"] = profile_ok and wire_ok and tool_ok and negative_ok and disconnect_ok
        finally:
            if gateway.poll() is None:
                cleanup = probe_pi.stop_gateway(gateway, gateway_port, control_token)
            fixture.close()
    result["cleanup"] = {"gateway": cleanup, "temporary_directory": "TemporaryDirectory reaped"}
    result["finished_at"] = now()
    write(output, result)
    return 0 if result["passed"] else 2


def codex_profile_load(binary: Path, output: Path, executable: Path) -> int:
    version, version_error = installed_version("codex", executable)
    result: dict[str, Any] = {
        "schema_version": 1,
        "client": "codex",
        "started_at": now(),
        "binary": str(binary),
        "binary_sha256": probe_pi.sha256(binary),
        "installed_executable": str(executable),
        "installed_version": version,
        "version_error": version_error,
        "override_scope": "temporary HOME and CODEX_HOME",
        "protocol": "OpenAI Responses with WebSocket disabled",
        "listing": {"status": "pending"},
        "selection": {"status": "pending"},
        "inference": {"status": "pending"},
        "tools": {"status": "pending"},
        "profile_load": {"status": "pending"},
        "disconnect": {"status": "pending"},
        "passed": False,
        "supported_profile_only": True,
    }
    write(output, result)
    if version != "0.154.0":
        result["profile_load"] = {
            "status": "unsupported",
            "reason": "installed version does not match the supported 0.154.0 profile",
        }
        result["disconnect"] = {"status": "not_run"}
        result["finished_at"] = now()
        write(output, result)
        return 2

    cleanup = "not_started"
    with tempfile.TemporaryDirectory(prefix="llmgw-codex-profile-") as temporary:
        root = Path(temporary)
        home = root / "client-home"
        codex_home = home / ".codex"
        gateway_dir = root / "gateway"
        cwd = root / "empty-work"
        for directory in (codex_home, gateway_dir, cwd):
            directory.mkdir(parents=True, mode=0o700)
        tool_path = cwd / "synthetic-read.txt"
        probe_pi.write_private(tool_path, CODEX_TOOL_CONTENT)
        fixture = CodexFixture(tool_path)
        profile = codex_home / "llmgw.config.toml"
        probe_pi.write_private(profile, "this is deliberately invalid = [\n")
        control_environment = isolated_environment(root, home, "codex", codex_home)
        malformed = subprocess.run(
            [str(executable), "--profile", "llmgw", "mcp", "list"],
            cwd=cwd,
            env=control_environment,
            capture_output=True,
            timeout=10,
        )
        probe_pi.write_private(profile, 'notify = ["keep"]\n')
        gateway_port = probe_pi.reserve_loopback_port()
        config = gateway_dir / "config.toml"
        probe_pi.write_private(
            config,
            f'''listen = "127.0.0.1:{gateway_port}"
concurrency = 1
cancel_policy = "drain"
accounting = "reserved"

[upstream]
api_base = "http://127.0.0.1:{fixture.port}/team/v1"

[upstream.auth]
mode = "none"

[quota.rpm]
kind = "unlimited"

[quota.tpm]
kind = "unknown"

[[models]]
id = "example-model"

[[roots]]
id = "codex-work"
endpoints = ["responses"]
models = ["example-model"]
''',
        )
        state_dir = Path(
            json.loads(
                subprocess.check_output(
                    [str(binary), "doctor", "--config", str(config), "--json"], timeout=10
                )
            )["state_directory"]
        )
        state_dir.mkdir(mode=0o700)

        control_token = "task4-codex-control-" + os.urandom(18).hex()

        probe_pi.write_private(state_dir / "control-token", control_token)
        environment = isolated_environment(root, home, "codex", codex_home)
        gateway = subprocess.Popen(
            [str(binary), "run", "--config", str(config)],
            cwd=cwd,
            env=environment,
            stdout=subprocess.DEVNULL,
            stderr=subprocess.DEVNULL,
        )
        try:
            probe_pi.wait_for_gateway(gateway, gateway_port, control_token)
            connect = [
                str(binary), "--config", str(config), "connect", "codex",
                "--client-executable", str(executable), "--client-home", str(home),
                "--root", "codex-work", "--model", "example-model",
            ]
            preview = subprocess.run(
                connect, env=environment, capture_output=True, text=True, timeout=10
            )
            reviewed_hash = next(
                (line.removeprefix("preview hash: ") for line in preview.stdout.splitlines()
                 if line.startswith("preview hash: ")),
                None,
            )
            preview_ok = (
                preview.returncode == 0
                and reviewed_hash is not None


            )
            applied = subprocess.run(
                [*connect, "--apply-hash", str(reviewed_hash)],
                env=environment,
                capture_output=True,
                text=True,
                timeout=10,
            ) if preview_ok else None
            codex_environment = environment.copy()
            loaded = subprocess.run(
                [str(executable), "--profile", "llmgw", "mcp", "list"],
                cwd=cwd,
                env=codex_environment,
                capture_output=True,
                timeout=10,
            )
            exec_command = [
                str(executable),
                "--profile",
                "llmgw",
                "exec",
                "--skip-git-repo-check",
                "--ephemeral",
                "--ignore-rules",
                "--disable",
                "plugins",
                "--disable",
                "remote_plugin",
                "--disable",
                "apps",
                "--sandbox",
                "read-only",
                "--json",
                "-C",
                str(cwd),
                f"Read only the exact synthetic file {tool_path} and report the fixture result.",
            ]
            inference = subprocess.run(
                exec_command,
                cwd=cwd,
                env=codex_environment,
                capture_output=True,
                timeout=20,
            )
            tool_ok = (
                inference.returncode == 0
                and CODEX_TOOL_MARKER.encode() in inference.stdout
                and len(fixture.observations) == 2
                and fixture.observations[1]["tool_output_matches"]
                and json_lines_contain_tool_call(
                    inference.stdout,
                    "exec_command",
                    f"cat {shlex.quote(str(tool_path))}",
                )
            )
            wire_ok = len(fixture.observations) == 2 and all(
                observation[key]
                for observation in fixture.observations
                for key in (
                    "path_matches",
                    "model_matches",
                    "stream_enabled",
                    "local_data_token_absent",
                    "authorization_absent",
                )
            )
            cleanup = probe_pi.stop_gateway(gateway, gateway_port, control_token)
            negative_attempts_before = len(fixture.observations)
            try:
                negative = subprocess.run(
                    exec_command,
                    cwd=cwd,
                    env=codex_environment,
                    capture_output=True,
                    timeout=8,
                )
                negative_failed = negative.returncode != 0
                negative_timed_out = False
            except subprocess.TimeoutExpired:
                negative_failed = True
                negative_timed_out = True
            negative_ok = (
                negative_failed and len(fixture.observations) == negative_attempts_before
            )
            profile.write_text(
                "user_added_after_connect = true\n" + profile.read_text(encoding="utf-8"),
                encoding="utf-8",
            )
            profile.chmod(0o600)
            disconnected = subprocess.run(
                [str(binary), "--config", str(config), "disconnect", "codex"],
                env=environment,
                capture_output=True,
                text=True,
                timeout=10,
            )
            restored_text = profile.read_text(encoding="utf-8")
            reloaded = subprocess.run(
                [str(executable), "--profile", "llmgw", "mcp", "list"],
                cwd=cwd,
                env=codex_environment,
                capture_output=True,
                timeout=10,
            )
            profile_ok = (
                malformed.returncode != 0
                and preview_ok
                and applied is not None
                and applied.returncode == 0
                and loaded.returncode == 0
            )
            disconnect_ok = (
                disconnected.returncode == 0
                and reloaded.returncode == 0
                and "user_added_after_connect = true" in restored_text
                and "llmgw" not in restored_text

            )
            result["profile_load"] = {
                "status": "verified" if profile_ok else "failed",
                "named_profile_parse_control": malformed.returncode != 0,
                "reviewed_connect": preview_ok,
                "installed_client_load": loaded.returncode == 0,
            }
            result["listing"] = {
                "status": "unverified",
                "reason": "the gateway model list is not a Codex catalog",
            }
            result["selection"] = {
                "status": "verified" if wire_ok else "failed",
                "selected_model": "example-model",
                "root": "codex-work",
            }
            result["inference"] = {
                "status": "verified" if inference.returncode == 0 and wire_ok else "failed",
                "upstream_attempts": len(fixture.observations),
            }
            result["tools"] = {
                "status": "verified" if tool_ok else "failed",
                "observed_exec_command_call": json_lines_contain_tool_call(
                    inference.stdout,
                    "exec_command",
                    f"cat {shlex.quote(str(tool_path))}",
                ),
                "exact_synthetic_result_in_followup": fixture.observations[1]["tool_output_matches"]
                if len(fixture.observations) > 1
                else False,
                "subsequent_model_request": len(fixture.observations) == 2,
            }
            result["wire"] = {
                "status": "verified" if wire_ok else "failed",
                "path": "/r/codex-work/v1/responses -> /team/v1/responses",
                "local_token_stripped_upstream": wire_ok,
                "authorization_absent_upstream": wire_ok,
            }
            result["gateway_off"] = {
                "status": "expected_failure_verified" if negative_ok else "failed",
                "upstream_attempts": len(fixture.observations) - negative_attempts_before,
                "client_timed_out_after_gateway_off": negative_timed_out,
                "counted_as_positive": False,
            }
            result["disconnect"] = {
                "status": "verified" if disconnect_ok else "failed",
                "unrelated_user_addition_preserved": "user_added_after_connect = true" in restored_text,
                "managed_provider_removed": "llmgw" not in restored_text,
                "installed_client_reload": reloaded.returncode == 0,
            }
            actual_ok = profile_ok and disconnect_ok and tool_ok and wire_ok and negative_ok
            result["passed"] = actual_ok
            result["supported_profile_only"] = not actual_ok
        finally:
            if gateway.poll() is None:
                cleanup = probe_pi.stop_gateway(gateway, gateway_port, control_token)
            fixture.close()
    result["cleanup"] = {"gateway": cleanup, "temporary_directory": "TemporaryDirectory reaped"}
    result["finished_at"] = now()
    write(output, result)
    return 0 if result["passed"] else 2


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--binary", type=Path, required=True)
    parser.add_argument("--client", choices=("pi", "claude", "codex"), required=True)
    parser.add_argument("--client-executable")
    parser.add_argument("--output", type=Path, required=True)
    args = parser.parse_args()
    binary = args.binary.resolve(strict=True)
    if args.client == "pi":
        return verify_pi(binary, args.output, args.client_executable)
    default = Path("/opt/homebrew/bin/claude" if args.client == "claude" else "/opt/homebrew/bin/codex")
    executable = Path(args.client_executable).resolve() if args.client_executable else default
    if args.client == "codex":
        return codex_profile_load(binary, args.output, executable)
    return verify_claude(binary, args.output, executable)


if __name__ == "__main__":
    raise SystemExit(main())
