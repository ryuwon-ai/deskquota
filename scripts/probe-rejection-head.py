#!/usr/bin/env python3
"""Gated 429 head ordering on an existing binary; no performance ranking or API calls."""
import argparse
from concurrent.futures import ThreadPoolExecutor
from datetime import datetime, timezone
import hashlib
import http.client
import http.server
import json
from pathlib import Path
import subprocess
import sys
import tempfile
import threading
import time

ROOT = Path(__file__).resolve().parents[1]
sys.dont_write_bytecode = True
sys.path.insert(0, str(ROOT / "product/scripts"))
from measure_native import free_port, safe_environment

BODY = b'{"model":"example-model","messages":[]}'
TRANSIENT = b'{"error":{"code":"slow_down"}}'
PERMANENT = b'{"error":{"code":"insufficient_quota"}}'


def require(ok, reason):
    if not ok:
        raise AssertionError(reason)


def run_case(binary, name, *, direct=False, media="application/json", retry=False,
             timing=True, optimized=False):
    sent, release, head_ready = threading.Event(), threading.Event(), threading.Event()
    attempts, fixture_errors = [], []
    reply = TRANSIENT if not timing else PERMANENT

    class Handler(http.server.BaseHTTPRequestHandler):
        protocol_version = "HTTP/1.1"

        def log_message(self, *_args):
            pass

        def do_POST(self):
            try:
                require(self.rfile.read(int(self.headers["Content-Length"])) == BODY,
                        "request_body_changed")
                attempts.append(1)
                self.send_response(429)
                self.send_header("Content-Type", media)
                self.send_header("Content-Length", str(len(reply)))
                self.send_header("Connection", "close")
                if timing:
                    self.send_header("Retry-After", "2")
                self.end_headers()
                self.wfile.write(reply[:1])
                self.wfile.flush()
                sent.set()
                require(release.wait(5), "body_gate_deadline")
                self.wfile.write(reply[1:])
                self.wfile.flush()
            except Exception as error:
                fixture_errors.append(type(error).__name__)
            finally:
                self.close_connection = True

    upstream = http.server.ThreadingHTTPServer(("127.0.0.1", 0), Handler)
    thread = threading.Thread(target=upstream.serve_forever)
    thread.start()
    process = None
    try:
        with tempfile.TemporaryDirectory(prefix="deskquota-rejection-head-") as temporary:
            base = Path(temporary).resolve()
            port = upstream.server_port if direct else free_port()
            token = "synthetic-rejection-head-control"

            def control(path):
                connection = http.client.HTTPConnection("127.0.0.1", port, timeout=2)
                try:
                    connection.request("POST" if path == "stop" else "GET", f"/_llmgw/{path}",
                                       headers={"X-LLMGW-Control-Token": token, "Connection": "close"})
                    response = connection.getresponse()
                    require(response.status == 200, "control_status")
                    return json.loads(response.read())
                finally:
                    connection.close()

            if not direct:
                template = (ROOT / "product/examples/fixture.toml").read_text()
                template = f"retry_transient_429 = {str(retry).lower()}\n" + template
                template = template.replace('startup_hold_secs = 60', 'startup_hold_secs = 0')
                template = template.replace('127.0.0.1:4141', f'127.0.0.1:{port}')
                template = template.replace('127.0.0.1:18080', f'127.0.0.1:{upstream.server_port}')
                config = base / "config.toml"
                config.write_text(template)
                environment = safe_environment(base / "home")
                doctor = json.loads(subprocess.check_output(
                    [str(binary), "doctor", "--config", str(config), "--json"],
                    env=environment, timeout=10))
                state = Path(doctor["state_directory"])
                require(state.is_relative_to(base), "state_escaped_temporary_directory")
                state.mkdir(mode=0o700)
                secret = state / "control-token"
                secret.write_text(token)
                secret.chmod(0o600)
                process = subprocess.Popen([str(binary), "run", "--config", str(config)],
                                           stdout=subprocess.PIPE, stderr=subprocess.PIPE,
                                           env=environment)
                deadline = time.monotonic() + 5
                while True:
                    require(process.poll() is None, "gateway_startup_failed")
                    try:
                        control("health")
                        break
                    except ConnectionRefusedError:
                        require(time.monotonic() < deadline, "gateway_startup_deadline")
                        time.sleep(.01)

            def client():
                connection = http.client.HTTPConnection("127.0.0.1", port, timeout=5)
                try:
                    connection.request("POST", "/r/pi-work/v1/chat/completions", BODY,
                                       {"Content-Type": "application/json", "Connection": "close"})
                    response = connection.getresponse()
                    head_ready.set()
                    return response.status, response.read(), response.getheader("Retry-After")
                finally:
                    connection.close()

            with ThreadPoolExecutor(max_workers=1) as pool:
                pending = pool.submit(client)
                try:
                    require(sent.wait(3), "upstream_not_reached")
                    # Bounded ordering observation. The gate stays shut throughout;
                    # the interval is not a latency sample or an overhead estimate.
                    before_gate = head_ready.wait(.3)
                    require(not release.is_set(), "fixture_gate_already_open")
                finally:
                    release.set()
                status, returned, returned_timing = pending.result(timeout=5)
            require(status == 429 and returned == reply, "rejection_wire_changed")
            require(returned_timing == ("2" if timing else None), "retry_timing_changed")
            require(attempts == [1] and not fixture_errors, "fixture_attempt_or_handler_failure")
            shared_cooldown = None
            if not direct:
                shared_cooldown = control("status")["admission"]["shared_cooldown_ms"]
                require(shared_cooldown > 0, "missing_shared_cooldown")
                control("stop")
                stdout, stderr = process.communicate(timeout=5)
                require(process.returncode == 0, "gateway_stop_failed")
                require(token.encode() not in stdout + stderr and BODY not in stdout + stderr,
                        "fixture_data_in_process_output")
            should_stream = media != "application/json" or (timing and not retry)
            expected = direct or (optimized and should_stream)
            result = dict(name=name, direct=direct, retry=retry, media=media, timing=timing,
                          head_before_body_gate=before_gate, expected_head_before_gate=expected,
                          body_preserved=True, status=429, attempts=1,
                          shared_cooldown_ms=shared_cooldown, passed=before_gate == expected)
            return result
    finally:
        release.set()
        if process is not None and process.poll() is None:
            process.terminate()
            try:
                process.wait(timeout=3)
            except subprocess.TimeoutExpired:
                process.kill()
                process.wait(timeout=3)
        upstream.shutdown()
        upstream.server_close()
        thread.join(timeout=3)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--binary", required=True, type=Path)
    parser.add_argument("--output", required=True, type=Path)
    parser.add_argument("--expect", choices=("current", "stream-ineligible"), default="current")
    args = parser.parse_args()
    binary = args.binary.resolve(strict=True)
    result = dict(check="gated_rejection_head_ordering", timestamp=datetime.now(timezone.utc).isoformat(),
                  binary=str(binary), binary_sha256=hashlib.sha256(binary.read_bytes()).hexdigest(),
                  expectation=args.expect, cases=[], passed=False,
                  limitations=["Synthetic loopback ordering only; no provider or performance claim",
                               "One request per case; no throughput or general p95 measurement",
                               "Checked released candidate port; no socket activation"])
    try:
        plans = [dict(name="direct_json", direct=True),
                 dict(name="gateway_json_retry_off"),
                 dict(name="gateway_text_retry_off", media="text/plain"),
                 dict(name="gateway_text_retry_on", media="text/plain", retry=True),
                 dict(name="gateway_json_no_timing_retry_off", timing=False),
                 dict(name="gateway_json_retry_on", retry=True)]
        for plan in plans:
            result["cases"].append(run_case(binary, **plan, optimized=args.expect == "stream-ineligible"))
        result["passed"] = all(case["passed"] for case in result["cases"])
    except Exception as error:
        result["error_class"] = type(error).__name__
        if isinstance(error, AssertionError):
            result["check_failed"] = str(error)
    args.output.parent.mkdir(parents=True, exist_ok=True)
    args.output.write_text(json.dumps(result, indent=2) + "\n")
    print(json.dumps({"passed": result["passed"], "cases": result["cases"]}))
    return 0 if result["passed"] else 1


if __name__ == "__main__":
    raise SystemExit(main())
