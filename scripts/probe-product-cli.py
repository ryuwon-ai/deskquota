#!/usr/bin/env python3
"""Exercise the foreground product binary with temporary state and loopback only."""

import argparse
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
from datetime import datetime, timezone


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--binary", type=Path, required=True)
    parser.add_argument("--output", type=Path, required=True)
    args = parser.parse_args()
    binary = args.binary.resolve(strict=True)
    root = Path(__file__).resolve().parents[1]
    body = b'{ "model": "example-model", "messages": [], "opaque": {"preserve": true} }'
    captured = []

    class Fixture(http.server.BaseHTTPRequestHandler):
        def do_POST(self):
            received = self.rfile.read(int(self.headers["Content-Length"]))
            captured.append({
                "path": self.path,
                "body_preserved": received == body,
                "local_headers_absent": all(
                    name not in self.headers
                    for name in ("X-LLMGW-Token", "X-LLMGW-Control-Token")
                ),
                "none_auth_absent": all(
                    name not in self.headers for name in ("Authorization", "x-api-key")
                ),
            })
            response = b'{"fixture":"complete"}'
            self.send_response(200)
            self.send_header("Content-Type", "application/json")
            self.send_header("Content-Length", str(len(response)))
            self.end_headers()
            self.wfile.write(response)

        def log_message(self, *_args):
            pass

    upstream = http.server.ThreadingHTTPServer(("127.0.0.1", 0), Fixture)
    thread = threading.Thread(target=upstream.serve_forever, daemon=True)
    thread.start()
    process = None
    try:
        with tempfile.TemporaryDirectory(prefix="llmgw-cli-probe-") as temporary:
            base = Path(temporary)
            with socket.socket() as reservation:
                reservation.bind(("127.0.0.1", 0))
                port = reservation.getsockname()[1]
            # run has no port-reporting interface yet; reserve then release a
            # candidate port and fail the probe if the spawned process cannot bind.
            template = (root / "product/examples/fixture.toml").read_text()
            # This probe isolates transport from the known-quota restart hold.
            quota_example = '[quota.rpm]\nkind = "known"\nvalue = 60'
            assert template.count(quota_example) == 1, "fixture RPM section changed"
            assert '[quota.tpm]\nkind = "unknown"' in template, "fixture TPM section changed"
            template = template.replace(quota_example, '[quota.rpm]\nkind = "unlimited"')
            template = template.replace("127.0.0.1:4141", f"127.0.0.1:{port}")
            template = template.replace("127.0.0.1:18080", f"127.0.0.1:{upstream.server_port}")
            config = base / "fixture.toml"
            config.write_text(template)
            command = [str(binary), "run", "--config", str(config)]
            state = Path(json.loads(subprocess.check_output([str(binary), "doctor", "--config", str(config), "--json"], timeout=10))["state_directory"])
            state.mkdir(mode=0o700)
            data_token, control_token = secrets.token_hex(32), secrets.token_hex(32)
            for name, value in (("data-token", data_token), ("control-token", control_token)):
                target = state / name
                target.write_text(value)
                target.chmod(0o600)
            process = subprocess.Popen(command, stdout=subprocess.PIPE, stderr=subprocess.PIPE)

            def request(method, path, token, payload=None, extra=None):
                connection = http.client.HTTPConnection("127.0.0.1", port, timeout=2)
                try:
                    header = "X-LLMGW-Control-Token" if path.startswith("/_llmgw/") else "X-LLMGW-Token"
                    headers = {header: token, "Connection": "close", **(extra or {})}
                    connection.request(method, path, payload, headers)
                    response = connection.getresponse()
                    return response.status, response.read()
                finally:
                    connection.close()

            deadline = time.monotonic() + 10
            while True:
                assert process.poll() is None, "foreground process exited before health"
                try:
                    status, response = request("GET", "/_llmgw/health", control_token)
                    assert status == 200 and json.loads(response)["status"] == "ok"
                    break
                except ConnectionRefusedError:
                    assert time.monotonic() < deadline, "startup deadline exceeded"
                    time.sleep(0.02)
            status, _ = request("GET", "/_llmgw/status", data_token)
            assert status == 401, "data credential authorized control"
            status, response = request(
                "POST", "/r/pi-work/v1/chat/completions?request-key=a%2Bb", data_token, body,
                {"Content-Type": "application/json", "Authorization": "synthetic-old",
                 "x-api-key": "synthetic-old", "X-LLMGW-Control-Token": control_token},
            )
            assert status == 200 and json.loads(response) == {"fixture": "complete"}
            assert captured == [{
                "path": "/team/v1/chat/completions?api-version=fixture&request-key=a%2Bb",
                "body_preserved": True, "local_headers_absent": True, "none_auth_absent": True,
            }], "upstream wire contract mismatch"
            status, response = request("POST", "/_llmgw/stop", control_token)
            assert status == 200 and json.loads(response)["status"] == "stopping"
            stdout, stderr = process.communicate(timeout=15)
            assert process.returncode == 0, "control stop did not exit cleanly"
            for value in (data_token.encode(), control_token.encode(), body):
                assert value not in stdout + stderr, "sensitive fixture material reached output"
            result = {
                "check": "actual_foreground_binary_loopback_only",
                "timestamp": datetime.now(timezone.utc).isoformat(),
                "binary": str(binary.relative_to(root)),
                "binary_sha256": hashlib.sha256(binary.read_bytes()).hexdigest(),
                "os": platform.platform(), "python": platform.python_version(),
                "passed": True, "upstream_attempts": len(captured),
                "quota_fixture": {"rpm": "unlimited", "tpm": "unknown"},
                "checks": ["config_specific_state_path", "protected_temp_state_loaded", "health",
                           "data_cannot_control", "prefix_query_body_auth", "control_stop_exit_zero",
                           "no_fixture_credentials_or_body_in_process_output"],
                "limitations": ["Synthetic HTTP fixture only; no LLM or agent task",
                                "No SSE lifecycle, quota, native setup, other OS or performance claim",
                                "Port selection uses a checked candidate, not socket activation"],
            }
            args.output.parent.mkdir(parents=True, exist_ok=True)
            args.output.write_text(json.dumps(result, indent=2) + "\n")
            print(json.dumps({"passed": True, "upstream_attempts": len(captured)}))
    finally:
        if process is not None and process.poll() is None:
            process.terminate()
            try:
                process.wait(timeout=5)
            except subprocess.TimeoutExpired:
                process.kill()
                process.wait(timeout=5)
        upstream.shutdown()
        upstream.server_close()
        thread.join(timeout=5)


if __name__ == "__main__":
    main()
