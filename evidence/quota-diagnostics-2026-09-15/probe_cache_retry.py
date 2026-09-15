#!/usr/bin/env python3
"""Paired release-binary identity check, not a provider or latency benchmark."""
import argparse
import asyncio
import contextlib
import hashlib
import json
from pathlib import Path
import sys
import tempfile

REPO = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(REPO / "product/scripts"))
from benchmark import abort_startup, control, digest
from benchmark_cache import response_chunks
from benchmark_http import Client, encode
from measure_native import safe_environment, free_port


async def measure(args, report):
    attempts, writers = [], set()

    async def fixture(reader, writer):
        writers.add(writer)
        try:
            while True:
                head = await reader.readuntil(b"\r\n\r\n")
                fields = dict(line.lower().split(b":", 1) for line in head.split(b"\r\n")[1:] if line)
                payload = json.loads(await reader.readexactly(int(fields[b"content-length"])))
                case = payload["messages"][0]["content"]
                attempts.append({"case": case, "retry_count": fields[b"x-stainless-retry-count"].strip().decode()})
                body = b"".join(response_chunks(payload["stream"]))
                kind = "text/event-stream" if payload["stream"] else "application/json"
                vary = "Vary: Accept-Encoding, X-Stainless-Retry-Count\r\n" if "vary" in case else ""
                writer.write(f"HTTP/1.1 200 OK\r\nContent-Type: {kind}\r\n{vary}Content-Length: {len(body)}\r\n\r\n".encode() + body)
                await writer.drain()
        except (OSError, asyncio.IncompleteReadError):
            pass
        finally:
            writers.discard(writer)
            writer.close()
            with contextlib.suppress(OSError):
                await writer.wait_closed()

    upstream = await asyncio.start_server(fixture, "127.0.0.1", 0)
    proc, client = None, None
    try:
        with tempfile.TemporaryDirectory(prefix="deskquota-retry-probe-") as temporary:
            directory, port = Path(temporary), free_port()
            env = safe_environment(directory / "home")
            config = directory / "config.toml"
            config.write_text(f'''listen = "127.0.0.1:{port}"
startup_hold_secs = 0
[cache]
ttl_secs = 300
max_history = 3
[upstream]
api_base = "http://127.0.0.1:{upstream.sockets[0].getsockname()[1]}/v1"
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
            lookup = await asyncio.create_subprocess_exec(str(args.binary), "doctor", "--config", str(config), "--json", stdout=asyncio.subprocess.PIPE, stderr=asyncio.subprocess.PIPE, env=env)
            try:
                output, _ = await asyncio.wait_for(lookup.communicate(), 10)
                assert lookup.returncode == 0, "configuration rejected"
            finally:
                await abort_startup(lookup)
            state = Path(json.loads(output)["state_directory"])
            state.mkdir(mode=0o700)
            token = state / "control-token"
            token.write_text("synthetic-benchmark-control")
            token.chmod(0o600)
            proc = await asyncio.create_subprocess_exec(str(args.binary), "run", "--config", str(config), stdout=asyncio.subprocess.DEVNULL, stderr=asyncio.subprocess.DEVNULL, env=env)
            try:
                for _ in range(100):
                    assert proc.returncode is None, "gateway exited"
                    try:
                        report["status_before"] = await control(port)
                        break
                    except ConnectionRefusedError:
                        await asyncio.sleep(.02)
                else:
                    raise TimeoutError("gateway readiness")
                client = Client(port)
                for stream in (False, True):
                    for vary in (False, True):
                        case = ("sse" if stream else "json") + ("-vary" if vary else "-reuse")
                        body = encode({"model": "synthetic", "messages": [{"role": "user", "content": case}], "max_tokens": 16, "stream": stream})
                        expected = b"".join(response_chunks(stream))
                        for retry in ("0", "1"):
                            before = len(attempts)
                            response = await asyncio.wait_for(client.exchange("POST", "/r/cache/v1/chat/completions", body, {"x-stainless-retry-count": retry}), 10)
                            row = {"case": case, "retry_count": retry, "status": response["status"], "upstream_attempts": len(attempts)-before, "body_exact": response["body"] == expected, "body_sha256": hashlib.sha256(response["body"]).hexdigest()}
                            report["requests"].append(row)
                            assert row["status"] == 200 and row["body_exact"]
                            assert row["upstream_attempts"] == (0 if args.arm == "candidate" and not vary and retry == "1" else 1)
                for _ in range(100):
                    report["status_after"] = await control(port)
                    if report["status_after"]["active"] == 0:
                        break
                    await asyncio.sleep(.01)
                else:
                    raise AssertionError("workers did not settle")
                expected_count = 6 if args.arm == "candidate" else 8
                assert len(attempts) == report["status_after"]["upstream_attempts"] == expected_count
                assert report["status_after"]["usage_known"] == expected_count
                assert report["status_after"]["observed_input_tokens"] == expected_count * 3
                assert report["status_after"]["observed_output_tokens"] == expected_count
                assert report["status_after"]["admission"]["tpm_debited"] == str(expected_count * 4)
                assert attempts == [{"case": row["case"], "retry_count": row["retry_count"]} for row in report["requests"] if row["upstream_attempts"]]
                report["fixture_attempts"] = attempts
                await control(port, "stop", "POST")
                await asyncio.wait_for(proc.wait(), 15)
                assert proc.returncode == 0
            finally:
                if client is not None:
                    await client.close()
                await abort_startup(proc)
    finally:
        upstream.close()
        await upstream.wait_closed()
        for writer in list(writers):
            writer.close()


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--binary", type=Path, required=True)
    parser.add_argument("--arm", choices=("baseline", "candidate"), required=True)
    parser.add_argument("--output", type=Path, required=True)
    args = parser.parse_args()
    args.binary = args.binary.resolve(strict=True)
    report = {"scope": __doc__, "arm": args.arm, "binary_sha256": digest(args.binary), "script_sha256": digest(Path(__file__)), "requests": [], "passed": False}
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
