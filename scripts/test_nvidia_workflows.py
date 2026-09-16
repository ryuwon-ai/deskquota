"""Offline task/probe checks; no credentials or NVIDIA traffic."""
import argparse
import json
import os
from pathlib import Path
import tempfile
import threading
import time
import unittest
from unittest.mock import patch

import nvidia_workflows as probe


ANSWERS = {
    "code_edit": probe.FIXED_CODE,
    "review_admission": '{"line":2,"issue":"off_by_one"}',
    "review_cache": '{"line":2,"issue":"missing_user_scope"}',
    "classifier": '{"category":"rate_limit"}',
}


class Checks(unittest.TestCase):
    def args(self, output, live=False):
        return argparse.Namespace(model="fixture/model", gateway_base="http://127.0.0.1:4141/r/eval/v1",
                                  live=live, gateway_retries_disabled=True, gateway_first=False,
                                  key_env="PROBE_KEY", output=output)

    def test_task_checker_never_executes_and_rejects_wrong_answers(self):
        for task, answer in ANSWERS.items():
            self.assertTrue(probe.check_task(task, answer))
            self.assertFalse(probe.check_task(task, "not an answer"))
            self.assertFalse(probe.check_task(task, "x" * 8193))
        self.assertTrue(probe.check_task("code_edit", probe.FIXED_CODE.replace("    ", "\t")))
        self.assertFalse(probe.check_task("code_edit", probe.FIXED_CODE.replace("min(", "max(")))
        self.assertFalse(probe.check_task("code_edit", probe.FIXED_CODE + '\nraise Exception("must not execute")'))
        self.assertFalse(probe.check_task("review_admission", '{"line":1,"issue":"off_by_one"}'))
        self.assertFalse(probe.check_task("classifier", '{"category":"server_error"}'))

    def test_dry_run_and_boundaries(self):
        with tempfile.TemporaryDirectory() as temp:
            output = Path(temp) / "dry.json"
            args = self.args(output)
            with patch.object(probe.transport, "request", side_effect=AssertionError("dry network call")):
                report = probe.execute(args)
            self.assertEqual(report["max_client_requests"], 10)
            self.assertEqual(report["max_requested_output_tokens"], 3840)
            self.assertEqual(report["requests"], [])
            self.assertTrue(all(size <= 4096 for size in report["request_body_bytes"].values()))
            if os.name != "nt":
                self.assertEqual(output.stat().st_mode & 0o777, 0o600)
            original = output.read_bytes()
            with self.assertRaises(FileExistsError):
                probe.execute(args)
            self.assertEqual(output.read_bytes(), original)
            args.live, args.output, args.gateway_retries_disabled = True, Path(temp) / "live.json", False
            with self.assertRaises(ValueError):
                probe.execute(args)
            self.assertFalse(args.output.exists())
            args.gateway_retries_disabled = True
            with patch.dict(os.environ, PROBE_KEY=""), self.assertRaises(ValueError):
                probe.execute(args)
            self.assertFalse(args.output.exists())

    def test_complete_tasks_repeat_exact_body_and_parallel_reviews(self):
        with tempfile.TemporaryDirectory() as temp, patch.dict(os.environ, PROBE_KEY="synthetic-key"), patch.object(probe, "PAUSE", 0):
            bodies = []
            lock = threading.Lock()
            review_barrier = threading.Barrier(2)

            def request(base, body, key, deadline, check, cancel_event):
                task = check.args[0]
                self.assertTrue(0 < deadline <= probe.transport.DEADLINE)
                self.assertFalse(cancel_event.is_set())
                self.assertEqual(key, "synthetic-key")
                self.assertLessEqual(len(body), 4096)
                if task.startswith("review_"):
                    review_barrier.wait(timeout=5)
                with lock:
                    bodies.append((base, task, body))
                return {"outcome": "completed", "task_passed": check(ANSWERS[task]),
                        "usage": {"status": "observed", "prompt_tokens": 1, "completion_tokens": 2, "total_tokens": 3}}

            output = Path(temp) / "result.json"
            args = self.args(output, True)
            args.gateway_first = True
            with patch.object(probe.transport, "request", side_effect=request) as call:
                report = probe.execute(args)
            self.assertEqual(call.call_count, 10)
            self.assertEqual(report["status"], "completed_tasks")
            self.assertEqual(report["passed_workflows"], 6)
            self.assertEqual(report["completed_responses"], 10)
            self.assertEqual(report["arm_order"], ["gateway", "direct"])
            self.assertIsNone(report["upstream_attempts"])
            classifiers = [body for _, task, body in bodies if task == "classifier"]
            self.assertEqual(len(classifiers), 4)
            self.assertTrue(all(body == classifiers[0] for body in classifiers))
            raw = output.read_text()
            for secret in ["synthetic-key", *ANSWERS.values(), *probe.PROMPTS.values()]:
                self.assertNotIn(secret, raw)

    def test_parallel_http_failure_stops_after_inflight_pair(self):
        with tempfile.TemporaryDirectory() as temp, patch.dict(os.environ, PROBE_KEY="synthetic-key"), patch.object(probe, "PAUSE", 0):
            def request(*_, check, **__):
                task = check.args[0]
                return {"outcome": "http_error", "http_status": 429} if task == "review_admission" else {"outcome": "completed", "task_passed": True}

            with patch.object(probe.transport, "request", side_effect=request) as call:
                report = probe.execute(self.args(Path(temp) / "failure.json", True))
            self.assertEqual(call.call_count, 3)
            self.assertEqual(report["status"], "stopped_on_transport_failure")
            self.assertEqual(len(report["requests"]), 3)
            self.assertEqual(report["workflows"][-1]["outcome"], "transport_failed")

    def test_semantic_failure_counts_without_repair_retry(self):
        with tempfile.TemporaryDirectory() as temp, patch.dict(os.environ, PROBE_KEY="synthetic-key"), patch.object(probe, "PAUSE", 0):
            def request(*_, check, **__):
                return {"outcome": "completed", "task_passed": check.args[0] != "code_edit"}

            with patch.object(probe.transport, "request", side_effect=request) as call:
                report = probe.execute(self.args(Path(temp) / "quality.json", True))
            self.assertEqual(call.call_count, 10)
            self.assertEqual(report["status"], "completed_with_task_failures")
            self.assertEqual(report["passed_workflows"], 4)
            self.assertEqual(report["completed_responses"], 10)

    def test_run_deadline_and_interrupt_are_preserved(self):
        with tempfile.TemporaryDirectory() as temp, patch.dict(os.environ, PROBE_KEY="synthetic-key"):
            with patch.object(probe, "RUN_DEADLINE", 0), patch.object(probe.transport, "request") as call:
                report = probe.execute(self.args(Path(temp) / "deadline.json", True))
            call.assert_not_called()
            self.assertEqual(report["status"], "run_deadline")
            output = Path(temp) / "interrupted.json"
            with patch.object(probe.transport, "request", side_effect=KeyboardInterrupt), self.assertRaises(KeyboardInterrupt):
                probe.execute(self.args(output, True))
            report = json.loads(output.read_text())
            self.assertEqual(report["status"], "interrupted")
            self.assertEqual(report["requests"][0]["outcome"], "interrupted_or_failed")

    def test_parallel_interrupt_cancels_sibling_before_executor_waits(self):
        with tempfile.TemporaryDirectory() as temp, patch.dict(os.environ, PROBE_KEY="synthetic-key"), patch.object(probe, "PAUSE", 0):
            sibling_started = threading.Event()
            sibling_cancelled = threading.Event()

            def request(*_, check, cancel_event, **__):
                if check.args[0] == "review_admission":
                    self.assertTrue(sibling_started.wait(1))
                    raise KeyboardInterrupt
                if check.args[0] == "review_cache":
                    sibling_started.set()
                    if cancel_event.wait(2):
                        sibling_cancelled.set()
                    return {"outcome": "cancelled"}
                return {"outcome": "completed", "task_passed": True}

            started = time.monotonic()
            output = Path(temp) / "parallel-interrupt.json"
            with patch.object(probe.transport, "request", side_effect=request), self.assertRaises(KeyboardInterrupt):
                probe.execute(self.args(output, True))
            self.assertTrue(sibling_cancelled.is_set())
            self.assertLess(time.monotonic() - started, 1)
            self.assertEqual(json.loads(output.read_text())["status"], "interrupted")


if __name__ == "__main__":
    unittest.main()
