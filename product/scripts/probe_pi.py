#!/usr/bin/env python3
"""Exercise an installed Pi through llmgw and a synthetic loopback provider.

The probe uses isolated temporary Pi state, an empty working directory, a fixed
OpenAI Chat Completions fixture, and no real model or user file. Raw prompts,
request bodies, credentials, stdout, and stderr never enter the JSON artifact.
"""

from __future__ import annotations

import argparse
import datetime as dt
import hashlib
import http.client
import json
import os
import platform
import shutil
import socket
import subprocess
import tempfile
import threading
import time
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
from pathlib import Path
from typing import Any


ROOT = Path(__file__).resolve().parents[1]
TEMPLATE = ROOT / "tests" / "fixtures" / "pi-models.json"
PROVIDER_NAME = "llmgw-e2e"
MODEL_ID = "example-model"
ROOT_ID = "pi-work"
LOCAL_PREFIX = f"/r/{ROOT_ID}/v1"
UPSTREAM_PATH = "/team/v1/chat/completions"
E2E_HEADER_VALUE = "task4-installed-pi"
COMPLETION_MARKER = "TASK4_COMPLETION_OK"
TOOL_OUTPUT_MARKER = "TASK4_TOOL_OK"
TOOL_FILE_CONTENT = "TASK4_SYNTHETIC_READ_RESULT"
TOOL_CALL_ID = "call_task4_read_1"
MAX_REQUEST_BYTES = 1024 * 1024
PI_TIMEOUT_SECONDS = 25


def sha256(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as handle:
        for chunk in iter(lambda: handle.read(1024 * 1024), b""):
            digest.update(chunk)
    return digest.hexdigest()


def utc_now() -> str:
    return dt.datetime.now(dt.timezone.utc).isoformat()


def reserve_loopback_port() -> int:
    with socket.socket(socket.AF_INET, socket.SOCK_STREAM) as listener:
        listener.bind(("127.0.0.1", 0))
        return int(listener.getsockname()[1])


def resolve_pi(value: str | None) -> tuple[Path, Path, dict[str, Any]]:
    candidate = value or shutil.which("pi")
    if not candidate:
        raise ValueError("installed Pi executable not found; pass --pi PATH")
    executable = Path(candidate)
    if not executable.exists():
        raise ValueError(f"installed Pi executable not found: {executable}")
    entry = executable.resolve()
    package_dir = entry.parent.parent
    manifest = package_dir / "package.json"
    if not manifest.is_file():
        raise ValueError(f"installed Pi package manifest not found beside: {entry}")
    try:
        package = json.loads(manifest.read_text(encoding="utf-8"))
    except (OSError, json.JSONDecodeError) as error:
        raise ValueError(f"cannot read installed Pi package manifest: {error}") from error
    if package.get("name") != "@earendil-works/pi-coding-agent" or not package.get("version"):
        raise ValueError("the selected executable is not a recognized installed Pi package")
    return executable.absolute(), entry, package


def reference_clone() -> dict[str, Any]:
    clone = ROOT.parent / "references" / "pi"
    manifest = clone / "packages" / "coding-agent" / "package.json"
    result: dict[str, Any] = {"version": None, "commit": None}
    if not manifest.is_file():
        return result
    try:
        result["version"] = json.loads(manifest.read_text(encoding="utf-8")).get("version")
    except (OSError, json.JSONDecodeError):
        pass
    try:
        repository = subprocess.run(
            ["git", "-C", str(clone), "rev-parse", "--show-toplevel"],
            capture_output=True,
            text=True,
            timeout=3,
            check=False,
        )
        if repository.returncode != 0 or Path(repository.stdout.strip()).resolve() != clone.resolve():
            return result
        revision = subprocess.run(
            ["git", "-C", str(clone), "rev-parse", "HEAD"],
            capture_output=True,
            text=True,
            timeout=3,
            check=False,
        )
        if revision.returncode == 0:
            result["commit"] = revision.stdout.strip() or None
    except (OSError, subprocess.TimeoutExpired):
        pass
    return result


def render_models(gateway_base: str, data_token: str) -> str:
    try:
        raw = TEMPLATE.read_text(encoding="utf-8")
    except OSError as error:
        raise ValueError(f"cannot read Pi models template: {error}") from error
    replacements = {
        "__GATEWAY_BASE_URL__": gateway_base,
        "__DATA_TOKEN__": data_token,
    }
    for placeholder, value in replacements.items():
        if raw.count(placeholder) == 0:
            raise ValueError(f"Pi models template is missing {placeholder}")
        raw = raw.replace(placeholder, value)
    if "__" in raw:
        raise ValueError("Pi models template contains an unresolved placeholder")
    try:
        json.loads(raw)
    except json.JSONDecodeError as error:
        raise ValueError(f"rendered Pi models template is invalid JSON: {error}") from error
    return raw


def write_private(path: Path, contents: str) -> None:
    path.write_text(contents, encoding="utf-8")
    path.chmod(0o600)


def isolated_pi_environment(temp: Path, home: Path, agent_dir: Path) -> dict[str, str]:
    environment = {
        "PATH": os.environ.get("PATH", "/opt/homebrew/bin:/usr/local/bin:/usr/bin:/bin"),
        "LANG": "en_US.UTF-8",
        "LC_ALL": "en_US.UTF-8",
        "HOME": str(home),
        "USERPROFILE": str(home),
        "PI_CODING_AGENT_DIR": str(agent_dir),
        "PI_OFFLINE": "1",
        "PI_TELEMETRY": "0",
        "NO_COLOR": "1",
        "DO_NOT_TRACK": "1",
        "OTEL_SDK_DISABLED": "true",
    }
    for name in ("SystemRoot", "WINDIR", "ComSpec", "PATHEXT"):
        if name in os.environ:
            environment[name] = os.environ[name]
    for name in ("TMPDIR", "XDG_CONFIG_HOME", "XDG_CACHE_HOME", "XDG_DATA_HOME"):
        directory = temp / name.lower().replace("_home", "")
        directory.mkdir(mode=0o700)
        environment[name] = str(directory)
    return environment


def sse(chunks: list[dict[str, Any]]) -> bytes:
    records = [f"data: {json.dumps(chunk, separators=(',', ':'))}\n\n" for chunk in chunks]
    records.append("data: [DONE]\n\n")
    return "".join(records).encode("utf-8")


def completion_response(marker: str, request_number: int) -> bytes:
    base = {
        "id": f"chatcmpl-task4-{request_number}",
        "object": "chat.completion.chunk",
        "created": 0,
        "model": MODEL_ID,
    }
    return sse(
        [
            {**base, "choices": [{"index": 0, "delta": {"role": "assistant", "content": marker}, "finish_reason": None}]},
            {**base, "choices": [{"index": 0, "delta": {}, "finish_reason": "stop"}]},
            {**base, "choices": [], "usage": {"prompt_tokens": 1, "completion_tokens": 1, "total_tokens": 2}},
        ]
    )


def tool_call_response(tool_path: Path) -> bytes:
    base = {
        "id": "chatcmpl-task4-tool-call",
        "object": "chat.completion.chunk",
        "created": 0,
        "model": MODEL_ID,
    }
    arguments = json.dumps({"path": str(tool_path)}, separators=(",", ":"))
    return sse(
        [
            {
                **base,
                "choices": [
                    {
                        "index": 0,
                        "delta": {
                            "role": "assistant",
                            "tool_calls": [
                                {
                                    "index": 0,
                                    "id": TOOL_CALL_ID,
                                    "type": "function",
                                    "function": {"name": "read", "arguments": arguments},
                                }
                            ],
                        },
                        "finish_reason": None,
                    }
                ],
            },
            {**base, "choices": [{"index": 0, "delta": {}, "finish_reason": "tool_calls"}]},
            {**base, "choices": [], "usage": {"prompt_tokens": 1, "completion_tokens": 1, "total_tokens": 2}},
        ]
    )


class FixtureState:
    def __init__(self, flow: str, tool_path: Path):
        self.flow = flow
        self.tool_path = tool_path
        self.lock = threading.Lock()
        self.observations: list[dict[str, Any]] = []

    def observe(self, handler: BaseHTTPRequestHandler, payload: Any, body_bytes: int) -> tuple[dict[str, Any], bytes]:
        with self.lock:
            attempt = len(self.observations) + 1
        object_body = isinstance(payload, dict)
        tools = payload.get("tools") if object_body else None
        tool_names = []
        if isinstance(tools, list):
            for tool in tools:
                if isinstance(tool, dict):
                    function = tool.get("function")
                    if isinstance(function, dict) and isinstance(function.get("name"), str):
                        tool_names.append(function["name"])
        messages = payload.get("messages") if object_body else None
        message_roles = [message.get("role") for message in messages if isinstance(message, dict)] if isinstance(messages, list) else []
        assistant_call_matches = False
        tool_result_matches = False
        if isinstance(messages, list):
            for message in messages:
                if not isinstance(message, dict):
                    continue
                if message.get("role") == "assistant" and isinstance(message.get("tool_calls"), list):
                    assistant_call_matches = any(
                        isinstance(call, dict)
                        and call.get("id") == TOOL_CALL_ID
                        and isinstance(call.get("function"), dict)
                        and call["function"].get("name") == "read"
                        for call in message["tool_calls"]
                    )
                if message.get("role") == "tool":
                    tool_result_matches = (
                        message.get("tool_call_id") == TOOL_CALL_ID
                        and message.get("content") == TOOL_FILE_CONTENT
                    )
        observation = {
            "attempt": attempt,
            "method_matches": handler.command == "POST",
            "path_matches": handler.path == UPSTREAM_PATH,
            "body_within_cap": body_bytes <= MAX_REQUEST_BYTES,
            "model_matches": object_body and payload.get("model") == MODEL_ID,
            "stream_enabled": object_body and payload.get("stream") is True,
            "custom_header_received": (
                handler.headers.get("x-synthetic-e2e") == E2E_HEADER_VALUE
                or handler.headers.get("x-llmgw-client") == "pi"
            ),
            "local_data_token_absent": handler.headers.get("x-llmgw-token") is None,
            "local_control_token_absent": handler.headers.get("x-llmgw-control-token") is None,
            "authorization_absent": handler.headers.get("authorization") is None,
            "tool_names": tool_names,
            "message_roles": message_roles,
            "assistant_tool_call_matches": assistant_call_matches,
            "tool_result_matches": tool_result_matches,
        }
        with self.lock:
            self.observations.append(observation)
        common_ok = all(
            observation[key]
            for key in (
                "method_matches",
                "path_matches",
                "body_within_cap",
                "model_matches",
                "stream_enabled",
                "custom_header_received",
                "local_data_token_absent",
                "local_control_token_absent",
                "authorization_absent",
            )
        )
        if not common_ok:
            return observation, b""
        if self.flow == "completion" and attempt == 1 and tool_names == []:
            return observation, completion_response(COMPLETION_MARKER, attempt)
        if self.flow == "read_tool" and attempt == 1 and tool_names == ["read"]:
            return observation, tool_call_response(self.tool_path)
        if (
            self.flow == "read_tool"
            and attempt == 2
            and tool_names == ["read"]
            and assistant_call_matches
            and tool_result_matches
        ):
            return observation, completion_response(TOOL_OUTPUT_MARKER, attempt)
        return observation, b""


def start_fixture(flow: str, tool_path: Path) -> tuple[ThreadingHTTPServer, threading.Thread, FixtureState]:
    state = FixtureState(flow, tool_path)

    class Handler(BaseHTTPRequestHandler):
        protocol_version = "HTTP/1.1"

        def log_message(self, *_args: Any) -> None:
            pass

        def do_POST(self) -> None:
            self.connection.settimeout(5)
            try:
                length = int(self.headers.get("Content-Length", "-1"))
            except ValueError:
                length = -1
            if length < 0 or length > MAX_REQUEST_BYTES:
                self.send_error(413)
                return
            try:
                body = self.rfile.read(length)
            except TimeoutError:
                self.send_error(408)
                return
            try:
                payload = json.loads(body)
            except (UnicodeDecodeError, json.JSONDecodeError):
                self.send_error(400)
                return
            _observation, response = state.observe(self, payload, length)
            if not response:
                error = b'{"error":{"message":"unexpected synthetic request","type":"fixture_error","code":"fixture_error"}}'
                self.send_response(400)
                self.send_header("Content-Type", "application/json")
                self.send_header("Content-Length", str(len(error)))
                self.end_headers()
                self.wfile.write(error)
                return
            self.send_response(200)
            self.send_header("Content-Type", "text/event-stream")
            self.send_header("Content-Length", str(len(response)))
            self.end_headers()
            self.wfile.write(response)
            self.wfile.flush()

    server = ThreadingHTTPServer(("127.0.0.1", 0), Handler)
    server.daemon_threads = True
    thread = threading.Thread(target=server.serve_forever, kwargs={"poll_interval": 0.05}, daemon=True)
    thread.start()
    return server, thread, state


def gateway_config(port: int, upstream_port: int) -> str:
    return f'''listen = "127.0.0.1:{port}"
concurrency = 1
cancel_policy = "drain"
accounting = "reserved"

[upstream]
api_base = "http://127.0.0.1:{upstream_port}/team/v1"

[upstream.auth]
mode = "none"

[quota.rpm]
kind = "unlimited"

[quota.tpm]
kind = "unknown"

[[models]]
id = "{MODEL_ID}"
max_output_tokens = 32

[[roots]]
id = "{ROOT_ID}"
endpoints = ["chat/completions"]
models = ["{MODEL_ID}"]
'''


def control_request(port: int, token: str, method: str, path: str, timeout: float = 0.5) -> int | None:
    connection = http.client.HTTPConnection("127.0.0.1", port, timeout=timeout)
    try:
        connection.request(method, path, headers={"x-llmgw-control-token": token, "Content-Length": "0"})
        response = connection.getresponse()
        response.read()
        return response.status
    except OSError:
        return None
    finally:
        connection.close()


def wait_for_gateway(process: subprocess.Popen[bytes], port: int, control_token: str) -> None:
    deadline = time.monotonic() + 5
    while time.monotonic() < deadline:
        if process.poll() is not None:
            raise RuntimeError(f"gateway exited during startup with code {process.returncode}")
        if control_request(port, control_token, "GET", "/_llmgw/health") == 200:
            return
        time.sleep(0.03)
    raise RuntimeError("gateway did not become ready within 5 seconds")


def stop_process(process: subprocess.Popen[bytes], timeout: float = 3) -> str:
    if process.poll() is not None:
        return "already_exited"
    process.terminate()
    try:
        process.wait(timeout=timeout)
        return "terminated"
    except subprocess.TimeoutExpired:
        process.kill()
        process.wait(timeout=timeout)
        return "killed"


def stop_gateway(process: subprocess.Popen[bytes], port: int, control_token: str) -> str:
    if process.poll() is not None:
        return "already_exited"
    status = control_request(port, control_token, "POST", "/_llmgw/stop", timeout=1)
    if status == 200:
        try:
            process.wait(timeout=5)
            return "control_stop"
        except subprocess.TimeoutExpired:
            pass
    return stop_process(process)


def run_pi(command: list[str], cwd: Path, env: dict[str, str]) -> tuple[int, bytes, bytes, bool, str]:
    process = subprocess.Popen(command, cwd=cwd, env=env, stdout=subprocess.PIPE, stderr=subprocess.PIPE)
    cleanup = "normal_exit"
    try:
        try:
            stdout, stderr = process.communicate(timeout=PI_TIMEOUT_SECONDS)
            return process.returncode, stdout, stderr, False, cleanup
        except subprocess.TimeoutExpired:
            cleanup = stop_process(process)
            stdout, stderr = process.communicate()
            return process.returncode, stdout, stderr, True, cleanup
    finally:
        if process.poll() is None:
            stop_process(process)
        if process.stdout is not None:
            process.stdout.close()
        if process.stderr is not None:
            process.stderr.close()


def parse_pi_events(stdout: bytes) -> tuple[list[dict[str, Any]], bool]:
    if len(stdout) > 2 * 1024 * 1024:
        return [], False
    events: list[dict[str, Any]] = []
    try:
        for line in stdout.decode("utf-8").splitlines():
            value = json.loads(line)
            if not isinstance(value, dict):
                return [], False
            events.append(value)
    except (UnicodeDecodeError, json.JSONDecodeError):
        return [], False
    return events, True


def event_text_contains(events: list[dict[str, Any]], marker: str) -> bool:
    for event in events:
        if event.get("type") != "message_end":
            continue
        message = event.get("message")
        if not isinstance(message, dict) or message.get("role") != "assistant":
            continue
        content = message.get("content")
        if isinstance(content, list) and any(
            isinstance(block, dict) and block.get("type") == "text" and marker in str(block.get("text", ""))
            for block in content
        ):
            return True
    return False


def assistant_failure_observed(events: list[dict[str, Any]]) -> bool:
    for event in events:
        message = event.get("message")
        if (
            event.get("type") == "message_end"
            and isinstance(message, dict)
            and message.get("role") == "assistant"
            and message.get("stopReason") in {"error", "aborted"}
        ):
            return True
        update = event.get("assistantMessageEvent")
        if event.get("type") == "message_update" and isinstance(update, dict) and update.get("type") == "error":
            return True
    return False


def summarize_tool_events(events: list[dict[str, Any]], tool_path: Path) -> dict[str, Any]:
    starts = [event for event in events if event.get("type") == "tool_execution_start"]
    ends = [event for event in events if event.get("type") == "tool_execution_end"]
    exact_start = len(starts) == 1 and starts[0].get("toolName") == "read" and starts[0].get("toolCallId") == TOOL_CALL_ID
    exact_arguments = exact_start and starts[0].get("args") == {"path": str(tool_path)}
    exact_end = (
        len(ends) == 1
        and ends[0].get("toolName") == "read"
        and ends[0].get("toolCallId") == TOOL_CALL_ID
        and ends[0].get("isError") is False
    )
    result_contains_marker = False
    if exact_end:
        result = ends[0].get("result")
        if isinstance(result, dict) and isinstance(result.get("content"), list):
            result_contains_marker = any(
                isinstance(block, dict)
                and block.get("type") == "text"
                and block.get("text") == TOOL_FILE_CONTENT
                for block in result["content"]
            )
    return {
        "start_count": len(starts),
        "end_count": len(ends),
        "exact_read_start": exact_start,
        "exact_read_arguments": exact_arguments,
        "exact_read_end_without_error": exact_end,
        "result_contains_exact_synthetic_content": result_contains_marker,
    }


def run_flow(
    pi: Path,
    binary: Path,
    flow: str,
    gateway_off: bool,
    native_connect: bool = False,
) -> dict[str, Any]:
    with tempfile.TemporaryDirectory(prefix=f"llmgw-pi-{flow}-") as temporary:
        temp = Path(temporary)
        client_home = temp / "client-home"
        agent_dir = client_home / ".pi" / "agent" if native_connect else temp / "agent"
        cwd = temp / "empty-work"
        fixture_dir = temp / "fixture"
        config_dir = temp / "gateway"
        for directory in (agent_dir, cwd, fixture_dir, config_dir):
            directory.mkdir(parents=True)
        env = isolated_pi_environment(temp, client_home, agent_dir)
        tool_path = fixture_dir / "synthetic-read.txt"
        write_private(tool_path, TOOL_FILE_CONTENT)
        fixture, fixture_thread, state = start_fixture(flow, tool_path)
        gateway: subprocess.Popen[bytes] | None = None
        gateway_cleanup = "not_started"
        adapter = {
            "native_connect": native_connect,
            "preview_succeeded": False,
            "apply_succeeded": False,
            "connected_model_listed": False,
            "disconnect_succeeded": False,
            "actual_reload_succeeded": False,
            "unrelated_user_addition_preserved": False,
            "managed_provider_removed": False,
        }
        data_token = "task4-data-" + os.urandom(18).hex()
        control_token = "task4-control-" + os.urandom(18).hex()
        gateway_port = reserve_loopback_port()
        try:
            write_private(config_dir / "config.toml", gateway_config(gateway_port, fixture.server_port))
            state_dir = Path(json.loads(subprocess.check_output([str(binary), "doctor", "--config", str(config_dir / "config.toml"), "--json"], cwd=cwd, env=env, timeout=10))["state_directory"])
            state_dir.mkdir(mode=0o700)
            write_private(state_dir / "data-token", data_token)
            write_private(state_dir / "control-token", control_token)
            if native_connect:
                write_private(
                    agent_dir / "models.json",
                    json.dumps(
                        {
                            "providers": {
                                "other": {
                                    "baseUrl": "https://example.invalid/v1",
                                    "api": "openai-completions",
                                    "apiKey": "placeholder",
                                    "models": [],
                                }
                            },
                            "userBefore": True,
                        },
                        separators=(",", ":"),
                    ),
                )
                write_private(
                    agent_dir / "settings.json",
                    json.dumps({"quietStartup": True}, separators=(",", ":")),
                )
            else:
                write_private(
                    agent_dir / "models.json",
                    render_models(f"http://127.0.0.1:{gateway_port}{LOCAL_PREFIX}", data_token),
                )
                write_private(
                    agent_dir / "settings.json",
                    json.dumps(
                        {
                            "enableInstallTelemetry": False,
                            "enableAnalytics": False,
                            "compaction": {"enabled": False},
                            "retry": {"enabled": False, "maxRetries": 0, "provider": {"maxRetries": 0}},
                            "defaultThinkingLevel": "off",
                            "quietStartup": True,
                        },
                        separators=(",", ":"),
                    ),
                )
            if not gateway_off or native_connect:
                gateway = subprocess.Popen(
                    [str(binary), "run", "--config", str(config_dir / "config.toml")],
                    cwd=cwd,
                    env=env,
                    stdout=subprocess.DEVNULL,
                    stderr=subprocess.DEVNULL,
                )
                wait_for_gateway(gateway, gateway_port, control_token)
            if native_connect:
                connect_command = [
                    str(binary),
                    "--config",
                    str(config_dir / "config.toml"),
                    "connect",
                    "pi",
                    "--client-executable",
                    str(pi),
                    "--client-home",
                    str(client_home),
                    "--root",
                    ROOT_ID,
                    "--model",
                    MODEL_ID,
                ]
                preview = subprocess.run(connect_command, cwd=cwd, env=env, capture_output=True, text=True, timeout=10)
                preview_hash = next(
                    (
                        line.removeprefix("preview hash: ")
                        for line in preview.stdout.splitlines()
                        if line.startswith("preview hash: ")
                    ),
                    None,
                )
                adapter["preview_succeeded"] = (
                    preview.returncode == 0
                    and preview_hash is not None
                    and data_token not in preview.stdout
                    and data_token not in preview.stderr
                )
                if not adapter["preview_succeeded"]:
                    raise RuntimeError("native Pi connect preview failed or exposed the local token")
                applied = subprocess.run(
                    [*connect_command, "--apply-hash", str(preview_hash)],
                    cwd=cwd,
                    env=env,
                    capture_output=True,
                    text=True,
                    timeout=10,
                )
                adapter["apply_succeeded"] = applied.returncode == 0
                if not adapter["apply_succeeded"]:
                    raise RuntimeError("native Pi connect apply failed")
                if gateway_off and gateway is not None:
                    gateway_cleanup = stop_gateway(gateway, gateway_port, control_token)
                    gateway = None
            if native_connect:
                listed = subprocess.run(
                    [str(pi), "--offline", "--no-extensions", "--list-models", "llmgw"],
                    cwd=cwd,
                    env=env,
                    capture_output=True,
                    timeout=10,
                )
                adapter["connected_model_listed"] = (
                    listed.returncode == 0
                    and MODEL_ID.encode() in listed.stdout
                    and b"Invalid models.json" not in listed.stderr
                    and b"Failed to load models.json" not in listed.stderr
                )
            command = [
                str(pi),
                "--offline",
                "--no-approve",
                "--no-extensions",
                "--no-skills",
                "--no-prompt-templates",
                "--no-themes",
                "--no-context-files",
                "--no-session",
                "--provider",
                "llmgw" if native_connect else PROVIDER_NAME,
                "--model",
                MODEL_ID,
                "--thinking",
                "off",
                "--system-prompt",
                "Follow only the synthetic fixture response.",
                "--mode",
                "json",
                "--print",
            ]
            if flow == "completion":
                command.extend(["--no-tools", "Return the synthetic completion marker."])
            else:
                command.extend(["--tools", "read", "Read the exact synthetic path supplied by the fixture."])
            exit_code, stdout, stderr, timed_out, pi_cleanup = run_pi(command, cwd, env)
            events, json_valid = parse_pi_events(stdout)
            tool_events = summarize_tool_events(events, tool_path)
            assistant_failed = assistant_failure_observed(events)
            completion_marker = event_text_contains(
                events, COMPLETION_MARKER if flow == "completion" else TOOL_OUTPUT_MARKER
            )
            with state.lock:
                observations = list(state.observations)
            expected_attempts = 1 if flow == "completion" else 2
            common_wire = len(observations) == expected_attempts and all(
                all(
                    row[key]
                    for key in (
                        "method_matches",
                        "path_matches",
                        "body_within_cap",
                        "model_matches",
                        "stream_enabled",
                        "custom_header_received",
                        "local_data_token_absent",
                        "local_control_token_absent",
                        "authorization_absent",
                    )
                )
                for row in observations
            )
            flow_specific = False
            if flow == "completion":
                flow_specific = len(observations) == 1 and observations[0]["tool_names"] == []
            elif len(observations) == 2:
                flow_specific = (
                    observations[0]["tool_names"] == ["read"]
                    and observations[1]["tool_names"] == ["read"]
                    and observations[1]["assistant_tool_call_matches"]
                    and observations[1]["tool_result_matches"]
                    and tool_events["start_count"] == 1
                    and tool_events["end_count"] == 1
                    and tool_events["exact_read_start"]
                    and tool_events["exact_read_arguments"]
                    and tool_events["exact_read_end_without_error"]
                    and tool_events["result_contains_exact_synthetic_content"]
                )
            passed = (
                exit_code == 0
                and not timed_out
                and json_valid
                and not assistant_failed
                and completion_marker
                and common_wire
                and flow_specific
            )
            result = {
                "flow": flow,
                "passed": passed,
                "pi_exit_code": exit_code,
                "pi_timed_out": timed_out,
                "pi_cleanup": pi_cleanup,
                "stdout_json_valid": json_valid,
                "assistant_failure_observed": assistant_failed,
                "stderr_present": bool(stderr.strip()),
                "output_marker_observed": completion_marker,
                "upstream_attempts": len(observations),
                "expected_positive_attempts": expected_attempts,
                "gateway_root_route_exercised": common_wire,
                "wire_observations": observations,
                "tool_events": tool_events,
                "adapter": adapter,
            }
            if native_connect:
                current = json.loads((agent_dir / "models.json").read_text(encoding="utf-8"))
                current["userAddedAfterConnect"] = True
                write_private(agent_dir / "models.json", json.dumps(current, indent=2))
                disconnected = subprocess.run(
                    [
                        str(binary),
                        "--config",
                        str(config_dir / "config.toml"),
                        "disconnect",
                        "pi",
                    ],
                    cwd=cwd,
                    env=env,
                    capture_output=True,
                    text=True,
                    timeout=10,
                )
                adapter["disconnect_succeeded"] = disconnected.returncode == 0
                restored = json.loads((agent_dir / "models.json").read_text(encoding="utf-8"))
                adapter["unrelated_user_addition_preserved"] = (
                    restored.get("userBefore") is True
                    and restored.get("userAddedAfterConnect") is True
                )
                adapter["managed_provider_removed"] = "llmgw" not in restored.get("providers", {})
                reload_run = subprocess.run(
                    [str(pi), "--offline", "--no-extensions", "--list-models", "llmgw"],
                    cwd=cwd,
                    env=env,
                    capture_output=True,
                    timeout=10,
                )
                adapter["actual_reload_succeeded"] = (
                    reload_run.returncode == 0
                    and MODEL_ID.encode() not in reload_run.stdout
                    and b"Invalid models.json" not in reload_run.stderr
                    and b"Failed to load models.json" not in reload_run.stderr
                )
                result["passed"] = result["passed"] and all(
                    adapter[key]
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
        finally:
            if gateway is not None:
                gateway_cleanup = stop_gateway(gateway, gateway_port, control_token)
            fixture.shutdown()
            fixture.server_close()
            fixture_thread.join(timeout=2)
            if gateway is not None and gateway.poll() is None:
                gateway_cleanup = stop_process(gateway)
        result["gateway_cleanup"] = gateway_cleanup
        return result


def build_metadata(pi: Path, entry: Path, package: dict[str, Any], binary: Path) -> dict[str, Any]:
    package_dir = entry.parent.parent
    source_paths = [
        package_dir / "package.json",
        package_dir / "dist" / "core" / "tools" / "read.js",
        package_dir
        / "node_modules"
        / "@earendil-works"
        / "pi-ai"
        / "dist"
        / "api"
        / "openai-completions.js",
    ]
    sources = {
        str(path.relative_to(package_dir)): sha256(path)
        for path in source_paths
        if path.is_file()
    }
    clone = reference_clone()
    clone_version = clone.get("version")
    return {
        "scope": "installed Pi -> llmgw -> synthetic loopback OpenAI Chat Completions fixture",
        "classification": "agent/mock compatibility; not real LLM quality or performance",
        "protocol": "OpenAI Chat Completions SSE over HTTP/1.1",
        "system": platform.platform(),
        "pi": {
            "package": package["name"],
            "installed_version": package["version"],
            "executable": str(pi),
            "entry_path": str(entry),
            "source_sha256": sources,
        },
        "reference_clone": {
            **clone,
            "differs_from_installed": (
                clone_version != package["version"] if isinstance(clone_version, str) else None
            ),
        },
        "gateway": {
            "binary": str(binary),
            "local_route": f"{LOCAL_PREFIX}/chat/completions",
            "upstream_route": UPSTREAM_PATH,
            "sha256_before": sha256(binary),
        },
        "harness": {
            "script_sha256": sha256(Path(__file__)),
            "template_sha256": sha256(TEMPLATE),
            "quota_fixture": {"rpm": "unlimited", "tpm": "unknown", "purpose": "isolated agent/mock transport probe"},
            "configured_policy_names": [
                "pi_agent_retry_disabled",
                "pi_provider_retry_disabled",
                "pi_compaction_disabled",
                "pi_telemetry_disabled",
                "gateway_auth_none",
                "gateway_cancel_drain",
                "gateway_rpm_unlimited",
                "gateway_tpm_unknown",
            ],
        },
    }


def write_artifact(path: Path, result: dict[str, Any]) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(json.dumps(result, indent=2, sort_keys=True) + "\n", encoding="utf-8")


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--pi", help="installed Pi executable (default: pi on PATH)")
    parser.add_argument("--binary", type=Path, required=True, help="llmgw binary to exercise")
    parser.add_argument("--output", type=Path, required=True, help="sanitized JSON artifact")
    parser.add_argument(
        "--gateway-off",
        action="store_true",
        help="run both inputs without starting llmgw; writes a failing negative artifact and exits nonzero",
    )
    args = parser.parse_args()
    try:
        pi, entry, package = resolve_pi(args.pi)
        binary = args.binary.resolve(strict=True)
        if not binary.is_file():
            raise ValueError(f"llmgw binary is not a file: {binary}")
        if not TEMPLATE.is_file():
            raise ValueError(f"Pi models template not found: {TEMPLATE}")
        metadata = build_metadata(pi, entry, package, binary)
    except (ValueError, OSError) as error:
        parser.error(str(error))
    result = {
        "schema_version": 1,
        "started_at": utc_now(),
        "mode": "gateway_off_negative" if args.gateway_off else "positive",
        **metadata,
        "cases": [],
        "passed": False,
    }
    write_artifact(args.output, result)
    try:
        for flow in ("completion", "read_tool"):
            case = run_flow(pi, binary, flow, args.gateway_off)
            result["cases"].append(case)
            write_artifact(args.output, result)
            print(
                json.dumps(
                    {
                        "flow": flow,
                        "passed": case["passed"],
                        "pi_exit_code": case["pi_exit_code"],
                        "upstream_attempts": case["upstream_attempts"],
                    },
                    sort_keys=True,
                ),
                flush=True,
            )
    finally:
        result["finished_at"] = utc_now()
        result["gateway"]["sha256_after"] = sha256(binary)
        result["gateway"]["binary_unchanged_after_run"] = (
            result["gateway"]["sha256_after"] == result["gateway"]["sha256_before"]
        )
        if args.gateway_off:
            result["expected_failure_observed"] = (
                len(result["cases"]) == 2
                and all(case["upstream_attempts"] == 0 for case in result["cases"])
                and all(case["assistant_failure_observed"] for case in result["cases"])
                and all(not case["output_marker_observed"] for case in result["cases"])
            )
        result["positive_flow_count"] = sum(case["passed"] for case in result["cases"])
        result["passed"] = (
            len(result["cases"]) == 2
            and result["positive_flow_count"] == 2
            and result["gateway"]["binary_unchanged_after_run"]
        )
        write_artifact(args.output, result)
    return 0 if result["passed"] else 1


if __name__ == "__main__":
    raise SystemExit(main())
