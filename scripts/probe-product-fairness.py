#!/usr/bin/env python3
"""Check native gateway root ordering, queued cancellation and overflow on loopback."""

import argparse
from contextlib import contextmanager
from datetime import datetime, timezone
import hashlib
import http.client
import http.server
import json
from pathlib import Path
import platform
import secrets
import select
import socket
import struct
import subprocess
import tempfile
import threading
import time


ROOT = Path(__file__).resolve().parents[1]
BODY = b'{"model":"example-model","messages":[]}'
REPLY = b'{"fixture":"complete"}'


class CheckFailed(Exception):
    """Only fixed, safe check identifiers may be stored in this exception."""


def require(condition, reason):
    if not condition:
        raise CheckFailed(reason)


def wait_until(predicate, seconds=5):
    deadline = time.monotonic() + seconds
    while not predicate():
        require(time.monotonic() < deadline, "condition_deadline")
        time.sleep(0.005)


class Upstream(http.server.ThreadingHTTPServer):
    daemon_threads = True

    def __init__(self):
        super().__init__(("127.0.0.1", 0), Handler)
        self.gate = threading.Event()
        self.started = threading.Event()
        self.lock = threading.Lock()
        self.trace = []
        self.last_attempt_at = None
        self.errors = []

    def recorded(self):
        with self.lock:
            return list(self.trace)


class Handler(http.server.BaseHTTPRequestHandler):
    protocol_version = "HTTP/1.1"

    def log_message(self, *_args):
        pass

    def do_GET(self):
        self.handle_fixture()

    def do_POST(self):
        self.handle_fixture()

    def handle_fixture(self):
        try:
            label = self.headers.get("X-Fixture-Request", "")
            require(label and len(label) <= 16 and label.isalnum(), "fixture_label")
            payload = self.rfile.read(int(self.headers.get("Content-Length", "0")))
            require(payload == (b"" if self.command == "GET" else BODY), "body_changed")
            require(all(name not in self.headers for name in (
                "X-LLMGW-Token", "X-LLMGW-Control-Token", "Authorization", "x-api-key")),
                "local_or_none_auth_forwarded")
            expected = {
                "chat": "/team/v1/chat/completions?api-version=fixture",
                "models": "/team/v1/models?api-version=fixture",
                "count": "/team/v1/messages/count_tokens?api-version=fixture",
            }
            require(self.path == expected[self.headers["X-Fixture-Endpoint"]], "upstream_path")
            with self.server.lock:
                self.server.trace.append(label)
                self.server.last_attempt_at = time.monotonic()
            if label == "A0":
                self.server.started.set()
                require(self.server.gate.wait(30), "upstream_gate_deadline")
            self.send_response(200)
            self.send_header("Content-Type", "application/json")
            self.send_header("Content-Length", str(len(REPLY)))
            self.send_header("Connection", "close")
            self.end_headers()
            self.wfile.write(REPLY)
        except Exception as error:
            with self.server.lock:
                self.server.errors.append(
                    str(error) if isinstance(error, CheckFailed) else type(error).__name__)
        finally:
            self.close_connection = True


def receive(sock, deadline):
    response = None
    try:
        sock.settimeout(max(0.001, deadline - time.monotonic()))
        response = http.client.HTTPResponse(sock)
        response.begin()
        body = response.read(65537)
        require(len(body) <= 65536, "response_bound")
        return response.status, body
    finally:
        if response is not None:
            response.close()
        sock.close()


def check_attempts(trace, expected_labels):
    require(len(trace) == len(expected_labels) and set(trace) == set(expected_labels),
            "lost_duplicate_or_unexpected_attempt")


class Runner:
    def __init__(self, binary):
        self.binary = binary
        self.temp = self.upstream = self.thread = self.process = None
        self.sockets = {}
        self.control_stopped = False

    def setup(self):
        self.temp = tempfile.TemporaryDirectory(prefix="llmgw-fairness-probe-")
        base = Path(self.temp.name)
        self.upstream = Upstream()
        self.thread = threading.Thread(target=self.upstream.serve_forever, daemon=True)
        self.thread.start()
        with socket.socket() as candidate:
            candidate.bind(("127.0.0.1", 0))
            self.port = candidate.getsockname()[1]
        text = (ROOT / "product/examples/fixture.toml").read_text()
        quota = '[quota.rpm]\nkind = "known"\nvalue = 60'
        require(text.count(quota) == 1 and '[quota.tpm]\nkind = "unknown"' in text,
                "example_quota_shape_changed")
        require(text.count("[[roots]]") == 1, "example_root_shape_changed")
        text = text.replace(quota, '[quota.rpm]\nkind = "unlimited"')
        text = text.split("[[roots]]", 1)[0]
        for root in ("fifo-a", "fifo-b"):
            text += (f'\n[[roots]]\nid = "{root}"\n'
                     'endpoints = ["chat/completions", "models", "messages/count_tokens"]\n'
                     'models = ["example-model"]\n')
        text = text.replace("127.0.0.1:4141", f"127.0.0.1:{self.port}")
        text = text.replace("127.0.0.1:18080", f"127.0.0.1:{self.upstream.server_port}")
        config = base / "fixture.toml"
        config.write_text(text)
        state = Path(json.loads(subprocess.check_output([str(self.binary), "doctor", "--config", str(config), "--json"], timeout=10))["state_directory"])
        state.mkdir(mode=0o700)
        self.data_token, self.control_token = secrets.token_hex(32), secrets.token_hex(32)
        for name, value in (("data-token", self.data_token), ("control-token", self.control_token)):
            path = state / name
            path.write_text(value)
            path.chmod(0o600)
        self.process = subprocess.Popen([str(self.binary), "run", "--config", str(config)],
                                        stdout=subprocess.PIPE, stderr=subprocess.PIPE)

        def healthy():
            require(self.process.poll() is None, "gateway_exited_before_health")
            try:
                return self.control("health")[0] == 200
            except ConnectionRefusedError:
                return False

        wait_until(healthy, 10)

    def control(self, path):
        connection = http.client.HTTPConnection("127.0.0.1", self.port, timeout=2)
        try:
            connection.request("POST" if path == "stop" else "GET", f"/_llmgw/{path}",
                               headers={"X-LLMGW-Control-Token": self.control_token,
                                        "Connection": "close"})
            response = connection.getresponse()
            return response.status, json.loads(response.read())
        finally:
            connection.close()

    def queued(self, count):
        def matched():
            status, data = self.control("status")
            require(status == 200, "status_http")
            admission = data.get("admission")
            require(isinstance(admission, dict), "admission_status_missing")
            roots = admission.get("roots", [])
            require([root["id"] for root in roots] == ["fifo-a", "fifo-b"], "root_set_changed")
            require(sum(root["queue_length"] for root in roots) == admission["queue_length"],
                    "root_queue_sum")
            require(admission["active"] == 1, "gate_did_not_retain_active_slot")
            return admission["queue_length"] == count
        wait_until(matched)

    def send(self, label, root, endpoint="chat"):
        path, method = {
            "chat": ("chat/completions", "POST"),
            "models": ("models", "GET"),
            "count": ("messages/count_tokens", "POST"),
        }[endpoint]
        body = b"" if method == "GET" else BODY
        sock = socket.create_connection(("127.0.0.1", self.port), timeout=3)
        self.sockets[label] = sock
        head = (f"{method} /r/{root}/v1/{path} HTTP/1.1\r\nHost: 127.0.0.1:{self.port}\r\n"
                f"X-LLMGW-Token: {self.data_token}\r\nX-Fixture-Request: {label}\r\n"
                f"X-Fixture-Endpoint: {endpoint}\r\nX-Session-Id: child-{label}\r\n"
                f"Content-Type: application/json\r\nContent-Length: {len(body)}\r\n"
                "Connection: close\r\n\r\n").encode()
        sock.sendall(head + body)

    def reset(self, label):
        sock = self.sockets.pop(label)
        sock.setsockopt(socket.SOL_SOCKET, socket.SO_LINGER, struct.pack("ii", 1, 0))
        sock.close()

    def first(self):
        self.send("A0", "fifo-a")
        require(self.upstream.started.wait(3), "first_upstream_missing")
        self.queued(0)

    def finish(self, expected_labels):
        require(self.upstream.recorded() == ["A0"], "queued_request_started_before_gate")
        self.upstream.gate.set()
        deadline = time.monotonic() + 12
        for label in expected_labels:
            status, body = receive(self.sockets.pop(label), deadline)
            require(status == 200 and body == REPLY, "completed_response_mismatch")
        trace = self.upstream.recorded()
        check_attempts(trace, expected_labels)
        require(not self.upstream.errors, "upstream_fixture_error")
        require(not self.sockets, "unaccounted_client_socket")
        require(self.control("stop")[0] == 200, "control_stop_rejected")
        stdout, stderr = self.process.communicate(timeout=15)
        require(self.process.returncode == 0, "control_stop_exit")
        require(all(value not in stdout + stderr for value in
                    (self.data_token.encode(), self.control_token.encode(), BODY)),
                "fixture_material_in_output")
        self.control_stopped = True
        return trace

    def close(self):
        if self.upstream is not None:
            self.upstream.gate.set()
        for sock in self.sockets.values():
            sock.close()
        if self.process is not None and self.process.poll() is None:
            self.process.terminate()
            try:
                self.process.communicate(timeout=12)
            except subprocess.TimeoutExpired:
                self.process.kill()
                self.process.communicate(timeout=5)
        if self.process is not None:
            for pipe in (self.process.stdout, self.process.stderr):
                if pipe is not None:
                    pipe.close()
        if self.upstream is not None:
            self.upstream.shutdown()
            self.upstream.server_close()
        if self.thread is not None:
            self.thread.join(timeout=3)
        if self.temp is not None:
            self.temp.cleanup()


@contextmanager
def gateway(binary):
    runner = Runner(binary)
    try:
        runner.setup()
        yield runner
    except Exception as error:
        # Preserve a bounded failure observation, never raw requests or output.
        safe = error if isinstance(error, CheckFailed) else CheckFailed(type(error).__name__)
        safe.observation = {"upstream_trace_before_cleanup": runner.upstream.recorded()
                            if runner.upstream is not None else [],
                            "owned_client_sockets_before_cleanup": len(runner.sockets),
                            "control_stopped_before_cleanup": runner.control_stopped}
        raise safe from None
    finally:
        runner.close()


def round_robin(binary):
    with gateway(binary) as runner:
        runner.first()
        started = time.monotonic()
        pending = [("A1", "fifo-a", "chat"), ("A2", "fifo-a", "chat"),
                   ("B1", "fifo-b", "models"), ("A3", "fifo-a", "chat"),
                   ("B2", "fifo-b", "count")]
        for count, request in enumerate(pending, 1):
            runner.send(*request)
            runner.queued(count)
        require(time.monotonic() - started < 4, "fixture_crossed_aging_threshold")
        trace = runner.finish(["A0"] + [item[0] for item in pending])
        require(runner.upstream.last_attempt_at - started < 4,
                "fixture_crossed_aging_threshold")
        require(trace == ["A0", "B1", "A1", "B2", "A2", "A3"], "root_rr_or_fifo_order")
        return {"case": "root_rr_fifo_and_metadata", "passed": True,
                "ingress": 6, "upstream_attempts": len(trace), "trace": trace}


def queued_cancel(binary):
    with gateway(binary) as runner:
        runner.first()
        started = time.monotonic()
        for count, (label, root) in enumerate((("A1", "fifo-a"), ("A2", "fifo-a"),
                                                ("B1", "fifo-b")), 1):
            runner.send(label, root)
            runner.queued(count)
        runner.reset("A1")
        runner.queued(2)
        require(time.monotonic() - started < 4, "fixture_crossed_aging_threshold")
        trace = runner.finish(["A0", "A2", "B1"])
        require(runner.upstream.last_attempt_at - started < 4,
                "fixture_crossed_aging_threshold")
        require(trace == ["A0", "B1", "A2"], "canceled_head_or_root_order")
        return {"case": "queued_rst", "passed": True, "ingress": 4,
                "canceled_before_upstream": 1, "upstream_attempts": len(trace), "trace": trace}


def overflow(binary):
    with gateway(binary) as runner:
        runner.first()
        labels = []
        for index in range(1, 33):
            for prefix, root in (("A", "fifo-a"), ("B", "fifo-b")):
                label = f"{prefix}{index}"
                runner.send(label, root)
                labels.append(label)
                # FIFO begins at enqueue, not client send; serialize observation.
                runner.queued(len(labels))
        runner.queued(64)
        runner.send("Overflow", "fifo-b")
        status, body = receive(runner.sockets.pop("Overflow"), time.monotonic() + 3)
        require(status == 429, "queue_overflow_not_rejected")
        require(json.loads(body).get("error", {}).get("code") == "gateway_queue_full",
                "queue_overflow_error_code")
        runner.queued(64)
        canceled = labels[:16]
        for label in canceled:
            runner.reset(label)
        runner.queued(48)
        runner.send("B33", "fifo-b")
        runner.queued(49)
        trace = runner.finish(["A0"] + labels[16:] + ["B33"])
        require([label for label in trace if label.startswith("A")] ==
                ["A0"] + [f"A{i}" for i in range(9, 33)], "overflow_root_a_fifo")
        require([label for label in trace if label.startswith("B")] ==
                [f"B{i}" for i in range(9, 34)], "overflow_root_b_fifo")
        return {"case": "shared_queue64_cancel_and_refill", "passed": True, "ingress": 67,
                "rejected": 1, "canceled_before_upstream": 16,
                "upstream_attempts": len(trace), "all_outcomes_accounted": len(trace) + 17 == 67}


def self_check(output=None):
    expected = ["A0", "B1", "A1"]
    check_attempts(expected, expected)
    rejected = 0
    for malformed in (["A0", "A1"], ["A0", "B1", "B1"], ["A0", "B1", "A1", "Extra"]):
        try:
            check_attempts(malformed, expected)
        except CheckFailed:
            rejected += 1
    require(rejected == 3, "harness_did_not_reject_bad_attempt_sets")
    upstream = Upstream()
    thread = threading.Thread(target=upstream.serve_forever, daemon=True)
    thread.start()
    sock = None
    try:
        sock = socket.create_connection(("127.0.0.1", upstream.server_port), timeout=3)
        sock.sendall((f"POST /team/v1/chat/completions?api-version=fixture HTTP/1.1\r\n"
                      f"Host: 127.0.0.1:{upstream.server_port}\r\nX-Fixture-Request: A0\r\n"
                      f"X-Fixture-Endpoint: chat\r\nContent-Length: {len(BODY)}\r\n"
                      "Connection: close\r\n\r\n").encode() + BODY)
        require(upstream.started.wait(3), "self_check_request_not_observed")
        require(not select.select([sock], [], [], 0.02)[0], "self_check_gate_did_not_hold")
        upstream.gate.set()
        status, body = receive(sock, time.monotonic() + 3)
        sock = None
        require(status == 200 and body == REPLY, "self_check_response")
        require(upstream.recorded() == ["A0"] and not upstream.errors, "self_check_fixture")
    finally:
        upstream.gate.set()
        if sock is not None:
            sock.close()
        upstream.shutdown()
        upstream.server_close()
        thread.join(timeout=3)
    result = {"passed": True, "scope": "fixture_and_attempt_validation_only",
              "malformed_attempt_sets_rejected": rejected, "gate_held_until_released": True,
              "gateway_executed": False, "timestamp": datetime.now(timezone.utc).isoformat(),
              "script_sha256": hashlib.sha256(Path(__file__).read_bytes()).hexdigest()}
    if output is not None:
        output.parent.mkdir(parents=True, exist_ok=True)
        output.write_text(json.dumps(result, indent=2) + "\n")
    print(json.dumps(result))
    return 0


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--binary", type=Path)
    parser.add_argument("--output", type=Path)
    parser.add_argument("--self-check", action="store_true")
    args = parser.parse_args()
    if args.self_check:
        return self_check(args.output)
    if args.binary is None or args.output is None:
        parser.error("--binary and --output are required for a product probe")
    binary = args.binary.resolve(strict=True)
    digest = hashlib.sha256(binary.read_bytes()).hexdigest()
    results = []
    for check in (round_robin, queued_cancel, overflow):
        try:
            results.append(check(binary))
        except Exception as error:
            results.append({"case": check.__name__, "passed": False,
                            "failure": str(error) if isinstance(error, CheckFailed)
                            else type(error).__name__,
                            "observation": getattr(error, "observation", None)})
    unchanged = hashlib.sha256(binary.read_bytes()).hexdigest() == digest
    passed = unchanged and all(item["passed"] for item in results)
    result = {"check": "foreground_fair_admission_loopback_only", "passed": passed,
              "timestamp": datetime.now(timezone.utc).isoformat(), "binary": str(binary),
              "binary_sha256": digest, "binary_unchanged": unchanged, "os": platform.platform(),
              "script_sha256": hashlib.sha256(Path(__file__).read_bytes()).hexdigest(),
              "quota_fixture": {"rpm": "unlimited", "tpm": "unknown"}, "cases": results,
              "limitations": ["Synthetic native HTTP, no real agent or model",
                              "No token-budget, bypass-barrier, latency, RSS or Windows proof",
                              "RR setup and all attempt starts must precede the5-second aging threshold",
                              "Candidate port is released before spawn; bind failure fails the probe"]}
    args.output.parent.mkdir(parents=True, exist_ok=True)
    args.output.write_text(json.dumps(result, indent=2) + "\n")
    print(json.dumps({"passed": passed, "cases": results}))
    return 0 if passed else 1


if __name__ == "__main__":
    raise SystemExit(main())
