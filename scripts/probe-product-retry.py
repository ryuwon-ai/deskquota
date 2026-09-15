#!/usr/bin/env python3
"""Observe retry attempts and shared cooldown in an owned native process on loopback."""

import argparse
from collections import Counter
from contextlib import contextmanager
from datetime import datetime, timezone
import gzip
import hashlib
import http.client
import http.server
import json
from pathlib import Path
import platform
import secrets
import socket
import subprocess
import tempfile
import threading
import time

ROOT = Path(__file__).resolve().parents[1]
BODY = b'{ "model": "example-model", "messages": [], "opaque": "retry-probe-sentinel" }'
SUCCESS = b'{"fixture":"complete"}'
TRANSIENT = b'{"error":{"type":"rate_limit_error","code":"slow_down"}}'
RATE = b'{"error":{"type":"tokens","code":"rate_limit_exceeded"}}'


class CheckFailed(Exception):
    """Only fixed check identifiers, never raw input or process output."""


def require(condition, identifier):
    if not condition:
        raise CheckFailed(identifier)


def wait_until(predicate, seconds=5):
    deadline = time.monotonic() + seconds
    while not predicate():
        require(time.monotonic() < deadline, "condition_deadline")
        time.sleep(0.005)


def reply(status=200, body=SUCCESS, headers=()):
    return status, body, list(headers)


class Upstream(http.server.ThreadingHTTPServer):
    daemon_threads = True

    def __init__(self, plans):
        super().__init__(("127.0.0.1", 0), Handler)
        self.plans = plans
        self.lock = threading.Lock()
        self.trace = []
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
            require(label in self.server.plans, "unexpected_request_label")
            received = self.rfile.read(int(self.headers.get("Content-Length", "0")))
            require(received == (b"" if self.command == "GET" else BODY), "body_changed")
            require(all(name not in self.headers for name in (
                "X-LLMGW-Token", "X-LLMGW-Control-Token", "Authorization", "x-api-key")),
                "local_or_none_auth_forwarded")
            endpoint = self.headers.get("X-Fixture-Endpoint")
            require(endpoint in ("chat/completions", "models", "messages/count_tokens"),
                    "fixture_endpoint")
            require(self.path == f"/team/v1/{endpoint}?api-version=fixture", "path_changed")
            with self.server.lock:
                index = sum(row["label"] == label for row in self.server.trace)
                self.server.trace.append({"label": label, "at": time.monotonic(),
                                          "endpoint": endpoint, "body_preserved": True})
                plan = self.server.plans[label]
                require(index < len(plan), "unexpected_extra_attempt")
                status, body, headers = plan[index]
            self.send_response(status)
            self.send_header("Content-Type", "application/json")
            declared_lengths = [value for name, value in headers
                                if name.lower() == "content-length"]
            require(len(declared_lengths) <= 1, "duplicate_fixture_content_length")
            if not declared_lengths:
                self.send_header("Content-Length", str(len(body)))
            self.send_header("Connection", "close")
            for name, value in headers:
                self.send_header(name, value)
            self.end_headers()
            self.wfile.write(body)
        except Exception as error:
            with self.server.lock:
                self.server.errors.append(str(error) if isinstance(error, CheckFailed)
                                          else type(error).__name__)
        finally:
            self.close_connection = True


def check_trace(trace, expected):
    require(Counter(row["label"] for row in trace) == Counter(expected),
            "missing_duplicate_or_extra_attempt")


class Runner:
    def __init__(self, binary, plans, opt_in):
        self.binary, self.plans, self.opt_in = binary, plans, opt_in
        self.temp = self.upstream = self.thread = self.process = None
        self.sockets = {}
        self.submitted = []
        self.received = []
        self.outcomes = []
        self.stopped = False

    def setup(self):
        self.temp = tempfile.TemporaryDirectory(prefix="llmgw-retry-probe-")
        base = Path(self.temp.name)
        self.upstream = Upstream(self.plans)
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
        # Omit the retry setting for the default-off case; explicitly set it only
        # for opt-in. Remove an example default if the product starts showing it.
        lines = text.splitlines()
        retry_lines = [line for line in lines if line.startswith("retry_transient_429 =")]
        require(len(retry_lines) <= 1, "example_retry_shape_changed")
        text = "\n".join(line for line in lines if line not in retry_lines) + "\n"
        if self.opt_in:
            text = "retry_transient_429 = true\n" + text
        text = text.split("[[roots]]", 1)[0]
        for root in ("retry-a", "retry-b"):
            text += (f'\n[[roots]]\nid = "{root}"\n'
                     'endpoints = ["chat/completions", "models", "messages/count_tokens"]\n'
                     'models = ["example-model"]\n')
        text = text.replace("127.0.0.1:4141", f"127.0.0.1:{self.port}")
        text = text.replace("127.0.0.1:18080", f"127.0.0.1:{self.upstream.server_port}")
        config = base / "fixture.toml"
        config.write_text(text)
        state = Path(json.loads(subprocess.check_output([str(self.binary), "doctor", "--config", str(config), "--json"], timeout=10))["state_directory"])
        state.mkdir(mode=0o700)
        self.control_token = secrets.token_hex(32)
        for name, value in (("control-token", self.control_token),):
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
            body = response.read(65537)
            require(len(body) <= 65536, "control_response_bound")
            return response.status, json.loads(body)
        finally:
            connection.close()

    def queued_without_slot(self, count):
        def observed():
            status, body = self.control("status")
            require(status == 200, "control_status")
            admission = body.get("admission", {})
            return admission.get("active") == 0 and admission.get("queue_length") == count
        wait_until(observed, 0.5)

    def send(self, label, root="retry-a", endpoint="chat/completions"):
        require(label not in self.sockets and label not in self.received, "duplicate_ingress_label")
        method = "GET" if endpoint == "models" else "POST"
        body = b"" if method == "GET" else BODY
        sock = socket.create_connection(("127.0.0.1", self.port), timeout=3)
        self.sockets[label] = sock
        self.submitted.append(label)
        head = (f"{method} /r/{root}/v1/{endpoint} HTTP/1.1\r\nHost: 127.0.0.1:{self.port}\r\n"
                f"X-Fixture-Request: {label}\r\n"
                f"X-Fixture-Endpoint: {endpoint}\r\nX-Session-Id: child-{label}\r\n"
                f"Content-Type: application/json\r\nContent-Length: {len(body)}\r\n"
                "Connection: close\r\n\r\n").encode()
        sock.sendall(head + body)

    def receive(self, label, expected, *, gateway_generated=False):
        sock = self.sockets.pop(label)
        response = None
        try:
            sock.settimeout(5)
            response = http.client.HTTPResponse(sock)
            response.begin()
            body = response.read(131073)
            status, payload, headers = expected
            require(len(body) <= 131072, "data_response_bound")
            require(response.status == status and body == payload, "raw_response_mismatch")
            for name, value in headers:
                require((name.lower(), value) in [(k.lower(), v) for k, v in response.getheaders()],
                        "response_header_changed")
            self.received.append(label)
            self.outcomes.append({"label": label, "http_status": response.status,
                                  "body_matches_expected": True,
                                  "body_origin": "gateway_error" if gateway_generated else "upstream"})
        finally:
            if response is not None:
                response.close()
            sock.close()

    def finish(self, expected_attempts):
        require(not self.sockets, "unaccounted_ingress_socket")
        require(self.control("stop")[0] == 200, "control_stop_rejected")
        stdout, stderr = self.process.communicate(timeout=12)
        require(self.process.returncode == 0, "control_stop_exit")
        self.stopped = True
        require(not self.upstream.errors, "upstream_fixture_error")
        trace = self.upstream.recorded()
        check_trace(trace, expected_attempts)
        require(all(value not in stdout + stderr for value in
                    (self.control_token.encode(), BODY,
                     b"retry-probe-sentinel")), "fixture_material_in_output")
        origin = trace[0]["at"]
        return {"ingress": len(self.received), "upstream_attempts": len(trace),
                "opt_in": self.opt_in, "control_stop_exit_zero": True,
                "no_fixture_material_in_output": True,
                "outcomes": self.outcomes,
                "trace": [{**row, "at": round(row["at"] - origin, 6)} for row in trace]}

    def close(self):
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
def gateway(binary, plans, opt_in):
    runner = Runner(binary, plans, opt_in)
    try:
        runner.setup()
        yield runner
    except Exception as error:
        safe = error if isinstance(error, CheckFailed) else CheckFailed(type(error).__name__)
        safe.observation = {"upstream_attempt_labels":
                            [row["label"] for row in runner.upstream.recorded()]
                            if runner.upstream is not None else [],
                            "submitted_ingress": list(runner.submitted),
                            "completed_ingress": list(runner.received),
                            "unreceived_ingress": [label for label in runner.submitted
                                                   if label not in runner.received]}
        raise safe from None
    finally:
        runner.close()


def default_off(binary):
    rejection = reply(429, TRANSIENT, [("Retry-After", "1")])
    with gateway(binary, {"A1": [rejection], "B1": [reply()], "A2": [reply()]}, False) as run:
        run.send("A1")
        run.receive("A1", rejection)
        run.send("B1", "retry-b", "models")
        run.send("A2", "retry-a", "messages/count_tokens")
        run.queued_without_slot(2)
        require(len(run.upstream.recorded()) == 1, "cooldown_bypassed_by_new_client")
        run.receive("B1", reply())
        run.receive("A2", reply())
        trace = run.upstream.recorded()
        require(all(row["at"] - trace[0]["at"] >= 0.99 for row in trace[1:]),
                "standard_seconds_wait_shortened")
        return run.finish(["A1", "B1", "A2"])


def opt_in_shared_ms(binary):
    rejection = reply(429, RATE, [("retry-after-ms", "800")])
    with gateway(binary, {"A1": [rejection, reply()], "B1": [reply()]}, True) as run:
        run.send("A1")
        wait_until(lambda: len(run.upstream.recorded()) == 1)
        run.queued_without_slot(1)
        run.send("B1", "retry-b", "models")
        run.queued_without_slot(2)
        run.receive("A1", reply())
        run.receive("B1", reply())
        trace = run.upstream.recorded()
        require(all(row["at"] - trace[0]["at"] >= 0.79 for row in trace[1:]),
                "millisecond_wait_shortened")
        return run.finish(["A1", "A1", "B1"])


def maximum_wait(binary):
    headers = [("Retry-After", "0"), ("Retry-After", "1"), ("retry-after-ms", "50")]
    with gateway(binary, {"A1": [reply(429, TRANSIENT, headers), reply()]}, True) as run:
        run.send("A1")
        run.receive("A1", reply())
        trace = run.upstream.recorded()
        require(len(trace) == 2 and trace[1]["at"] - trace[0]["at"] >= 0.99,
                "maximum_valid_header_wait_shortened")
        return run.finish(["A1", "A1"])


def no_replay(binary):
    cases = {
        "permanent": reply(429, b'{"error":{"type":"rate_limit_error","code":"insufficient_quota"}}',
                           [("Retry-After", "0")]),
        "spenddetail": reply(429, b'{"error":{"code":"slow_down","details":{"error_code":"enforced_spend_limit_reached"}}}'),
        "duplicate": reply(429, b'{"error":{"code":"insufficient_quota","code":"slow_down"}}'),
        "unknown": reply(429, b'{"error":{"type":"rate_limit_error"}}'),
        "malformed": reply(429, b'{"error":{"code":"slow_down"}'),
        "invalidutf8": reply(429, b'{"error":{"code":"slow_down"},"padding":"\xff"}'),
        "surrogate": reply(429, b'{"error":{"code":"slow_down"},"padding":"\\ud800"}'),
        "encoded": reply(429, gzip.compress(TRANSIENT, mtime=0), [("Content-Encoding", "gzip")]),
        "oversized": reply(429, b'{"error":{"code":"slow_down"},"padding":"' + b'x' * 65536 + b'"}'),
        "unavailable": reply(503, TRANSIENT),
    }
    with gateway(binary, {key: [value] for key, value in cases.items()}, True) as run:
        for label, expected in cases.items():
            run.send(label)
            run.receive(label, expected)
        return run.finish(list(cases))


def at_most_one(binary):
    rejection = reply(429, TRANSIENT)
    with gateway(binary, {"A1": [rejection, rejection]}, True) as run:
        run.send("A1")
        run.receive("A1", rejection)
        trace = run.upstream.recorded()
        require(len(trace) == 2 and trace[1]["at"] - trace[0]["at"] >= 0.99,
                "fallback_wait_or_attempt_bound")
        return run.finish(["A1", "A1"])


def prehead_transport_failure(binary):
    variants = []
    for opt_in in (False, True):
        # An incomplete HTTP body is a transport error, even though the bytes
        # received so far form valid retryable JSON. Its timing header still
        # applies to the other root. No downstream head has been sent yet.
        truncated = reply(429, TRANSIENT, [("Retry-After", "1"),
                                            ("Content-Length", "1000")])
        error = reply(502, b'{"error":{"code":"upstream_transport_error"}}')
        with gateway(binary, {"A1": [truncated], "B1": [reply()]}, opt_in) as run:
            run.send("A1")
            run.receive("A1", error, gateway_generated=True)
            run.send("B1", "retry-b", "models")
            run.queued_without_slot(1)
            require(len(run.upstream.recorded()) == 1, "truncated_429_retried_or_cooldown_lost")
            run.receive("B1", reply())
            trace = run.upstream.recorded()
            require(trace[1]["at"] - trace[0]["at"] >= 0.99,
                    "truncated_429_cooldown_shortened")
            status, body = run.control("status")
            require(status == 200 and body["admission"]["active"] == 0
                    and body["admission"]["queue_length"] == 0,
                    "truncated_429_resources_not_released")
            variants.append(run.finish(["A1", "B1"]))
    return {"ingress": sum(row["ingress"] for row in variants),
            "upstream_attempts": sum(row["upstream_attempts"] for row in variants),
            "variants": variants}


def self_check():
    upstream = Upstream({"A1": [reply(429, TRANSIENT), reply()],
                         "T1": [reply(429, TRANSIENT, [("Content-Length", "1000")])]})
    thread = threading.Thread(target=upstream.serve_forever, daemon=True)
    thread.start()
    try:
        for expected in (reply(429, TRANSIENT), reply()):
            connection = http.client.HTTPConnection("127.0.0.1", upstream.server_port, timeout=2)
            try:
                connection.request("POST", "/team/v1/chat/completions?api-version=fixture", BODY,
                                   {"X-Fixture-Request": "A1", "X-Fixture-Endpoint": "chat/completions"})
                response = connection.getresponse()
                require((response.status, response.read()) == expected[:2], "fixture_sequence")
            finally:
                connection.close()
        check_trace(upstream.recorded(), ["A1", "A1"])
        rejected = 0
        for bad in ([], [{"label": "A1"}], [{"label": "A1"}] * 3,
                    [{"label": "A1"}, {"label": "B1"}]):
            try:
                check_trace(bad, ["A1", "A1"])
            except CheckFailed:
                rejected += 1
        require(rejected == 4 and not upstream.errors, "fixture_negative_detection")
        connection = http.client.HTTPConnection("127.0.0.1", upstream.server_port, timeout=2)
        try:
            connection.request("POST", "/team/v1/chat/completions?api-version=fixture", BODY,
                               {"X-Fixture-Request": "T1", "X-Fixture-Endpoint": "chat/completions"})
            response = connection.getresponse()
            require(response.status == 429, "truncated_fixture_status")
            try:
                response.read()
            except http.client.IncompleteRead as error:
                require(error.partial == TRANSIENT, "truncated_fixture_partial_body")
            else:
                raise CheckFailed("truncated_fixture_did_not_truncate")
        finally:
            connection.close()
        check_trace(upstream.recorded(), ["A1", "A1", "T1"])
        require(not upstream.errors, "truncated_fixture_error")
        return {"passed": True, "gateway_executed": False, "fixture_attempts": 3,
                "checks": ["scripted_429_then_200", "body_and_path_validation",
                           "missing_duplicate_extra_or_wrong_label_rejected",
                           "truncated_body_is_not_complete_json_transport"]}
    finally:
        upstream.shutdown()
        upstream.server_close()
        thread.join(timeout=3)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--binary", type=Path)
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument("--self-check", action="store_true")
    args = parser.parse_args()
    require(args.self_check or args.binary is not None, "binary_required")
    require(not args.output.exists(), "output_already_exists")
    result = {"check": "retry_fixture_self_check" if args.self_check else "native_retry_loopback",
              "timestamp": datetime.now(timezone.utc).isoformat(),
              "script_sha256": hashlib.sha256(Path(__file__).read_bytes()).hexdigest(),
              "os": platform.platform(), "python": platform.python_version(), "passed": False}
    try:
        if args.self_check:
            result.update(self_check())
        else:
            binary = args.binary.resolve(strict=True)
            before = hashlib.sha256(binary.read_bytes()).hexdigest()
            result.update(binary=str(binary), binary_sha256=before, gateway_executed=True,
                          quota_fixture={"rpm": "unlimited", "tpm": "unknown"}, cases=[])
            for case in (default_off, opt_in_shared_ms, maximum_wait, no_replay, at_most_one,
                         prehead_transport_failure):
                row = {"case": case.__name__, "passed": False}
                result["cases"].append(row)
                row.update(case(binary), passed=True)
            require(before == hashlib.sha256(binary.read_bytes()).hexdigest(), "binary_changed")
            result["passed"] = True
            result["limitations"] = [
                "Synthetic HTTP fixture; no actual provider, agent or LLM task",
                "Unlimited RPM isolates transport; no known-quota accounting assertion",
                "Observed minimum waits are correctness checks, not latency or throughput benchmarks",
                "No HTTP-date, cancel/stop race, resource-bound or cross-OS coverage in this probe",
                "Candidate port is checked by child startup, not socket activation",
            ]
    except Exception as error:
        result["error"] = str(error) if isinstance(error, CheckFailed) else type(error).__name__
        if hasattr(error, "observation"):
            result["failure_observation"] = error.observation
    args.output.parent.mkdir(parents=True, exist_ok=True)
    with args.output.open("x") as output:
        output.write(json.dumps(result, ensure_ascii=False, indent=2) + "\n")
    print(json.dumps({"passed": result["passed"], "output": str(args.output),
                      "error": result.get("error")}))
    return 0 if result["passed"] else 1


if __name__ == "__main__":
    raise SystemExit(main())
