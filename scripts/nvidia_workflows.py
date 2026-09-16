#!/usr/bin/env python3
"""Opt-in, bounded NVIDIA task/API comparison; not an installed agent benchmark."""
import argparse
import ast
from concurrent.futures import ThreadPoolExecutor, as_completed
from functools import partial
import hashlib
import json
import os
from pathlib import Path
import threading
import time

import nvidia_smoke as transport

MAX_TOKENS = 384
RUN_DEADLINE = 900
PAUSE = 10
FIXED_CODE = "def bounded_backoff(attempt, ceiling):\n    delay = 2 ** attempt\n    return min(delay, ceiling)\n"
PROMPTS = {
    "code_edit": (
        "Fix this public toy function by changing only the incorrect max call to min. "
        "Return the complete Python function, no markdown or explanation.\n"
        "def bounded_backoff(attempt, ceiling):\n    delay = 2 ** attempt\n    return max(delay, ceiling)\n"
    ),
    "review_admission": (
        "Review this public toy function. used is the count BEFORE admitting one new request. "
        "The resulting count must not exceed limit. Return only JSON with the faulty line number "
        'and issue: {"line":N,"issue":"off_by_one"} if faulty, or {"issue":"none"}.\n'
        "1: def may_admit(used, limit):\n2:     return used <= limit\n"
    ),
    "review_cache": (
        "Review this public toy cache key. Cache entries MUST be isolated by user as well as "
        "model and prompt. Return only JSON with the faulty line number and issue: "
        '{"line":N,"issue":"missing_user_scope"} if faulty, or {"issue":"none"}.\n'
        "1: def cache_key(model, prompt, user):\n2:     return (model, prompt)\n"
    ),
    "classifier": (
        "Classify this public synthetic HTTP incident. Return only JSON with category equal to "
        '"rate_limit" for 429, "server_error" for 5xx, or "other" otherwise.\n'
        "Incident: HTTP 429; Retry-After: 2.\n"
    ),
}
WORKFLOWS = (
    ("code_edit", ("code_edit",)),
    ("parallel_review", ("review_admission", "review_cache")),
    ("classifier_rerun", ("classifier", "classifier")),
)


def check_task(task, text):
    if len(text) > 8192:
        return False
    try:
        if task == "code_edit":
            # ponytail: fixed-task AST equality; broader repairs need a sandboxed evaluator.
            return ast.dump(ast.parse(text.strip())) == ast.dump(ast.parse(FIXED_CODE))
        expected = {
            "review_admission": {"line": 2, "issue": "off_by_one"},
            "review_cache": {"line": 2, "issue": "missing_user_scope"},
            "classifier": {"category": "rate_limit"},
        }
        return json.loads(text) == expected[task]
    except (ValueError, SyntaxError, RecursionError):
        return False


def payload(model, task):
    return json.dumps({
        "model": model, "messages": [{"role": "user", "content": PROMPTS[task]}],
        "temperature": 0, "max_tokens": MAX_TOKENS, "stream": True,
        "stream_options": {"include_usage": True},
    }, separators=(",", ":")).encode()


def execute(args):
    if not transport.MODEL.fullmatch(args.model):
        raise ValueError("invalid model")
    transport.gateway_base(args.gateway_base)
    if args.live and not args.gateway_retries_disabled:
        raise ValueError("live comparison requires a separately verified gateway with retries disabled")
    key = transport.credential(args.key_env) if args.live else ""
    if key and key in args.model:
        raise ValueError("model contains credential")
    bodies = {task: payload(args.model, task) for task in PROMPTS}
    if any(len(body) > 4096 for body in bodies.values()):
        raise ValueError("fixed fixture request body exceeds its bound")
    arms = ["gateway", "direct"] if args.gateway_first else ["direct", "gateway"]
    report = {
        "status": "dry_run", "model": args.model, "arm_order": arms,
        "scope": "fixed real task/API probe; no installed agent, tool execution, or performance claim",
        "started_utc": time.strftime("%Y-%m-%dT%H:%M:%SZ", time.gmtime()),
        "probe_sha256": hashlib.sha256(Path(__file__).read_bytes()).hexdigest(),
        "transport_sha256": hashlib.sha256(Path(transport.__file__).read_bytes()).hexdigest(),
        "max_client_requests": 10, "max_tokens_per_request": MAX_TOKENS,
        "max_requested_output_tokens": 10 * MAX_TOKENS,
        "request_body_bytes": {task: len(body) for task, body in bodies.items()},
        "request_deadline_s": transport.DEADLINE, "run_deadline_s": RUN_DEADLINE,
        "between_workflows_pause_s": PAUSE,
        "gateway_retries_disabled_operator_confirmed": args.gateway_retries_disabled,
        "upstream_attempts": None,
        "attempt_scope": "10 client requests; upstream cap depends on audited retry-free gateway configuration",
        "usage_scope": "reported usage per delivered response; cached usage is not fresh provider spending",
        "timing_scope": "cold client process and HTTP connection included; inter-workflow pauses excluded from task timing",
        "requests": [], "workflows": [],
    }
    fd = os.open(args.output, os.O_WRONLY | os.O_CREAT | os.O_EXCL, 0o600)
    with os.fdopen(fd, "w") as out:
        def save():
            out.seek(0)
            json.dump(report, out, indent=2)
            out.write("\n")
            out.truncate()
            out.flush()

        save()
        if not args.live:
            return report
        started = time.monotonic()
        cancelled = threading.Event()
        report["status"] = "running"

        def invoke(arm, task):
            remaining = RUN_DEADLINE - (time.monotonic() - started)
            if remaining <= 0:
                return {"outcome": "run_deadline", "task_passed": False}
            return transport.request(
                transport.DIRECT if arm == "direct" else args.gateway_base,
                bodies[task], key, deadline=min(transport.DEADLINE, remaining),
                check=partial(check_task, task), cancel_event=cancelled,
            )

        try:
            for arm in arms:
                for workflow, tasks in WORKFLOWS:
                    remaining = RUN_DEADLINE - (time.monotonic() - started)
                    if remaining <= 0 or (report["workflows"] and remaining <= PAUSE):
                        report["status"] = "run_deadline"
                        return report
                    if report["workflows"]:
                        time.sleep(PAUSE)
                    flow_started = time.monotonic()
                    flow = {"arm": arm, "workflow": workflow, "outcome": "started", "request_indices": []}
                    report["workflows"].append(flow)
                    save()

                    def new_row(task):
                        if len(report["requests"]) >= 10:
                            raise ValueError("client request cap exhausted")
                        row = {"arm": arm, "task": task, "outcome": "started",
                               "payload_sha256": hashlib.sha256(bodies[task]).hexdigest()}
                        flow["request_indices"].append(len(report["requests"]))
                        report["requests"].append(row)
                        save()
                        return row

                    if workflow == "parallel_review":
                        # Only the two independent reviews overlap; no repair or retry loops.
                        with ThreadPoolExecutor(max_workers=2) as pool:
                            futures = {}
                            try:
                                for task in tasks:
                                    row = new_row(task)
                                    futures[pool.submit(invoke, arm, task)] = row
                                for future in as_completed(futures):
                                    futures[future].update(future.result())
                                    save()
                            except BaseException:
                                # Signal workers before the executor waits for their shutdown.
                                cancelled.set()
                                for future in futures:
                                    future.cancel()
                                raise
                    else:
                        for task in tasks:
                            row = new_row(task)
                            row.update(invoke(arm, task))
                            save()
                            if row["outcome"] != "completed":
                                break
                    rows = [report["requests"][i] for i in flow["request_indices"]]
                    transport_ok = len(rows) == len(tasks) and all(row["outcome"] == "completed" for row in rows)
                    passed = transport_ok and all(row.get("task_passed") is True for row in rows)
                    flow.update(outcome="passed" if passed else "task_failed" if transport_ok else "transport_failed",
                                elapsed_ms=(time.monotonic() - flow_started) * 1000)
                    save()
                    if not transport_ok:
                        report["status"] = "stopped_on_transport_failure"
                        return report
            report["status"] = "completed_tasks" if all(f["outcome"] == "passed" for f in report["workflows"]) else "completed_with_task_failures"
        except BaseException as error:
            cancelled.set()
            report.update(status="interrupted" if isinstance(error, KeyboardInterrupt) else "failed",
                          error_type=type(error).__name__)
            raise
        finally:
            for row in [*report["requests"], *report["workflows"]]:
                if row["outcome"] == "started":
                    row["outcome"] = "interrupted_or_failed"
            report["elapsed_ms"] = (time.monotonic() - started) * 1000
            report["passed_workflows"] = sum(f["outcome"] == "passed" for f in report["workflows"])
            report["completed_responses"] = sum(r["outcome"] == "completed" for r in report["requests"])
            save()
    return report


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--live", action="store_true")
    parser.add_argument("--model", required=True)
    parser.add_argument("--gateway-base", required=True)
    parser.add_argument("--gateway-first", action="store_true")
    parser.add_argument("--gateway-retries-disabled", action="store_true",
                        help="confirm the separately verified isolated gateway has retry_transient_429=false")
    parser.add_argument("--key-env", default="NVIDIA_API_KEY")
    parser.add_argument("--output", type=Path, required=True)
    try:
        report = execute(parser.parse_args())
    except (ValueError, OSError) as error:
        print(json.dumps({"status": "not_run", "error_type": type(error).__name__}))
        return 2
    print(json.dumps({k: report[k] for k in ("status", "max_client_requests")}))
    return 0 if report["status"] in ("dry_run", "completed_tasks") else 1


if __name__ == "__main__":
    raise SystemExit(main())
