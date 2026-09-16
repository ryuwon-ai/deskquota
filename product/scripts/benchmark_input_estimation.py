#!/usr/bin/env python3
"""Native input-estimator comparison; synthetic costs, no provider/API access."""
import argparse
import asyncio
import collections
import contextlib
import importlib.util
import json
import math
import os
from pathlib import Path
import platform
import tempfile
import time
import tomllib

import benchmark as bench
from benchmark_accounting import build_gateway_config
from benchmark_http import Client, Mock, encode
from measure_native import safe_environment

ROOT = Path(__file__).resolve().parents[2]
spec = importlib.util.spec_from_file_location("native_comparison", ROOT / "scripts/compare-native-gateways.py")
comparison = importlib.util.module_from_spec(spec)
spec.loader.exec_module(comparison)

# Independent tiktoken 0.14.0 cl100k_base encode_ordinary counts, checked before
# measurement. This deliberately simple chat framing is a fixture, not a claim
# about any provider's hidden template or quota admission formula.
TEXTS = [("x" * 1900, 238),
         ("Please summarize the build failure and propose one minimal fix. " * 32, 353),
         ("빌드 오류의 원인을 설명하고 최소 수정 방법을 제안해 주세요. " * 32, 929)]


def payload(row):
    return row["request_json"].encode("utf-8")


def cases(profile, seed):
    if profile == "original18":
        rows = comparison.workload(seed, "generation", "quarter_actual")
        for row in rows:
            row["request_json"] = comparison.payload(row).decode()
        return rows, {"rpm": 16, "tpm": 6000}, 60
    rows = []
    count = 2 if profile == "mismatch" else 6 if profile == "conversation" else 105 if profile.startswith("no_wait") else 12
    history = []
    for i in range(count):
        content, units = TEXTS[i % len(TEXTS)]
        if profile == "mismatch":
            content, units = TEXTS[0]
        if profile.startswith("no_wait") and i % 4 == 3:
            content, units = "x" * 131072, 16384
        history = history + [(content, units)] if profile == "conversation" else [(content, units)]
        messages = [{"role": "user", "content": text} for text, _ in history]
        body = json.dumps({"model": "synthetic", "messages": messages,
                           "max_tokens": 64, "stream": True}, ensure_ascii=False, separators=(",", ":"))
        rows.append(dict(id=f"g{i}", root=0 if profile == "conversation" else i % 4,
                         request_json=body, input_bytes=len(body.encode()), output_reservation=64,
                         canonical_input_units=3 + sum(tokens + 3 for _, tokens in history),
                         canonical_output_units=16, service_ms=0 if profile.startswith("no_wait") else (100 if i % 3 == 0 else 10),
                         offset_s=0, timeout_s=5 if profile.startswith("no_wait") else 1.5,
                         length="long" if len(body.encode()) > 2500 else "short",
                         workload=profile, workflow_id="conversation" if profile == "conversation" else f"request{i}",
                         cost_case="synthetic_chat_framing", window=0,
                         warmup=profile.startswith("no_wait") and i < 5))
    if profile == "mismatch":
        for row in rows:
            row["canonical_input_units"] = len(payload(row))
            row["cost_case"] = "deliberately_mismatched_byte_input"
        tpm = len(payload(rows[0])) + 64 + 512
    elif profile.startswith("no_wait"):
        tpm = 1000000000
    else:
        tpm = max(sum(r["canonical_input_units"] + r["canonical_output_units"] for r in rows) + 512,
                  max(len(payload(r)) + r["output_reservation"] for r in rows))
    return rows, {"rpm": 1000000, "tpm": tpm}, 2


def response_body(row):
    common = dict(id="chatcmpl-" + row["id"], object="chat.completion.chunk", created=1, model="synthetic")
    events = [dict(common, choices=[dict(index=0, delta={"role": "assistant", "content": "fixture:" + row["id"]}, finish_reason=None)], usage=None),
              dict(common, choices=[dict(index=0, delta={}, finish_reason="stop")], usage=None),
              dict(common, choices=[], usage=dict(prompt_tokens=row["canonical_input_units"], completion_tokens=row["canonical_output_units"],
                                                 total_tokens=row["canonical_input_units"] + row["canonical_output_units"]))]
    return b"".join(b"data: " + encode(event) + b"\n\n" for event in events) + b"data: [DONE]\n\n"


class Fixture(Mock):
    def __init__(self, quota, rows):
        super().__init__(quota)
        self.plans = {r["id"]: r for r in rows}
        self.slots = asyncio.Semaphore(2)
        self.errors = []

    async def handle(self, reader, writer):
        self.writers.add(writer)
        try:
            while True:
                head = await reader.readuntil(b"\r\n\r\n")
                fields = dict(line.lower().split(b":", 1) for line in head.split(b"\r\n")[1:] if line)
                size = int(fields[b"content-length"])
                assert 0 <= size <= 1024 * 1024
                body = await reader.readexactly(size)
                row = self.plans[fields[b"x-benchmark-ingress"].strip().decode()]
                assert body == payload(row), "gateway changed request bytes or output cap"
                now = time.monotonic()
                self.ordinals[row["id"]] += 1
                cost = row["canonical_input_units"] + row["canonical_output_units"]
                attempt = dict(id=f'{row["id"]}/{self.ordinals[row["id"]]}', ingress_id=row["id"], received_s=now,
                               request_body_exact=True, body_bytes=size, actual_cost_fixture_units=cost, outcome="pending")
                self.attempts.append(attempt)
                task = asyncio.current_task()
                self.tasks.add(task)
                try:
                    budget = self.budget()
                    if budget["rpm_used"] >= self.quota["rpm"] or budget["tpm_used_fixture_units"] + cost > self.quota["tpm"]:
                        data = encode({"error": {"type": "rate_limit_error", "code": "rate_limit_exceeded", "message": "synthetic quota"}})
                        delay = math.ceil(max(.001, self.debits[0][0] + 60 - now)) if self.debits else 60
                        writer.write(f"HTTP/1.1 429 Too Many Requests\r\nContent-Type: application/json\r\nContent-Length: {len(data)}\r\nRetry-After: {delay}\r\n\r\n".encode() + data)
                        attempt["outcome"] = "rejected"
                    else:
                        self.debits.append((now, cost))
                        async with self.slots:
                            attempt["service_started_s"] = time.monotonic()
                            await asyncio.sleep(row["service_ms"] / 1000)
                            data = response_body(row)
                            writer.write(b"HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\nTransfer-Encoding: chunked\r\n\r\n")
                            writer.write(f"{len(data):x}\r\n".encode() + data + b"\r\n0\r\n\r\n")
                            attempt["outcome"] = "completed"
                    await writer.drain()
                except OSError:
                    attempt["outcome"] = "disconnected"
                finally:
                    attempt["ended_s"] = time.monotonic()
                    self.tasks.discard(task)
                    self.budget_samples.append(self.budget())
        except (OSError, asyncio.IncompleteReadError):
            pass
        except Exception as error:
            self.errors.append(repr(error))
        finally:
            self.writers.discard(writer)
            writer.close()
            with contextlib.suppress(OSError):
                await writer.wait_closed()


async def launch(binary, encoding, quota, unknown, upstream_port, temp, result, overhead=None):
    probe = await asyncio.start_server(lambda r, w: w.close(), "127.0.0.1", 0)
    port = probe.sockets[0].getsockname()[1]
    probe.close()
    await probe.wait_closed()
    text, _ = build_gateway_config(arm="production_actual", listen_port=port, upstream_port=upstream_port,
                                   quota=quota, launched_binary={}, cap=2, roots=4)
    text = "startup_hold_secs = 0\n" + text
    if encoding != "legacy":
        text = text.replace('id = "synthetic"\n', f'id = "synthetic"\ninput_estimator = "{encoding}"\n')
    if overhead is not None:
        text = text.replace(f'input_estimator = "{encoding}"\n', f'input_estimator = "{encoding}"\ninput_token_overhead = {overhead}\n')
    if unknown:
        text = text.replace(f'[quota.tpm]\nkind = "known"\nvalue = {quota["tpm"]}', '[quota.tpm]\nkind = "unknown"')
    config = temp / "config.toml"
    config.write_text(text)
    env = safe_environment(temp / "home")
    result["config"] = tomllib.loads(text)
    result["config_sha256"] = bench.digest(config)
    lookup = await asyncio.create_subprocess_exec(str(binary), "doctor", "--config", str(config), "--json",
                                                stdout=asyncio.subprocess.PIPE, stderr=asyncio.subprocess.PIPE, env=env)
    try:
        output, error = await asyncio.wait_for(lookup.communicate(), 10)
    finally:
        await bench.abort_startup(lookup)
    assert lookup.returncode == 0, error.decode()
    state = Path(json.loads(output)["state_directory"])
    state.mkdir(mode=0o700)
    token = state / "control-token"
    token.write_text("synthetic-benchmark-control")
    token.chmod(0o600)
    started = time.monotonic()
    with (temp / "gateway.log").open("wb") as log:
        proc = await asyncio.create_subprocess_exec(str(binary), "run", "--config", str(config), stdout=log, stderr=log, env=env)
    try:
        for _ in range(300):
            assert proc.returncode is None, (temp / "gateway.log").read_text()
            try:
                result["status_before"] = await bench.control(port)
                break
            except (OSError, asyncio.IncompleteReadError):
                await asyncio.sleep(.005)
        else:
            raise TimeoutError("gateway readiness")
        result["readiness_ms"] = (time.monotonic() - started) * 1000
        assert result["status_before"]["identity"]["fingerprint"] == result["config_sha256"]
        assert result["status_before"]["admission"]["accounting"] == "actual"
        return proc, port
    except BaseException:
        await bench.abort_startup(proc)
        raise


async def run(binary, encoding, profile, seed, result, overhead=None):
    rows, quota, window = cases(profile, seed)
    result.update(encoding=encoding, profile=profile, seed=seed, submitted=rows, outcomes=[], quota=quota, passed=False,
                  measurement_window_s=window, binary_sha256=bench.digest(binary), binary_bytes=binary.stat().st_size,
                  load_start=os.getloadavg(), upstream_retries=0, client_retries=0, cache_enabled=False)
    mock = Fixture(quota, rows)
    await mock.start()
    proc = monitor = None
    clients = []
    jobs = []
    try:
        with tempfile.TemporaryDirectory(prefix="deskquota-input-") as directory:
            proc, port = await launch(binary, encoding, quota, profile == "no_wait_unknown", mock.port, Path(directory), result, overhead)
            result["idle_resource"] = await bench.owned_resources(proc.pid)
            result["resource_samples"] = []
            async def observe():
                while True:
                    result["resource_samples"].append(await bench.owned_resources(proc.pid))
                    await asyncio.sleep(1)
            if profile == "original18":
                monitor = asyncio.create_task(observe())
            start = mock.origin = time.monotonic()
            result["workload_start_s"] = start
            result["measurement_start_s"] = start
            async def submit(row, client):
                await asyncio.sleep(max(0, start + row["offset_s"] - time.monotonic()))
                sent = time.monotonic()
                record = dict(id=row["id"], sent_s=sent, outcome="error", warmup=row.get("warmup", False),
                              scheduling_lag_ms=max(0, sent-start-row["offset_s"])*1000)
                try:
                    cancellation = row.get("cancel_after_ms")
                    response = await asyncio.wait_for(client.exchange("POST", f'/r/r{row["root"]}/v1/chat/completions', payload(row),
                                                                     {"x-benchmark-ingress": row["id"]}),
                                                      cancellation / 1000 if cancellation else row["timeout_s"])
                    record.update(status=response["status"], usage=response["usage"])
                    if response["status"] == 200:
                        assert response["body"] == response_body(row), "response bytes changed"
                        assert response["terminal_marker_s"] is not None and response["output"] == "fixture:" + row["id"]
                        assert response["usage"] == dict(status="observed", prompt_tokens=row["canonical_input_units"], completion_tokens=row["canonical_output_units"])
                        record.update(outcome="completed", payload_valid=True, body_exact=True,
                                      first_output_delta_ms=(response["first_output_delta_s"] - sent) * 1000)
                    else:
                        record["outcome"] = "rejected" if response["status"] == 429 else "timeout" if response["status"] in (408, 504) else "error"
                except TimeoutError:
                    record["outcome"] = "cancelled" if row.get("cancel_after_ms") else "timeout"
                    record["timeout_owner"] = "client"
                    await client.close(reset=True)
                except (OSError, asyncio.IncompleteReadError) as error:
                    record["error_class"] = type(error).__name__
                    await client.close(reset=True)
                finally:
                    record["ended_s"] = time.monotonic()
                    record["elapsed_ms"] = (record["ended_s"] - sent) * 1000
                    record["within_measurement"] = record["ended_s"] <= start + window
                    result["outcomes"].append(record)
            sequential = profile == "conversation" or profile.startswith("no_wait")
            clients = [Client(port) for _ in range(1 if sequential else len(rows))]
            if sequential:
                for i, row in enumerate(rows):
                    if profile.startswith("no_wait") and i == 5:
                        assert len(result["outcomes"]) == 5 and all(r["outcome"] == "completed" for r in result["outcomes"]), "warmup failed"
                        start = mock.origin = time.monotonic()
                        result["measurement_start_s"] = start
                    await submit(row, clients[0])
            else:
                jobs = [asyncio.create_task(submit(row, client)) for row, client in zip(rows, clients)]
                await asyncio.gather(*jobs)
            for client in clients:
                await client.close()
            await mock.drain()
            for _ in range(200):
                result["status_after"] = await bench.control(port)
                if result["status_after"]["active"] == 0 and result["status_after"]["admission"]["queue_length"] == 0:
                    break
                await asyncio.sleep(.01)
            else:
                raise AssertionError("request cleanup failed")
            result["resources_after"] = await bench.owned_resources(proc.pid)
            result["attempts"] = mock.attempts
            result["protocol_errors"] = mock.errors
            result["budget_samples"] = mock.budget_samples
            assert not mock.errors
            assert result["status_after"]["upstream_attempts"] == len(mock.attempts)
            assert not result["status_after"]["exact_cache"]["enabled"]
            assert int(result["status_after"]["admission"]["tpm_held"]) == 0
            bench.validate_run(result)
            result["summary"] = comparison.summarize(rows, result["outcomes"])
            if profile.startswith("no_wait"):
                measured = {r["id"] for r in rows if not r["warmup"]}
                result["warm_summary"] = comparison.summarize([r for r in rows if r["id"] in measured],
                                                              [r for r in result["outcomes"] if r["id"] in measured])
            result["attempt_outcomes"] = dict(collections.Counter(a["outcome"] for a in mock.attempts))
            await bench.stop_gateway(proc, port)
            assert bench.digest(binary) == result["binary_sha256"]
            result["passed"] = True
    finally:
        for job in jobs:
            if not job.done():
                job.cancel()
        if jobs:
            await asyncio.gather(*jobs, return_exceptions=True)
        if monitor:
            monitor.cancel()
            with contextlib.suppress(asyncio.CancelledError):
                await monitor
        for client in clients:
            await client.close()
        if proc:
            await bench.abort_startup(proc)
        try:
            await mock.close()
        finally:
            result["attempts"] = mock.attempts
            result["protocol_errors"] = mock.errors
            result["budget_samples"] = mock.budget_samples
            result["cleanup"] = {"gateway_exit": proc.returncode if proc else None,
                                 "pending_mock_tasks": len(mock.tasks)}


async def self_check():
    rows, quota, _ = cases("mismatch", 101)
    mock = Fixture(quota, rows)
    await mock.start()
    client = Client(mock.port)
    try:
        first, second = [await client.exchange("POST", "/v1/chat/completions", payload(row), {"x-benchmark-ingress": row["id"]}) for row in rows]
        assert first["status"] == 200 and first["body"] == response_body(rows[0])
        assert second["status"] == 429
        assert len(mock.attempts) == 2 and not mock.errors
        assert "빌드" in cases("burst", 101)[0][2]["request_json"]
        assert len(cases("original18", 101)[0]) == 18
    finally:
        await client.close()
        await mock.close()
    print("PASS: independent quota rejects overflow; original18, UTF-8, exact SSE and terminal checks")


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--binary", type=Path)
    parser.add_argument("--encoding", choices=["legacy", "utf8_bytes", "cl100k_base", "o200k_base"], default="legacy")
    parser.add_argument("--profile", choices=["original18", "burst", "conversation", "mismatch", "no_wait_known", "no_wait_unknown"], default="burst")
    parser.add_argument("--seed", type=int, default=101)
    parser.add_argument("--input-overhead", type=int)
    parser.add_argument("--output", type=Path)
    parser.add_argument("--self-check", action="store_true")
    args = parser.parse_args()
    if args.self_check:
        asyncio.run(self_check())
        return
    if not args.binary or not args.output:
        parser.error("--binary and --output are required")
    if args.input_overhead is not None and (args.encoding not in {"cl100k_base", "o200k_base"} or not 0 <= args.input_overhead <= 2**63 - 1):
        parser.error("--input-overhead requires BPE and a nonnegative TOML integer")
    args.binary = args.binary.resolve(strict=True)
    args.output.parent.mkdir(parents=True, exist_ok=True)
    result = dict(scope="Synthetic input-estimation mechanism; not provider accuracy or competitor performance proof",
                  platform=platform.platform(), started_utc=time.strftime("%Y-%m-%dT%H:%M:%SZ", time.gmtime()),
                  script_hashes={str(p.relative_to(ROOT)): bench.digest(p) for p in [Path(__file__), Path(bench.__file__), ROOT / "product/scripts/benchmark_http.py", Path(comparison.__file__)]})
    with args.output.open("x") as output:
        try:
            asyncio.run(run(args.binary, args.encoding, args.profile, args.seed, result, args.input_overhead))
        except Exception as error:
            result["error"] = repr(error)
            raise
        finally:
            json.dump(result, output, indent=2, ensure_ascii=False)
            output.write("\n")
    print(json.dumps({"output": str(args.output), "passed": result["passed"], "summary": result["summary"]["all"], "attempts": result["attempt_outcomes"]}))


if __name__ == "__main__":
    main()
