#!/usr/bin/env python3
"""Paired native JSON-accounting mechanism check, using only synthetic loopback traffic."""
import argparse
import asyncio
import collections
import contextlib
import datetime
import hashlib
import json
from pathlib import Path
import platform
import tempfile
import time

from benchmark import abort_startup, control, digest, distribution, owned_resources, stop_gateway
from benchmark_http import Client, encode
from measure_native import safe_environment


REQUEST = encode({"model": "synthetic", "messages": [{"role": "user", "content": "synthetic"}],
                  "max_tokens": 128, "stream": False})
RESERVATION = len(REQUEST) + 128
PATH = "/r/probe/v1/chat/completions"


async def run(binary, arm, mode, repeat, row):
    payload = {"id": "synthetic", "object": "chat.completion", "model": "synthetic",
               "choices": [{"index": 0, "message": {"role": "assistant", "content": "ok"},
                            "finish_reason": "stop"}]}
    if mode != "missing":
        payload["usage"] = {"prompt_tokens": 3, "completion_tokens": 1}
    expected_body = encode(payload)
    attempts, writers, handlers = [], set(), set()

    async def fixture(reader, writer):
        task = asyncio.current_task()
        handlers.add(task)
        writers.add(writer)
        try:
            while True:
                head = await reader.readuntil(b"\r\n\r\n")
                fields = dict(line.lower().split(b":", 1) for line in head.split(b"\r\n")[1:] if line)
                body = await reader.readexactly(int(fields[b"content-length"]))
                assert body == REQUEST, "unexpected upstream payload"
                attempts.append(time.monotonic())
                if mode != "no_wait":
                    await asyncio.sleep(.01)
                writer.write((f"HTTP/1.1 200 OK\r\nContent-Type: application/json\r\n"
                              f"Content-Length: {len(expected_body)}\r\n\r\n").encode() + expected_body)
                await writer.drain()
        except (OSError, asyncio.IncompleteReadError):
            pass
        finally:
            writers.discard(writer)
            writer.close()
            with contextlib.suppress(OSError):
                await writer.wait_closed()
            handlers.discard(task)

    upstream = await asyncio.start_server(fixture, "127.0.0.1", 0)
    upstream_port = upstream.sockets[0].getsockname()[1]
    proc = None
    clients = []
    row.update(arm=arm, mode=mode, repeat=repeat, requests=[], passed=False)
    try:
        with tempfile.TemporaryDirectory(prefix="deskquota-json-") as temp:
            directory = Path(temp)
            env = safe_environment(directory / "home")
            reservation = await asyncio.start_server(lambda r, w: w.close(), "127.0.0.1", 0)
            port = reservation.sockets[0].getsockname()[1]
            reservation.close()
            await reservation.wait_closed()
            limit = 1000000 if mode == "no_wait" else RESERVATION + 12
            config = directory / "config.toml"
            config.write_text(f'''listen = "127.0.0.1:{port}"
concurrency = 1
startup_hold_secs = 0
accounting = "{'reserved' if mode == 'reserved' else 'actual'}"
[upstream]
api_base = "http://127.0.0.1:{upstream_port}/v1"
[upstream.auth]
mode = "none"
[quota.rpm]
kind = "unlimited"
[quota.tpm]
kind = "known"
value = {limit}
[[models]]
id = "synthetic"
max_output_tokens = 128
[[roots]]
id = "probe"
endpoints = ["chat/completions"]
models = ["synthetic"]
''')
            row.update(config_sha256=digest(config), tpm_limit=limit,
                       reservation_per_request=RESERVATION, cache_enabled=False)
            lookup = await asyncio.create_subprocess_exec(str(binary), "doctor", "--config", str(config), "--json",
                         stdout=asyncio.subprocess.PIPE, stderr=asyncio.subprocess.PIPE, env=env)
            try:
                output, _ = await asyncio.wait_for(lookup.communicate(), 10)
            finally:
                await abort_startup(lookup)
            assert lookup.returncode == 0, "doctor failed"
            state = Path(json.loads(output)["state_directory"])
            state.mkdir(mode=0o700)
            token = state / "control-token"
            token.write_text("synthetic-benchmark-control")
            token.chmod(0o600)
            proc = await asyncio.create_subprocess_exec(str(binary), "run", "--config", str(config),
                       stdout=asyncio.subprocess.DEVNULL, stderr=asyncio.subprocess.DEVNULL, env=env)
            for _ in range(100):
                assert proc.returncode is None, "gateway exited during startup"
                try:
                    row["status_before"] = await control(port)
                    break
                except ConnectionRefusedError:
                    await asyncio.sleep(.02)
            else:
                raise TimeoutError("gateway startup")
            row["resources_before"] = await owned_resources(proc.pid)

            async def request(client, index, warmup=False):
                record = {"id": index, "warmup": warmup, "outcome": "pending"}
                row["requests"].append(record)
                start = time.monotonic()
                try:
                    response = await asyncio.wait_for(client.exchange("POST", PATH, REQUEST, {}),
                                                      5 if mode == "no_wait" else .75)
                    record.update(status=response["status"], body_exact=response["body"] == expected_body)
                    record["outcome"] = "completed" if response["status"] == 200 and record["body_exact"] else "error"
                except TimeoutError:
                    record["outcome"] = "timeout"
                    await client.close(reset=True)
                except (OSError, asyncio.IncompleteReadError):
                    record["outcome"] = "error"
                finally:
                    record["elapsed_ms"] = (time.monotonic() - start) * 1000

            start = time.monotonic()
            if mode == "no_wait":
                clients.append(Client(port))
                for index in range(25):
                    await request(clients[0], index, warmup=index < 5)
            else:
                clients.extend(Client(port) for _ in range(4))
                await asyncio.gather(*(request(client, i) for i, client in enumerate(clients)))
            row["elapsed_ms"] = (time.monotonic() - start) * 1000
            for client in clients:
                await client.close()
            for _ in range(100):
                row["status_after"] = await control(port)
                if row["status_after"]["active"] == 0 and row["status_after"]["admission"]["queue_length"] == 0:
                    break
                await asyncio.sleep(.01)
            else:
                raise AssertionError("gateway did not clean up requests")
            row["resources_after"] = await owned_resources(proc.pid)
            row["upstream_attempt_times"] = attempts
            row["upstream_attempts"] = len(attempts)
            counts = collections.Counter(r["outcome"] for r in row["requests"])
            row["outcomes"] = {name: counts[name] for name in ("completed", "timeout", "error", "cancelled", "rejected")}
            completed = 25 if mode == "no_wait" else (4 if arm == "candidate" and mode == "actual" else 1)
            assert counts["completed"] == completed and counts["error"] == 0
            assert counts["timeout"] == len(row["requests"]) - completed
            assert len(attempts) == completed == row["status_after"]["upstream_attempts"]
            assert not row["status_after"]["exact_cache"]["enabled"]
            observed = completed if arm == "candidate" and mode != "missing" else 0
            assert row["status_after"]["usage_known"] == observed
            assert row["status_after"]["usage_unknown"] == completed - observed
            assert row["status_after"]["observed_input_tokens"] == observed * 3
            assert row["status_after"]["observed_output_tokens"] == observed
            assert row["status_after"]["terminal_total"] == completed
            expected_debit = completed * (4 if arm == "candidate" and mode not in {"reserved", "missing"} else RESERVATION)
            assert int(row["status_after"]["admission"]["tpm_debited"]) == expected_debit
            assert int(row["status_after"]["admission"]["tpm_held"]) == 0
            row["expected_tpm_debited"] = expected_debit
            if arm == "candidate":
                status_proc = await asyncio.create_subprocess_exec(str(binary), "status", "--config", str(config),
                              stdout=asyncio.subprocess.PIPE, stderr=asyncio.subprocess.PIPE, env=env)
                try:
                    status_text, _ = await asyncio.wait_for(status_proc.communicate(), 5)
                finally:
                    await abort_startup(status_proc)
                assert status_proc.returncode == 0
                row["status_cache_lines"] = [line for line in status_text.decode().splitlines() if "cache" in line.lower()]
                assert row["status_cache_lines"], "plain status lacks cache visibility"
            await stop_gateway(proc, port)
            row["passed"] = True
    finally:
        for client in clients:
            await client.close()
        if proc is not None:
            await abort_startup(proc)
        upstream.close()
        await upstream.wait_closed()
        for writer in list(writers):
            writer.close()
        if handlers:
            await asyncio.wait_for(asyncio.gather(*list(handlers)), 2)


async def measure(args, report):
    for repeat in range(args.repeats):
        arms = ["baseline", "candidate"] if repeat % 2 == 0 else ["candidate", "baseline"]
        for mode in ("actual", "reserved", "missing", "no_wait"):
            for arm in arms:
                row = {}
                report["runs"].append(row)
                await run(getattr(args, arm), arm, mode, repeat, row)
    report["no_wait_summary"] = {
        arm: distribution([r["elapsed_ms"] for run in report["runs"] if run["arm"] == arm and run["mode"] == "no_wait"
                           for r in run["requests"] if not r["warmup"] and r["outcome"] == "completed"])
        for arm in ("baseline", "candidate")}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--baseline", type=Path, required=True)
    parser.add_argument("--candidate", type=Path, required=True)
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument("--repeats", type=int, default=5)
    args = parser.parse_args()
    if not 1 <= args.repeats <= 10:
        parser.error("repeats must be 1..10")
    for arm in ("baseline", "candidate"):
        setattr(args, arm, getattr(args, arm).resolve(strict=True))
    report = {"schema": 1, "scope": "JSON accounting mechanism and descriptive loopback overhead; not competitor, provider or low-end proof",
              "platform": platform.platform(), "repeats": args.repeats,
              "started_at": datetime.datetime.now(datetime.timezone.utc).isoformat(),
              "script_sha256": digest(Path(__file__)), "request_sha256": hashlib.sha256(REQUEST).hexdigest(),
              "binaries": {arm: {"path": str(getattr(args, arm)), "sha256": digest(getattr(args, arm)),
                                 "bytes": getattr(args, arm).stat().st_size} for arm in ("baseline", "candidate")},
              "runs": [], "passed": False}
    args.output.parent.mkdir(parents=True, exist_ok=True)
    with args.output.open("x") as output:
        try:
            asyncio.run(measure(args, report))
            for arm in ("baseline", "candidate"):
                assert digest(getattr(args, arm)) == report["binaries"][arm]["sha256"], "binary changed during measurement"
            report["passed"] = True
        except Exception as error:
            report["error_class"] = type(error).__name__
            raise
        finally:
            report["finished_at"] = datetime.datetime.now(datetime.timezone.utc).isoformat()
            json.dump(report, output, indent=2)
            output.write("\n")


if __name__ == "__main__":
    main()
