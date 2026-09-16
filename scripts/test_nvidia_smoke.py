"""Local-only checks. No credentials or requests to NVIDIA required."""
import argparse
import contextlib
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
import json
import os
from pathlib import Path
import tempfile
import threading
import time
from types import SimpleNamespace
import unittest
from unittest.mock import patch

import nvidia_smoke as probe


def check_ok(text):
    return text == "OK"


def stream(finish="stop"):
    return (b'data: {"model":"fixture/model","choices":[{"delta":{"content":"OK"},"finish_reason":null}]}\r\n\r\n'
            + b'data: ' + json.dumps({"choices": [{"delta": {}, "finish_reason": finish}],
                                     "usage": {"prompt_tokens": 5, "completion_tokens": 1, "total_tokens": 6}}).encode()
            + b'\n\ndata: [DONE]\n\n')


class Checks(unittest.TestCase):
    def test_child_cleanup_and_terminal_pipe_race(self):
        class Process:
            pid = None
            stopped = False
            joined = False

            def start(self):
                self.pid = 42
                raise KeyboardInterrupt

            def is_alive(self):
                return not self.stopped

            def terminate(self):
                self.stopped = True

            def join(self, *_):
                self.joined = True

        process = Process()
        receiver, sender = unittest.mock.Mock(), unittest.mock.Mock()
        context = SimpleNamespace(Pipe=lambda **_: (receiver, sender), Process=lambda **_: process)
        with patch.object(probe.multiprocessing, "get_context", return_value=context), self.assertRaises(KeyboardInterrupt):
            probe.request(probe.DIRECT, b"{}", "synthetic-key")
        self.assertTrue(process.stopped and process.joined)
        receiver.close.assert_called_once()
        sender.close.assert_called_once()

        # The child exits after a false poll; its final message is still pending.
        process = Process()
        process.start = lambda: setattr(process, "pid", 42)
        process.stopped = True
        receiver.reset_mock()
        receiver.poll.side_effect = [False, True, True]
        receiver.recv.side_effect = [{"outcome": "completed"}, EOFError()]
        context.Process = lambda **_: process
        with patch.object(probe.multiprocessing, "get_context", return_value=context):
            self.assertEqual(probe.request(probe.DIRECT, b"{}", "synthetic-key")["outcome"], "completed")
        self.assertTrue(process.joined)

    def test_sse_fragmentation_and_error_boundaries(self):
        parser = probe.SSE()
        for byte in stream():
            parser.feed(bytes([byte]))
        self.assertTrue(parser.success())
        self.assertEqual(parser.result["usage"]["total_tokens"], 6)
        for payload in (stream("error"), stream().replace(b"[DONE]", b""),
                        b'event: error\ndata: {}\n\n' + stream(),
                        b'data: {"error":{}}\n\n' + stream(), stream() + b'data: {}\n\n'):
            parser = probe.SSE()
            parser.feed(payload)
            self.assertFalse(parser.success())
        parser = probe.SSE(("fixture/model",))
        parser.feed(stream())
        self.assertNotIn("reported_model", parser.result)
        content = []
        parser = probe.SSE(content_sink=content.append)
        parser.feed(stream())
        self.assertEqual(content, ["OK"])
        self.assertNotIn("content", parser.result)

    def test_input_boundaries(self):
        for url in ("https://127.0.0.1:80/v1", "http://localhost:80/v1", "http://127.0.0.1:0/v1",
                    "http://127.0.0.1:80/v1?key=x", "http://127.0.0.1:80@evil.invalid/v1"):
            with self.assertRaises(ValueError):
                probe.gateway_base(url)
        self.assertEqual(probe.gateway_base("http://127.0.0.1:1234/r/smoke/v1"), "http://127.0.0.1:1234/r/smoke/v1")
        for value in ("", "short", "header\r\ninjection", "unicode-가"):
            with patch.dict(os.environ, PROBE_KEY=value), self.assertRaises(ValueError):
                probe.credential("PROBE_KEY")

    def test_real_http_and_wall_deadline(self):
        observations = []

        class Handler(BaseHTTPRequestHandler):
            def log_message(self, *_):
                pass

            def do_POST(self):
                observations.append((self.path, self.headers.get("Authorization") == "Bearer synthetic-key", self.headers.get("X-LLMGW-Token") is None))
                self.rfile.read(int(self.headers["Content-Length"]))
                path = self.path
                status = 302 if path.startswith("/redirect/") else 429 if path.startswith("/limited/") else 200
                content = stream("error" if path.startswith("/error/") else "stop")
                if path.startswith("/large/"):
                    content = b":" + b"x" * (probe.LIMIT + 1)
                self.send_response(status)
                self.send_header("Content-Type", "application/json" if path.startswith("/json/") else "text/event-stream")
                self.send_header("Content-Length", str(len(content)))
                self.send_header("Location", "https://must-not-follow.invalid")
                self.send_header("Retry-After", "3")
                self.send_header("X-Ratelimit-Remaining-Tokens", "unsafe-text")
                self.end_headers()
                if path.startswith("/stall/"):
                    time.sleep(1)
                with contextlib.suppress(OSError):
                    self.wfile.write(content)

        server = ThreadingHTTPServer(("127.0.0.1", 0), Handler)
        thread = threading.Thread(target=server.serve_forever, daemon=True)
        thread.start()
        try:
            base = "http://127.0.0.1:" + str(server.server_port)
            for case, expected in (("ok", "completed"), ("redirect", "http_error"), ("limited", "http_error"),
                                   ("error", "invalid_stream"), ("json", "non_sse"), ("large", "error")):
                result = probe.request(base + "/" + case, b"{}", "synthetic-key", deadline=5)
                self.assertEqual(result["outcome"], expected, result)
                self.assertEqual(result["rate_headers"], {"retry-after": "3"})
            self.assertEqual(len(observations), 6)  # Redirect and 429 made no extra attempts.
            self.assertTrue(all(auth and no_custom for _, auth, no_custom in observations))
            checked = probe.request(base + "/ok", b"{}", "synthetic-key", deadline=5, check=check_ok)
            self.assertEqual(checked["outcome"], "completed")
            self.assertTrue(checked["task_passed"])
            self.assertNotIn("OK", json.dumps(checked))
            result = probe.request(base + "/stall", b"{}", "synthetic-key", deadline=.4)
            self.assertEqual(result["outcome"], "timeout")
            self.assertLess(result["process_wall_ms"], 3000)
            cancelled = threading.Event()
            timer = threading.Timer(.1, cancelled.set)
            timer.start()
            result = probe.request(base + "/stall", b"{}", "synthetic-key", deadline=5, cancel_event=cancelled)
            timer.join()
            self.assertEqual(result["outcome"], "cancelled")
            self.assertLess(result["process_wall_ms"], 1000)
        finally:
            server.shutdown()
            server.server_close()
            thread.join()

    def test_budget_dry_run_failure_and_exclusive_artifact(self):
        with tempfile.TemporaryDirectory() as temp:
            output = Path(temp) / "dry.json"
            args = argparse.Namespace(model=["fixture/a", "fixture/b"], gateway_base="http://127.0.0.1:1/r/smoke/v1",
                                      live=False, key_env="PROBE_KEY", output=output)
            args.model = None
            with self.assertRaises(ValueError):
                probe.execute(args)
            self.assertFalse(output.exists())
            args.model = ["fixture/a", "fixture/b"]
            with patch.object(probe, "request", side_effect=AssertionError("dry run called network")):
                report = probe.execute(args)
            self.assertEqual(report["max_client_requests"], 8)
            self.assertEqual(report["requests"], [])
            original = output.read_bytes()
            with self.assertRaises(FileExistsError):
                probe.execute(args)
            self.assertEqual(output.read_bytes(), original)
            args.live, args.output = True, Path(temp) / "failure.json"
            with patch.dict(os.environ, PROBE_KEY="synthetic-key"), patch.object(probe, "PAUSE", 0), \
                 patch.object(probe, "request", side_effect=[{"outcome": "completed"}, {"outcome": "http_error", "http_status": 429}]) as call:
                report = probe.execute(args)
            self.assertEqual(report["status"], "stopped_on_failure")
            self.assertEqual(len(report["requests"]), 2)
            self.assertEqual(call.call_count, 2)
            raw = args.output.read_text()
            self.assertNotIn("synthetic-key", raw)
            self.assertNotIn("local-fixture", raw)
            self.assertNotIn("Reply with exactly OK.", raw)
            self.assertEqual(json.loads(raw)["status"], "stopped_on_failure")
            args.output = Path(temp) / "missing.json"
            with patch.dict(os.environ, PROBE_KEY=""), self.assertRaises(ValueError):
                probe.execute(args)
            self.assertFalse(args.output.exists())
            with patch.dict(os.environ, PROBE_KEY="synthetic-key"), \
                 patch.object(probe, "request", side_effect=KeyboardInterrupt), self.assertRaises(KeyboardInterrupt):
                probe.execute(args)
            interrupted = json.loads(args.output.read_text())
            self.assertEqual(interrupted["status"], "interrupted")
            self.assertEqual(interrupted["requests"][0]["outcome"], "interrupted_or_failed")


if __name__ == "__main__":
    unittest.main()
