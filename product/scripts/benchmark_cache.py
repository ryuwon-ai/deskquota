#!/usr/bin/env python3
"""Sequential synthetic direct/miss/hit check; no real model or provider calls."""
import argparse
import asyncio
import contextlib
import hashlib
import json
from pathlib import Path
import platform
import tempfile
import time

from benchmark import abort_startup, control, digest, distribution, owned_resources
from benchmark_http import Client, encode
from measure_native import safe_environment


def response_chunks(streaming):
    usage = {"prompt_tokens": 3, "completion_tokens": 1}
    if not streaming:
        return [encode({"id": "synthetic-cache", "object": "chat.completion",
                        "model": "synthetic", "choices": [{"index": 0,
                        "message": {"role": "assistant", "content": "synthetic result"},
                        "finish_reason": "stop"}], "usage": usage})]
    values = [
        {"choices": [{"index": 0, "delta": {"role": "assistant"}, "finish_reason": None}]},
        {"choices": [{"index": 0, "delta": {"content": "synthetic result"}, "finish_reason": None}]},
        {"choices": [{"index": 0, "delta": {}, "finish_reason": "stop"}]},
        {"choices": [], "usage": usage},
    ]
    return [b"data: " + encode(value) + b"\n\n" for value in values] + [b"data: [DONE]\n\n"]


async def measure(args, report):
    attempts = 0
    writers = set()

    async def fixture(reader, writer):
        nonlocal attempts
        writers.add(writer)
        try:
            while True:
                head = await reader.readuntil(b"\r\n\r\n")
                fields = dict(line.lower().split(b":", 1) for line in head.split(b"\r\n")[1:] if line)
                body = await reader.readexactly(int(fields[b"content-length"]))
                payload = json.loads(body)
                attempts += 1
                chunks = response_chunks(payload["stream"])
                await asyncio.sleep(args.delay_ms / 1000)
                kind = "text/event-stream" if payload["stream"] else "application/json"
                writer.write(f"HTTP/1.1 200 OK\r\nContent-Type: {kind}\r\nTransfer-Encoding: chunked\r\n\r\n".encode())
                for index, chunk in enumerate(chunks):
                    writer.write(f"{len(chunk):x}\r\n".encode() + chunk + b"\r\n")
                    await writer.drain()
                    if payload["stream"] and index == 1:
                        await asyncio.sleep(.005)
                writer.write(b"0\r\n\r\n")
                await writer.drain()
        except (OSError, asyncio.IncompleteReadError):
            pass
        finally:
            writers.discard(writer)
            writer.close()
            with contextlib.suppress(OSError):
                await writer.wait_closed()

    upstream = await asyncio.start_server(fixture, "127.0.0.1", 0)
    upstream_port = upstream.sockets[0].getsockname()[1]
    proc = None
    clients = []
    temporary = tempfile.TemporaryDirectory(prefix="deskquota-cache-")
    try:
        directory = Path(temporary.name)
        env = safe_environment(directory / "home")
        reservation = await asyncio.start_server(lambda r, w: w.close(), "127.0.0.1", 0)
        port = reservation.sockets[0].getsockname()[1]
        reservation.close()
        await reservation.wait_closed()
        config = directory / "config.toml"
        config.write_text(f'''listen = "127.0.0.1:{port}"
startup_hold_secs = 0
[cache]
ttl_secs = 300
max_history = 3
[upstream]
api_base = "http://127.0.0.1:{upstream_port}/v1"
[upstream.auth]
mode = "none"
[quota.rpm]
kind = "known"
value = 100000
[quota.tpm]
kind = "known"
value = 10000000
[[models]]
id = "synthetic"
max_output_tokens = 16
[[roots]]
id = "cache"
endpoints = ["chat/completions"]
models = ["synthetic"]
''')
        lookup = await asyncio.create_subprocess_exec(str(args.binary), "doctor", "--config", str(config), "--json",
                     stdout=asyncio.subprocess.PIPE, stderr=asyncio.subprocess.PIPE, env=env)
        try:
            output, _ = await asyncio.wait_for(lookup.communicate(), 10)
        finally:
            await abort_startup(lookup)
        assert lookup.returncode == 0, "configuration rejected"
        state = Path(json.loads(output)["state_directory"])
        state.mkdir(mode=0o700)
        token = state / "control-token"
        token.write_text("synthetic-benchmark-control")
        token.chmod(0o600)
        proc = await asyncio.create_subprocess_exec(str(args.binary), "run", "--config", str(config),
                   stdout=asyncio.subprocess.DEVNULL, stderr=asyncio.subprocess.DEVNULL, env=env)
        for _ in range(100):
            assert proc.returncode is None, "gateway exited during startup"
            try:
                report["status_before"] = await control(port)
                break
            except ConnectionRefusedError:
                await asyncio.sleep(.02)
        else:
            raise TimeoutError("gateway readiness")
        report["resources_before"] = await owned_resources(proc.pid)
        direct, gateway = Client(upstream_port), Client(port)
        clients.extend([direct, gateway])
        for streaming in (False, True):
            expected = b"".join(response_chunks(streaming))
            for sample in range(args.samples):
                body = encode({"model": "synthetic", "messages": [{"role": "user", "content": f"synthetic-{sample}"}],
                               "max_tokens": 16, "temperature": 0, "stream": streaming})
                for arm, client, path in [("direct", direct, "/v1/chat/completions"),
                                           ("miss", gateway, "/r/cache/v1/chat/completions"),
                                           ("hit", gateway, "/r/cache/v1/chat/completions")]:
                    before = attempts
                    start = time.monotonic()
                    row = {"format": "sse" if streaming else "json", "sample": sample, "arm": arm}
                    report["requests"].append(row)
                    response = await asyncio.wait_for(client.exchange("POST", path, body, {}), 10)
                    row.update(status=response["status"], elapsed_ms=(time.monotonic()-start)*1000,
                               upstream_attempts=attempts-before, body_exact=response["body"] == expected)
                    for event in ("first_body", "first_output_delta"):
                        observed = response[event + "_s"]
                        row[event + "_ms"] = None if observed is None else (observed-start)*1000
                    assert row["status"] == 200 and row["body_exact"], "response mismatch"
                    if streaming:
                        row["sse_valid"] = (response["output"] == "synthetic result"
                            and response["terminal_marker_s"] is not None
                            and response["usage"] == {"status": "observed", "prompt_tokens": 3, "completion_tokens": 1})
                        assert row["sse_valid"], "SSE content type or event semantics lost"
                    assert row["upstream_attempts"] == (0 if arm == "hit" else 1), "cache did not avoid exactly one attempt"
        for _ in range(100):
            report["status_after"] = await control(port)
            if report["status_after"]["active"] == 0:
                break
            await asyncio.sleep(.01)
        else:
            raise AssertionError("upstream workers did not settle after EOF")
        report["resources_after"] = await owned_resources(proc.pid)
        report["fixture_attempts"] = attempts
        assert report["status_after"]["upstream_attempts"] == args.samples * 2
        assert report["status_after"]["admission"]["accounting"] == "actual"
        cache = report["status_after"]["exact_cache"]
        assert cache["enabled"] and cache["hits"] == args.samples * 2
        assert cache["misses"] == args.samples * 2 and cache["stores"] == args.samples * 2
        assert cache["entries"] <= 128 and cache["retained_bytes"] <= cache["budget_bytes"] == 4 * 1024 * 1024
        # JSON and SSE misses settle once; replay hits must not create observed usage.
        for name, expected in {"observed_input_tokens": args.samples * 6,
                               "observed_output_tokens": args.samples * 2,
                               "usage_known": args.samples * 2, "usage_unknown": 0,
                               "terminal_total": args.samples * 2}.items():
            assert report["status_after"][name] - report["status_before"][name] == expected, f"hit changed upstream {name}"
        report["counts"] = {"gateway_data_requests": args.samples * 4,
                            "direct_requests": args.samples * 2,
                            "cache_hits": args.samples * 2,
                            "gateway_upstream_attempts": args.samples * 2,
                            "hit_rate_gateway_data_requests": .5,
                            "failed_requests": 0, "cancelled_requests": 0}
        report["summary"] = {
            f"{kind}_{arm}": distribution([r["elapsed_ms"] for r in report["requests"] if r["format"] == kind and r["arm"] == arm])
            for kind in ("json", "sse") for arm in ("direct", "miss", "hit")
        }
        await control(port, "stop", "POST")
        await asyncio.wait_for(proc.wait(), 15)
        assert proc.returncode == 0
    finally:
        for client in clients:
            await client.close()
        if proc is not None:
            await abort_startup(proc)
        upstream.close()
        await upstream.wait_closed()
        for writer in list(writers):
            writer.close()
        temporary.cleanup()


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--binary", type=Path, required=True)
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument("--samples", type=int, default=30)
    parser.add_argument("--delay-ms", type=float, default=50)
    args = parser.parse_args()
    if not 1 <= args.samples <= 1000 or not 0 <= args.delay_ms <= 1000:
        parser.error("samples must be 1..1000 and delay-ms 0..1000")
    args.binary = args.binary.resolve(strict=True)
    report = {"schema": 1, "scope": "sequential synthetic loopback; not model quality, low-end or provider performance",
              "binary_sha256": digest(args.binary), "script_sha256": hashlib.sha256(Path(__file__).read_bytes()).hexdigest(),
              "binary_bytes": args.binary.stat().st_size,
              "platform": platform.platform(), "samples_per_format_arm": args.samples,
              "synthetic_delay_ms": args.delay_ms, "requests": [], "passed": False}
    args.output.parent.mkdir(parents=True, exist_ok=True)
    with args.output.open("x") as output:
        try:
            asyncio.run(measure(args, report))
            report["passed"] = True
        except Exception as error:
            report["error_class"] = type(error).__name__
            raise
        finally:
            json.dump(report, output, indent=2)
            output.write("\n")


if __name__ == "__main__":
    main()
