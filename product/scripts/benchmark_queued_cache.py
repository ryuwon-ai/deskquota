#!/usr/bin/env python3
"""Paired release-HTTP queued-cache mechanism checks; synthetic loopback only."""
import argparse
import asyncio
from collections import Counter
import contextlib
import copy
import hashlib
import json
from pathlib import Path
import platform
import tempfile
import time

from benchmark import abort_startup, control, digest, distribution, owned_resources
from benchmark_cache import response_chunks
from benchmark_http import Client, encode
from measure_native import safe_environment

COUNT = 10
SERVICE_SECONDS = .1
USAGE = {"prompt_tokens": 3, "completion_tokens": 1}
CASES = [
    {"name": f"duplicate_{kind}_{quota}", "stream": kind == "sse", "cap": 1,
     "rpm": 1 if quota == "rpm1" else 100000, "cache": True, "distinct": False}
    for kind in ("json", "sse") for quota in ("high", "rpm1")
] + [
    {"name": "distinct_json", "stream": False, "cap": 1,
     "rpm": 100000, "cache": True, "distinct": True},
    {"name": "disabled_json", "stream": False, "cap": 1,
     "rpm": 100000, "cache": False, "distinct": False},
] + [
    {"name": f"duplicate_{kind}_cap3", "stream": kind == "sse", "cap": 3,
     "rpm": 100000, "cache": True, "distinct": False} for kind in ("json", "sse")
]


def sha(data):
    return hashlib.sha256(data).hexdigest()


def request_body(case, index):
    return encode({"model": "synthetic", "messages": [{"role": "user", "content":
                   f"synthetic-{index if case['distinct'] else 0}"}],
                   "max_tokens": 16, "temperature": 0, "stream": case["stream"]})


def check_response(response, streaming):
    exact = response["body"] == b"".join(response_chunks(streaming))
    usage = response["usage"] if streaming else json.loads(response["body"]).get("usage")
    valid = response["status"] == 200 and exact
    if streaming:
        valid = (valid and response["output"] == "synthetic result"
                 and response["terminal_marker_s"] is not None
                 and usage == {"status": "observed", **USAGE})
    else:
        valid = valid and usage == USAGE
    return valid, usage


class GatedFixture:
    """Hold responses until actual gateway queue state has been observed."""
    def __init__(self, service_seconds=SERVICE_SECONDS):
        self.service_seconds = service_seconds
        self.gate = asyncio.Event()
        self.attempts = []
        self.errors = []
        self.writers = set()
        self.tasks = set()
        self.server = None
        self.released_s = None

    async def start(self):
        self.server = await asyncio.start_server(self.handle, "127.0.0.1", 0)
        self.port = self.server.sockets[0].getsockname()[1]

    def release(self):
        self.released_s = time.monotonic()
        self.gate.set()

    async def handle(self, reader, writer):
        task = asyncio.current_task()
        self.tasks.add(task)
        self.writers.add(writer)
        attempt = None
        try:
            while True:
                head = await reader.readuntil(b"\r\n\r\n")
                fields = dict(line.lower().split(b":", 1)
                              for line in head.split(b"\r\n")[1:] if line)
                body = await reader.readexactly(int(fields[b"content-length"]))
                payload = json.loads(body)
                attempt = {"id": len(self.attempts), "received_s": time.monotonic(),
                           "request_sha256": sha(body), "request_bytes": len(body),
                           "outcome": "pending", "fixture_usage": USAGE}
                self.attempts.append(attempt)
                await self.gate.wait()
                await asyncio.sleep(self.service_seconds)
                streaming = payload["stream"]
                kind = "text/event-stream" if streaming else "application/json"
                writer.write(f"HTTP/1.1 200 OK\r\nContent-Type: {kind}\r\nTransfer-Encoding: chunked\r\n\r\n".encode())
                attempt["first_write_s"] = time.monotonic()
                chunks = response_chunks(streaming)
                for index, chunk in enumerate(chunks):
                    writer.write(f"{len(chunk):x}\r\n".encode() + chunk + b"\r\n")
                    await writer.drain()
                    if streaming and index == 1:
                        await asyncio.sleep(.005)
                writer.write(b"0\r\n\r\n")
                await writer.drain()
                attempt.update(outcome="completed", final_write_s=time.monotonic(),
                               response_sha256=sha(b"".join(chunks)))
                attempt = None
        except (OSError, asyncio.IncompleteReadError):
            if attempt is not None:
                attempt.update(outcome="disconnected", disconnected_s=time.monotonic())
        except asyncio.CancelledError:
            if attempt is not None:
                attempt.update(outcome="cancelled", cancelled_s=time.monotonic())
            raise
        except Exception as error:
            self.errors.append(type(error).__name__)
            if attempt is not None:
                attempt["outcome"] = "fixture_error"
        finally:
            self.writers.discard(writer)
            writer.close()
            with contextlib.suppress(OSError):
                await writer.wait_closed()
            self.tasks.discard(task)

    async def close(self):
        self.gate.set()
        self.server.close()
        await self.server.wait_closed()
        for writer in list(self.writers):
            writer.close()
        tasks = list(self.tasks)
        for task in tasks:
            task.cancel()
        await asyncio.gather(*tasks, return_exceptions=True)


async def wait_status(port, predicate, seconds):
    until = time.monotonic() + seconds
    last = None
    while time.monotonic() < until:
        try:
            last = await control(port)
            if predicate(last):
                return last
        except ConnectionRefusedError:
            pass
        await asyncio.sleep(.005)
    raise TimeoutError(f"gateway state predicate not reached; last state={last}")


def configuration(case, port, upstream_port):
    cache = "[cache]\nttl_secs = 300\nmax_history = 3\n" if case["cache"] else ""
    return f'''listen = "127.0.0.1:{port}"
concurrency = {case["cap"]}
startup_hold_secs = 0
{cache}[upstream]
api_base = "http://127.0.0.1:{upstream_port}/v1"
[upstream.auth]
mode = "none"
[quota.rpm]
kind = "known"
value = {case["rpm"]}
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
'''


async def launch(binary, case, fixture, directory, run):
    env = safe_environment(directory / "home")
    reservation = await asyncio.start_server(lambda r, w: w.close(), "127.0.0.1", 0)
    port = reservation.sockets[0].getsockname()[1]
    reservation.close()
    await reservation.wait_closed()
    config = configuration(case, port, fixture.port)
    run.update(config=config, config_sha256=sha(config.encode()))
    config_path = directory / "config.toml"
    config_path.write_text(config)
    doctor = await asyncio.create_subprocess_exec(str(binary), "doctor", "--config", str(config_path), "--json",
             stdout=asyncio.subprocess.PIPE, stderr=asyncio.subprocess.PIPE, env=env)
    try:
        output, _ = await asyncio.wait_for(doctor.communicate(), 10)
        assert doctor.returncode == 0, "configuration rejected"
    finally:
        await abort_startup(doctor)
    state = Path(json.loads(output)["state_directory"])
    state.mkdir(mode=0o700)
    token = state / "control-token"
    token.write_text("synthetic-benchmark-control")
    token.chmod(0o600)
    proc = await asyncio.create_subprocess_exec(str(binary), "run", "--config", str(config_path),
           stdout=asyncio.subprocess.DEVNULL, stderr=asyncio.subprocess.DEVNULL, env=env)
    return proc, port


async def request(port, case, index, run):
    client = Client(port)
    body = request_body(case, index)
    row = {"id": index, "submitted_s": time.monotonic(), "request_sha256": sha(body),
           "request_bytes": len(body), "deadline_seconds": run["deadline_seconds"], "outcome": "pending"}
    run["outcomes"].append(row)
    try:
        response = await asyncio.wait_for(client.exchange("POST", "/r/cache/v1/chat/completions", body, {}),
                                          run["deadline_seconds"])
        row.update(status=response["status"], response_sha256=sha(response["body"]),
                   response_bytes=len(response["body"]),
                   **{name: response[name] for name in ("first_http_s", "first_body_s",
                       "first_output_delta_s", "terminal_marker_s", "body_eof_s")})
        if response["status"] == 200:
            valid, usage = check_response(response, case["stream"])
            row.update(outcome="completed" if valid else "invalid_payload", payload_valid=valid, usage=usage)
        else:
            row["outcome"] = "rejected" if response["status"] == 429 else "error"
    except TimeoutError:
        row["outcome"] = "timeout"
    except asyncio.CancelledError:
        row["outcome"] = "cancelled"
        raise
    except Exception as error:
        row.update(outcome="error", error_class=type(error).__name__)
    finally:
        row["terminal_s"] = time.monotonic()
        row["elapsed_ms"] = (row["terminal_s"] - row["submitted_s"]) * 1000
        # Queued timeouts must cancel their gateway request instead of leaving a FIN-only half-close.
        await client.close(reset=row["outcome"] != "completed")


def validate(run):
    case, rows, attempts = run["case"], run["outcomes"], run["attempts"]
    assert len(rows) == COUNT and {row["id"] for row in rows} == set(range(COUNT)), "terminal denominator mismatch"
    limited_baseline = case["rpm"] == 1 and run["arm"] == "baseline"
    expected_completed = 1 if limited_baseline else COUNT
    assert Counter(row["outcome"] for row in rows) == {"completed": expected_completed, **({"timeout": 9} if limited_baseline else {})}, "unexpected ingress outcomes"
    assert all(row.get("payload_valid") for row in rows if row["outcome"] == "completed"), "invalid completion"
    expected_attempts = (1 if limited_baseline else case["cap"] if run["arm"] == "candidate"
                         and case["cache"] and not case["distinct"] else COUNT)
    assert len(attempts) == expected_attempts, "unexpected actual upstream attempts"
    assert [a["id"] for a in attempts] == list(range(len(attempts))), "duplicate attempt identifiers"
    assert all(a["outcome"] == "completed" for a in attempts), "unfinished upstream attempt"
    assert not run["fixture_errors"], "fixture failed"
    request_hashes = {row["request_sha256"] for row in rows}
    assert all(a["request_sha256"] in request_hashes for a in attempts), "unknown upstream request"
    if case["distinct"]:
        assert len({a["request_sha256"] for a in attempts}) == COUNT, "distinct request lost or replayed"
    before, after = run["status_before"], run["status_after"]
    admission = after["admission"]
    assert after["active"] == admission["active"] == admission["queue_length"] == 0, "request or queue leaked"
    assert int(admission["tpm_held"]) == 0, "unstarted token reservation leaked"
    assert int(admission["rpm_debited"]) == expected_attempts, "RPM charge mismatch"
    assert int(admission["tpm_debited"]) == expected_attempts * 4, "TPM charge mismatch"
    assert after["upstream_attempts"] - before["upstream_attempts"] == expected_attempts, "gateway/mock attempts differ"
    assert admission["accounting"] == "actual", "expected actual usage accounting"
    for field, expected in {"observed_input_tokens": expected_attempts * 3,
                            "observed_output_tokens": expected_attempts,
                            "usage_known": expected_attempts, "usage_unknown": 0,
                            "terminal_total": expected_attempts}.items():
        assert after[field] - before[field] == expected, f"replay changed upstream {field}"
    cache = after["exact_cache"]
    expected_hits = expected_completed - expected_attempts
    assert cache["enabled"] == case["cache"] and cache["hits"] == expected_hits, "cache hit accounting mismatch"
    if case["cache"]:
        assert cache["considered"] == COUNT, "cache policy denominator mismatch"
        assert cache["misses"] == COUNT - expected_hits, "cache miss accounting mismatch"
        assert sum(cache["bypasses"].values()) == 0, "eligible request bypassed cache policy"
        assert cache["considered"] == cache["hits"] + cache["misses"] + sum(cache["bypasses"].values()), "cache outcomes do not partition considered requests"
        assert cache["stores"] == expected_attempts, "cache stores mismatch"
        assert cache["entries"] <= 128 and cache["retained_bytes"] <= cache["budget_bytes"] == 4 * 1024 * 1024
    else:
        assert all(cache[field] == 0 for field in ("considered", "hits", "misses", "stores")), "disabled cache recorded policy outcomes"
        assert sum(cache["bypasses"].values()) == 0, "disabled cache recorded bypasses"


async def measure(binary, run):
    fixture = GatedFixture()
    await fixture.start()
    run["attempts"], run["fixture_errors"] = fixture.attempts, fixture.errors
    proc, tasks = None, []
    with tempfile.TemporaryDirectory(prefix="deskquota-queued-cache-") as temporary:
        try:
            proc, port = await launch(binary, run["case"], fixture, Path(temporary), run)
            run["status_before"] = await wait_status(port, lambda _: True, 5)
            run["resources_before"] = await owned_resources(proc.pid)
            run["start_s"] = time.monotonic()
            tasks = [asyncio.create_task(request(port, run["case"], index, run)) for index in range(COUNT)]
            cap = run["case"]["cap"]
            run["status_at_gate"] = await wait_status(port, lambda status:
                status["admission"]["active"] == cap and status["admission"]["queue_length"] == COUNT - cap
                and len(fixture.attempts) == cap, .5)
            run["resources_at_gate"] = await owned_resources(proc.pid)
            assert not any(task.done() for task in tasks), "request completed before gate release"
            fixture.release()
            run["gate_release_s"] = fixture.released_s
            await asyncio.gather(*tasks)
            run["status_after"] = await wait_status(port, lambda status:
                status["active"] == 0 and status["admission"]["active"] == 0
                and status["admission"]["queue_length"] == 0 and int(status["admission"]["tpm_held"]) == 0, 2)
            run["resources_after"] = await owned_resources(proc.pid)
            # Verify no late timeout request starts after client RST and observed queue drain.
            await asyncio.sleep(.03)
            run["status_after_quiet"] = await control(port)
            assert run["status_after_quiet"]["upstream_attempts"] == run["status_after"]["upstream_attempts"]
            validate(run)
            await control(port, "stop", "POST")
            await asyncio.wait_for(proc.wait(), 15)
            assert proc.returncode == 0, "gateway did not stop cleanly"
            run["gateway_exit_code"] = proc.returncode
        finally:
            for task in tasks:
                if not task.done():
                    task.cancel()
            await asyncio.gather(*tasks, return_exceptions=True)
            if proc is not None:
                await abort_startup(proc)
            await fixture.close()
            for row in run["outcomes"]:
                row["post_release_terminal_ms"] = ((row["terminal_s"] - fixture.released_s) * 1000
                                                   if fixture.released_s is not None else None)
            run["counts"] = dict(Counter(row["outcome"] for row in run["outcomes"]))
            run["summary"] = {
                "ingress_completion_ms_success_only": distribution([r["elapsed_ms"] for r in run["outcomes"] if r["outcome"] == "completed"]),
                "ingress_terminal_ms_all_outcomes": distribution([r["elapsed_ms"] for r in run["outcomes"]]),
                "post_release_completion_ms_success_only": distribution([r["post_release_terminal_ms"] for r in run["outcomes"] if r["outcome"] == "completed" and r["post_release_terminal_ms"] is not None]),
            }


async def self_check(report):
    fixture = GatedFixture(.01)
    await fixture.start()
    clients = [Client(fixture.port), Client(fixture.port)]
    tasks = []
    try:
        for index, streaming in enumerate((False, True)):
            case = {"stream": streaming, "distinct": False}
            tasks.append(asyncio.create_task(clients[index].exchange("POST", "/v1/chat/completions", request_body(case, 0), {})))
        until = time.monotonic() + 2
        while len(fixture.attempts) != 2 and time.monotonic() < until:
            await asyncio.sleep(.005)
        assert len(fixture.attempts) == 2 and not any(task.done() for task in tasks)
        fixture.release()
        responses = await asyncio.wait_for(asyncio.gather(*tasks), 2)
        assert all(check_response(response, bool(index))[0] for index, response in enumerate(responses))
        assert all(a["received_s"] < fixture.released_s < a["first_write_s"] for a in fixture.attempts)
        assert not fixture.errors
        report["fixture"] = {"gated_responses": 2, "json_sse_body_usage_exact": True,
                             "attempts": fixture.attempts, "release_s": fixture.released_s}
    finally:
        for task in tasks:
            if not task.done():
                task.cancel()
        await asyncio.gather(*tasks, return_exceptions=True)
        for client in clients:
            await client.close()
        await fixture.close()
    case = CASES[0]
    good = {"case": case, "arm": "candidate", "fixture_errors": [],
            "outcomes": [{"id": index, "outcome": "completed", "payload_valid": True, "request_sha256": "same"} for index in range(COUNT)],
            "attempts": [{"id": 0, "outcome": "completed", "request_sha256": "same"}],
            "status_before": dict(upstream_attempts=0, observed_input_tokens=0, observed_output_tokens=0, usage_known=0, usage_unknown=0, terminal_total=0),
            "status_after": dict(active=0, upstream_attempts=1, observed_input_tokens=3, observed_output_tokens=1, usage_known=1, usage_unknown=0, terminal_total=1,
                admission=dict(active=0, queue_length=0, tpm_held="0", rpm_debited="1", tpm_debited="4", accounting="actual"),
                exact_cache=dict(enabled=True, considered=10, hits=9, misses=1, bypasses={}, stores=1, entries=1, retained_bytes=100, budget_bytes=4 * 1024 * 1024))}
    validate(good)
    report["rejected_corruptions"] = []
    for name, mutate in [
        ("missing terminal", lambda value: value["outcomes"].pop()),
        ("duplicate terminal", lambda value: value["outcomes"][0].update(id=1)),
        ("unvalidated body", lambda value: value["outcomes"][0].update(payload_valid=False)),
        ("extra upstream attempt", lambda value: value["attempts"].append(copy.deepcopy(value["attempts"][0]))),
        ("unknown request", lambda value: value["attempts"][0].update(request_sha256="unknown")),
        ("held TPM leak", lambda value: value["status_after"]["admission"].update(tpm_held="1")),
        ("replay usage debit", lambda value: value["status_after"].update(observed_input_tokens=30)),
        ("wrong cache hits", lambda value: value["status_after"]["exact_cache"].update(hits=8)),
        ("wrong cache misses", lambda value: value["status_after"]["exact_cache"].update(misses=10)),
        ("wrong considered denominator", lambda value: value["status_after"]["exact_cache"].update(considered=19)),
    ]:
        broken = copy.deepcopy(good)
        mutate(broken)
        try:
            validate(broken)
        except AssertionError:
            report["rejected_corruptions"].append(name)
        else:
            raise AssertionError(f"accepted {name}")


async def matrix(args, report):
    for repeat in range(args.repeats):
        for case in CASES:
            for arm in (("baseline", "candidate") if repeat % 2 == 0 else ("candidate", "baseline")):
                run = {"repeat": repeat, "case": case, "arm": arm, "outcomes": [],
                       "deadline_seconds": .75 if case["rpm"] == 1 else 3, "passed": False}
                report["runs"].append(run)
                try:
                    await measure(getattr(args, arm), run)
                    run["passed"] = True
                except Exception as error:
                    run.update(error_class=type(error).__name__, error=str(error))
                print(f"{case['name']} pair={repeat + 1} arm={arm} passed={run['passed']} outcomes={run.get('counts')}", flush=True)
    assert all(run["passed"] for run in report["runs"]), "one or more runs failed; preserved in output"


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--baseline", type=Path)
    parser.add_argument("--candidate", type=Path)
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument("--repeats", type=int, default=5)
    parser.add_argument("--self-check", action="store_true")
    args = parser.parse_args()
    if not 1 <= args.repeats <= 20 or (not args.self_check and (not args.baseline or not args.candidate)):
        parser.error("repeats must be 1..20; baseline and candidate required unless self-check")
    report = {"schema": 1, "gateway_executed": not args.self_check, "passed": False,
              "scope": "synthetic queued-cache mechanism; deliberate gate and service delay are not proxy overhead, model quality or provider performance",
              "clock": "one host monotonic; original-ingress deadline includes queue/gate; post-release duration separately reported",
              "attribution": "identical requests have identical headers/body; upstream attempts map to body hash, not individual identical ingress IDs",
              "resources_scope": "process RSS/CPU snapshots before, while gated, and after; not peak/continuous sampling",
              "fixture_service_seconds": SERVICE_SECONDS, "cases": CASES, "repeats": args.repeats,
              "platform": platform.platform(), "runs": [],
              "source_sha256": {name: digest(Path(__file__).with_name(name)) for name in
                    (Path(__file__).name, "benchmark.py", "benchmark_cache.py", "benchmark_http.py", "measure_native.py")}}
    if not args.self_check:
        report["binaries"] = {}
        for arm in ("baseline", "candidate"):
            binary = getattr(args, arm).resolve(strict=True)
            setattr(args, arm, binary)
            report["binaries"][arm] = {"path": str(binary), "sha256_before": digest(binary), "bytes": binary.stat().st_size}
    args.output.parent.mkdir(parents=True, exist_ok=True)
    with args.output.open("x") as output:
        try:
            asyncio.run(self_check(report) if args.self_check else matrix(args, report))
            report["passed"] = True
        except Exception as error:
            report.update(error_class=type(error).__name__, error=str(error))
            raise
        finally:
            if not args.self_check:
                for arm, record in report["binaries"].items():
                    record["sha256_after"] = digest(getattr(args, arm))
                    if record["sha256_after"] != record["sha256_before"]:
                        report["passed"] = False
                        report["error"] = "binary changed during matrix"
            json.dump(report, output, indent=2)
            output.write("\n")
    assert report["passed"], report.get("error")


if __name__ == "__main__":
    main()
