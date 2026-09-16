#!/usr/bin/env python3
"""Opt-in hosted NVIDIA compatibility probe; stdlib only, no raw content logs."""
import argparse
import hashlib
import http.client
import json
import multiprocessing
import os
from pathlib import Path
import re
import time
from urllib.parse import urlsplit

DIRECT = "https://integrate.api.nvidia.com/v1"
LIMIT = 1024 * 1024
DEADLINE = 150
PAUSE = 10
MODEL = re.compile(r"[A-Za-z0-9][A-Za-z0-9._/-]{0,159}\Z")
NUMERIC_HEADERS = {"retry-after", "retry-after-ms", "x-ratelimit-limit-requests",
                   "x-ratelimit-remaining-requests", "x-ratelimit-limit-tokens",
                   "x-ratelimit-remaining-tokens"}


def gateway_base(value):
    if not re.fullmatch(r"http://127\.0\.0\.1:[0-9]{1,5}(?:/r/[a-zA-Z0-9_-]{1,64})?/v1", value):
        raise ValueError("gateway must be a literal loopback /v1 or /r/ROOT/v1 base")
    if not 1 <= urlsplit(value).port <= 65535:
        raise ValueError("invalid gateway port")
    return value


def credential(name):
    if not re.fullmatch(r"[A-Za-z_][A-Za-z0-9_]*", name):
        raise ValueError("invalid credential environment name")
    value = os.environ.get(name, "")
    if not 8 <= len(value) <= 4096 or any(not 33 <= ord(c) <= 126 for c in value):
        raise ValueError("missing or invalid credential environment")
    return value


class SSE:
    def __init__(self, secrets=(), content_sink=None):
        self.pending = b""
        self.secrets = secrets
        self.content_sink = content_sink
        self.result = {"content_seen": False, "done": False, "finished": False,
                       "stream_error": False, "usage": {"status": "missing"}}

    def feed(self, chunk):
        self.pending += chunk
        if len(self.pending) > LIMIT:
            raise ValueError("SSE frame limit")
        # ponytail: one bounded stream, no SSE dependency or content retention.
        while (match := re.search(rb"\r?\n\r?\n", self.pending)):
            frame, self.pending = self.pending[:match.start()], self.pending[match.end():]
            lines = frame.splitlines()
            if any(line.partition(b":")[0] == b"event" and line.partition(b":")[2].strip() == b"error" for line in lines):
                self.result["stream_error"] = True
            data = b"\n".join(line[5:].lstrip(b" ") for line in lines if line.startswith(b"data:"))
            if not data:
                continue
            if self.result["done"]:
                self.result["stream_error"] = True
                continue
            if data == b"[DONE]":
                self.result["done"] = True
                continue
            event = json.loads(data)
            if not isinstance(event, dict):
                raise ValueError("invalid SSE event")
            if "error" in event or event.get("type") == "error":
                self.result["stream_error"] = True
            model = event.get("model")
            if isinstance(model, str) and MODEL.fullmatch(model) and not any(s in model for s in self.secrets):
                self.result["reported_model"] = model
            usage = event.get("usage")
            if usage is not None:
                values = {k: usage.get(k) for k in ("prompt_tokens", "completion_tokens", "total_tokens")} if isinstance(usage, dict) else {}
                valid = bool(values) and all(type(v) is int and 0 <= v < 2**64 for v in values.values())
                if self.result["usage"]["status"] != "missing" or not valid:
                    self.result["usage"] = {"status": "invalid"}
                else:
                    self.result["usage"] = {"status": "observed", **values}
            for choice in event.get("choices", []):
                content = choice.get("delta", {}).get("content")
                if content:
                    if not isinstance(content, str):
                        raise ValueError("invalid content delta")
                    self.result["content_seen"] = True
                    if self.content_sink is not None:
                        self.content_sink(content)
                finish = choice.get("finish_reason")
                if finish is not None:
                    if finish in ("stop", "length"):
                        self.result["finished"] = True
                    else:
                        self.result["stream_error"] = True

    def success(self):
        r = self.result
        return r["content_seen"] and r["done"] and r["finished"] and not r["stream_error"] and not self.pending.strip()


def request_worker(pipe, base, body, key, check=None):
    started = time.monotonic()
    result = {"outcome": "error", "http_status": None, "bytes_received": 0,
              "first_content_ms": None, "headers_ms": None}
    connection = None
    try:
        url = urlsplit(base)
        connection_type = http.client.HTTPSConnection if url.scheme == "https" else http.client.HTTPConnection
        connection = connection_type(url.hostname, url.port, timeout=DEADLINE)
        headers = {"Authorization": "Bearer " + key, "Content-Type": "application/json",
                   "Accept": "text/event-stream", "Accept-Encoding": "identity"}
        connection.request("POST", url.path + "/chat/completions", body, headers)
        response = connection.getresponse()
        result.update(http_status=response.status, headers_ms=(time.monotonic()-started)*1000)
        result["rate_headers"] = {
            k.lower(): v for k, v in response.getheaders()
            if k.lower() in NUMERIC_HEADERS and re.fullmatch(r"[0-9]{1,16}(?:\.[0-9]{1,6})?", v)
            and key not in v
        }
        pipe.send(result)
        if response.status != 200:
            result["outcome"] = "http_error"  # No retries, redirects or raw error bodies.
        elif response.getheader("Content-Type", "").split(";", 1)[0].strip().lower() != "text/event-stream":
            result["outcome"] = "non_sse"
        elif response.getheader("Content-Encoding", "identity").lower() != "identity":
            result["outcome"] = "encoded_response"
        else:
            content = []
            parser = SSE((key,), content.append if check is not None else None)
            while chunk := response.read1(16384):
                result["bytes_received"] += len(chunk)
                if result["bytes_received"] > LIMIT:
                    raise ValueError("response limit")
                parser.feed(chunk)
                result.update(parser.result)
                if result["first_content_ms"] is None and parser.result["content_seen"]:
                    result["first_content_ms"] = (time.monotonic()-started)*1000
                pipe.send(result)
                if parser.result["stream_error"]:
                    break
            result["outcome"] = "completed" if parser.success() else "invalid_stream"
            if check is not None and parser.success():
                # Task output stays inside the bounded worker; only its verdict leaves.
                result["task_passed"] = check("".join(content)) is True
    except Exception as error:
        result.update(outcome="error", error_type=type(error).__name__)
    finally:
        if connection:
            connection.close()
        result["elapsed_ms"] = (time.monotonic()-started)*1000
        pipe.send(result)
        pipe.close()


def request(base, body, key, deadline=DEADLINE, check=None, cancel_event=None):
    # Native spawn works on Windows too; a trickling socket cannot extend the wall limit.
    context = multiprocessing.get_context("spawn")
    receiver, sender = context.Pipe(duplex=False)
    process = context.Process(target=request_worker, args=(sender, base, body, key, check))
    started = time.monotonic()
    result = {"outcome": "worker_failed"}
    try:
        if cancel_event is not None and cancel_event.is_set():
            return {"outcome": "cancelled", "process_wall_ms": 0}
        process.start()
        sender.close()
        while True:
            if cancel_event is not None and cancel_event.is_set():
                result["outcome"] = "cancelled"
                break
            remaining = deadline - (time.monotonic()-started)
            if remaining <= 0:
                result["outcome"] = "timeout"
                break
            if receiver.poll(min(remaining, .1)):
                try:
                    result = receiver.recv()
                except EOFError:
                    break
    finally:
        sender.close()
        if process.pid is not None:
            if process.is_alive():
                process.terminate()
            process.join(2)
            if process.is_alive():
                process.kill()
                process.join()
        receiver.close()
    result["process_wall_ms"] = (time.monotonic()-started)*1000
    return result


def execute(args):
    models = args.model
    if not models or not 1 <= len(models) <= 2 or len(set(models)) != len(models) or not all(MODEL.fullmatch(m) for m in models):
        raise ValueError("one or two distinct valid models required")
    if args.gateway_base:
        gateway_base(args.gateway_base)
    key = ""
    if args.live:
        key = credential(args.key_env)
        if any(key in model for model in models):
            raise ValueError("model contains credential")
    arms = ["direct", "gateway", "gateway", "direct"] if args.gateway_base else ["direct", "direct"]
    report = {"status": "dry_run", "models": models, "arm_order_per_model": arms,
              "started_utc": time.strftime("%Y-%m-%dT%H:%M:%SZ", time.gmtime()),
              "probe_sha256": hashlib.sha256(Path(__file__).read_bytes()).hexdigest(),
              "max_client_requests": len(models)*len(arms), "max_tokens_per_request": 64,
              "pause_s": PAUSE, "deadline_s": DEADLINE, "requests": [],
              "scope": "compatibility only; client request cap is not an audited upstream attempt cap"}
    # Exclusive, private artifact. No original prompt, output, URL, or credentials recorded.
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
        report["status"] = "running"
        try:
            for model in models:
                body = json.dumps({"model": model, "messages": [{"role": "user", "content": "Reply with exactly OK."}],
                                   "temperature": 0, "max_tokens": 64, "stream": True,
                                   "stream_options": {"include_usage": True}}, separators=(",", ":")).encode()
                for arm in arms:
                    if report["requests"]:
                        time.sleep(PAUSE)
                    row = {"arm": arm, "model": model, "payload_sha256": hashlib.sha256(body).hexdigest(), "outcome": "started"}
                    report["requests"].append(row)
                    save()
                    row.update(request(DIRECT if arm == "direct" else args.gateway_base, body, key))
                    save()
                    if row["outcome"] != "completed":
                        report["status"] = "stopped_on_failure"
                        return report
            report["status"] = "completed_smoke"
        except BaseException as error:
            report.update(status="interrupted" if isinstance(error, KeyboardInterrupt) else "failed", error_type=type(error).__name__)
            for row in report["requests"]:
                if row["outcome"] == "started":
                    row["outcome"] = "interrupted_or_failed"
            raise
        finally:
            save()
    return report


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--live", action="store_true")
    parser.add_argument("--model", action="append", required=True)
    parser.add_argument("--gateway-base")
    parser.add_argument("--key-env", default="NVIDIA_API_KEY")
    parser.add_argument("--output", type=Path, required=True)
    try:
        report = execute(parser.parse_args())
    except (ValueError, OSError) as error:
        print(json.dumps({"status": "not_run", "error_type": type(error).__name__}))
        return 2
    print(json.dumps({k: report[k] for k in ("status", "max_client_requests")}))
    return 0 if report["status"] in ("dry_run", "completed_smoke") else 1


if __name__ == "__main__":
    raise SystemExit(main())
