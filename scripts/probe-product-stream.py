#!/usr/bin/env python3
"""Check foreground stream lifetime using synthetic, gated loopback HTTP only."""

import argparse
from concurrent.futures import ThreadPoolExecutor
from datetime import datetime, timezone
import hashlib
import http.client
import http.server
import json
from pathlib import Path
import platform
import re
import secrets
import select
import socket
import struct
import subprocess
import tempfile
import threading
import time


BODY = b'{"model":"example-model","messages":[],"stream":true}'
EVENTS = (
    b'data: {"choices":[{"delta":{"content":"fixture"}}]}\n\n'
    b'data: {"choices":[],"usage":{"prompt_tokens":4,"completion_tokens":1,"total_tokens":5}}\n\n'
    b'data: [DONE]\n\n'
)


def require(condition, message):
    if not condition:
        raise AssertionError(message)


def wait_until(predicate, seconds=3):
    deadline = time.monotonic() + seconds
    while not predicate():
        require(time.monotonic() < deadline, "condition deadline exceeded")
        time.sleep(0.01)


class GatedUpstream(http.server.ThreadingHTTPServer):
    daemon_threads = False
    block_on_close = True

    def __init__(self, pre_headers, gate_timeout=8):
        super().__init__(("127.0.0.1", 0), UpstreamHandler)
        self.pre_headers = pre_headers
        self.gate_timeout = gate_timeout
        self.lock = threading.Lock()
        self.attempts = 0
        self.first_started = threading.Event()
        self.first_disconnected = threading.Event()
        self.release = threading.Event()
        self.finished = threading.Event()
        self.fixture_errors = []

    def count(self):
        with self.lock:
            return self.attempts


class UpstreamHandler(http.server.BaseHTTPRequestHandler):
    protocol_version = "HTTP/1.1"

    def log_message(self, *_args):
        pass

    def do_POST(self):
        self.connection.settimeout(4)
        try:
            received = self.rfile.read(int(self.headers["Content-Length"]))
            require(received == BODY, "fixture received changed body")
            with self.server.lock:
                self.server.attempts += 1
                number = self.server.attempts
            if number == 1:
                self.server.first_started.set()
            if number == 1 and self.server.pre_headers:
                if not self.gate():
                    return
            self.send_response(200)
            self.send_header("Content-Type", "text/event-stream")
            self.send_header("Transfer-Encoding", "chunked")
            self.send_header("Connection", "close")
            self.end_headers()
            self.wfile.write(f"{len(EVENTS):x}\r\n".encode() + EVENTS + b"\r\n")
            self.wfile.flush()
            if number == 1 and not self.server.pre_headers:
                if not self.gate():
                    return
            self.wfile.write(b"0\r\n\r\n")
            self.wfile.flush()
            if number == 1:
                self.server.finished.set()
        except (BrokenPipeError, ConnectionResetError):
            self.server.first_disconnected.set()
        except Exception as error:
            # Store a safe class only; no headers, URL, request or token output.
            with self.server.lock:
                self.server.fixture_errors.append(type(error).__name__)
        finally:
            self.close_connection = True

    def gate(self):
        deadline = time.monotonic() + self.server.gate_timeout
        while not self.server.release.is_set():
            require(time.monotonic() < deadline, "fixture gate deadline exceeded")
            readable, _, _ = select.select([self.connection], [], [], 0.02)
            if readable:
                try:
                    if self.connection.recv(1, socket.MSG_PEEK) == b"":
                        self.server.first_disconnected.set()
                        return False
                except (ConnectionResetError, BrokenPipeError):
                    self.server.first_disconnected.set()
                    return False
        return True


def run_case(binary, fixture_template, policy, pre_headers, force_shutdown=False):
    upstream = GatedUpstream(pre_headers, gate_timeout=30 if force_shutdown else 8)
    upstream_thread = threading.Thread(target=upstream.serve_forever)
    upstream_thread.start()
    process = first = None
    pool = ThreadPoolExecutor(max_workers=1)
    try:
        with tempfile.TemporaryDirectory(prefix="llmgw-stream-probe-") as directory:
            base = Path(directory)
            with socket.socket() as candidate:
                candidate.bind(("127.0.0.1", 0))
                port = candidate.getsockname()[1]
            config = base / "fixture.toml"
            text = fixture_template.replace("127.0.0.1:4141", f"127.0.0.1:{port}")
            text = text.replace("127.0.0.1:18080", f"127.0.0.1:{upstream.server_port}")
            text = re.sub(r"(?m)^cancel_policy\s*=.*\n", "", text, count=1)
            # The default drain is tested without setting cancel_policy explicitly.
            if policy == "close":
                text = 'cancel_policy = "close"\n' + text
            config.write_text(text)
            state = Path(json.loads(subprocess.check_output([str(binary), "doctor", "--config", str(config), "--json"], timeout=10))["state_directory"])
            state.mkdir(mode=0o700)
            data_token, control_token = secrets.token_hex(32), secrets.token_hex(32)
            for name, value in (("data-token", data_token), ("control-token", control_token)):
                file = state / name
                file.write_text(value)
                file.chmod(0o600)
            process = subprocess.Popen(
                [str(binary), "run", "--config", str(config)],
                stdout=subprocess.PIPE, stderr=subprocess.PIPE,
            )

            def request(control_path=None):
                connection = http.client.HTTPConnection("127.0.0.1", port, timeout=5)
                try:
                    if control_path:
                        method = "POST" if control_path == "stop" else "GET"
                        connection.request(method, f"/_llmgw/{control_path}", headers={
                            "X-LLMGW-Control-Token": control_token, "Connection": "close"})
                    else:
                        connection.request("POST", "/r/pi-work/v1/chat/completions", BODY, {
                            "X-LLMGW-Token": data_token, "Content-Type": "application/json",
                            "Connection": "close"})
                    response = connection.getresponse()
                    return response.status, response.read()
                finally:
                    connection.close()

            def healthy():
                require(process.poll() is None, "gateway exited before health")
                try:
                    return request("health")[0] == 200
                except ConnectionRefusedError:
                    return False

            wait_until(healthy, 10)
            first = socket.create_connection(("127.0.0.1", port), timeout=3)
            request_head = (
                f"POST /r/pi-work/v1/chat/completions HTTP/1.1\r\nHost: 127.0.0.1:{port}\r\n"
                f"X-LLMGW-Token: {data_token}\r\nContent-Type: application/json\r\n"
                f"Content-Length: {len(BODY)}\r\nConnection: close\r\n\r\n"
            ).encode()
            first.sendall(request_head + BODY)
            require(upstream.first_started.wait(3), "first request did not reach upstream")
            if not pre_headers:
                received = bytearray()
                while b"data: [DONE]\n\n" not in received:
                    part = first.recv(4096)
                    require(part, "downstream ended before terminal marker")
                    received.extend(part)
                    require(len(received) <= 32768, "unexpected fixture response size")
                require(not upstream.release.is_set(), "EOF gate unexpectedly released")
            # A full RST is distinct from a valid HTTP request write-half-close.
            first.setsockopt(socket.SOL_SOCKET, socket.SO_LINGER, struct.pack("ii", 1, 0))
            first.close()
            first = None
            if force_shutdown:
                stopping_at = time.monotonic()
                require(request("stop")[0] == 200, "control stop failed")
                time.sleep(0.2)
                require(process.poll() is None, "process exited without draining active worker")
                require(not upstream.first_disconnected.is_set(), "drain closed HTTP before shutdown deadline")
                stdout, stderr = process.communicate(timeout=15)
                elapsed = time.monotonic() - stopping_at
                require(process.returncode == 0, "forced stop did not exit zero")
                require(9 <= elapsed <= 16, "forced stop outside configured 10s grace observation window")
                require(upstream.first_disconnected.wait(3), "forced stop left upstream HTTP open")
                require(upstream.count() == 1 and not upstream.finished.is_set(), "forced stop fixture completed unexpectedly")
                require(not upstream.fixture_errors, "fixture failed internally")
                require(all(value not in stdout + stderr for value in
                            (data_token.encode(), control_token.encode(), BODY)), "fixture data reached output")
                return {"policy": policy, "disconnect_phase": "after_terminal_marker",
                        "force_shutdown": True, "passed": True, "upstream_attempts": 1,
                        "upstream_close_observed": True, "body_eof_gate_completed": False,
                        "control_stop_exit_zero": True, "observed_stop_seconds": round(elapsed, 3)}
            second = pool.submit(request)
            if policy == "drain":
                # This is a bounded ordering observation, not a latency benchmark.
                time.sleep(0.2)
                require(upstream.count() == 1, "capacity released before first response body EOF")
                require(not second.done(), "second request completed while first body pending")
                require(not upstream.first_disconnected.is_set(), "default drain closed upstream")
                upstream.release.set()
                require(upstream.finished.wait(3), "first upstream did not reach body EOF")
            else:
                require(upstream.first_disconnected.wait(3), "close policy did not close upstream HTTP")
            status, response = second.result(timeout=5)
            require(status == 200 and response == EVENTS, "second response mismatch")
            require(upstream.count() == 2, "unexpected upstream attempt count")
            require(not upstream.fixture_errors, "fixture failed internally")
            require(request("stop")[0] == 200, "control stop failed")
            stdout, stderr = process.communicate(timeout=15)
            require(process.returncode == 0, "gateway stop did not exit zero")
            require(all(value not in stdout + stderr for value in
                        (data_token.encode(), control_token.encode(), BODY)), "fixture data reached output")
            return {"policy": policy, "disconnect_phase": "before_headers" if pre_headers else "after_terminal_marker",
                    "passed": True, "upstream_attempts": 2,
                    "upstream_close_observed": upstream.first_disconnected.is_set(),
                    "body_eof_gate_completed": upstream.finished.is_set(), "control_stop_exit_zero": True}
    finally:
        upstream.release.set()
        if first is not None:
            first.close()
        if process is not None and process.poll() is None:
            process.terminate()
            try:
                process.wait(timeout=12)
            except subprocess.TimeoutExpired:
                process.kill()
                process.wait(timeout=3)
        pool.shutdown(wait=True, cancel_futures=True)
        upstream.shutdown()
        upstream.server_close()
        upstream_thread.join(timeout=3)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--binary", type=Path, required=True)
    parser.add_argument("--output", type=Path, required=True)
    args = parser.parse_args()
    root = Path(__file__).resolve().parents[1]
    binary = args.binary.resolve(strict=True)
    binary_hash = hashlib.sha256(binary.read_bytes()).hexdigest()
    template = (root / "product/examples/fixture.toml").read_text()
    # Lifecycle checks have no quota wait; quota behavior has its own suite.
    quota_example = '[quota.rpm]\nkind = "known"\nvalue = 60'
    require(template.count(quota_example) == 1, "fixture RPM section changed")
    require('[quota.tpm]\nkind = "unknown"' in template, "fixture TPM section changed")
    template = template.replace(quota_example, '[quota.rpm]\nkind = "unlimited"')
    rows = []
    for policy, pre_headers, force_shutdown in (
        ("drain", False, False), ("close", False, False),
        ("drain", True, False), ("drain", False, True),
    ):
        try:
            rows.append(run_case(binary, template, policy, pre_headers, force_shutdown))
        except Exception as error:
            rows.append({"policy": policy, "disconnect_phase": "before_headers" if pre_headers else "after_terminal_marker",
                         "force_shutdown": force_shutdown,
                         "passed": False, "error_class": type(error).__name__,
                         "error": str(error) if isinstance(error, AssertionError) else "probe or transport failure"})
    unchanged = hashlib.sha256(binary.read_bytes()).hexdigest() == binary_hash
    result = {"check": "foreground_stream_lifetime_loopback_only",
              "timestamp": datetime.now(timezone.utc).isoformat(), "os": platform.platform(),
              "binary_sha256": binary_hash, "binary_unchanged_during_probe": unchanged,
              "passed": unchanged and all(row["passed"] for row in rows), "cases": rows,
              "quota_fixture": {"rpm": "unlimited", "tpm": "unknown"},
              "limitations": ["Synthetic HTTP, no actual LLM or agent task",
                              "RST cancellation; does not replace half-close, graceful-close or slow-reader tests",
                              "HTTP body/connection observations do not prove engine abort or quota refund",
                              "Forced process exit does not independently prove library child-task join ordering",
                              "No performance or cross-platform acceptance claim",
                              "Candidate port is released before process bind; startup is checked"]}
    args.output.parent.mkdir(parents=True, exist_ok=True)
    args.output.write_text(json.dumps(result, indent=2) + "\n")
    print(json.dumps({"passed": result["passed"], "cases": rows}))
    return 0 if result["passed"] else 1


if __name__ == "__main__":
    raise SystemExit(main())
