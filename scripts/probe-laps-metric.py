#!/usr/bin/env python3
"""Run the pinned LAPS timing function against one synthetic loopback response.

This verifies the measurement boundary, not LAPS inference performance.
Run with .venvs/laps-metric-probe/bin/python after installing aiohttp there.
"""

import argparse
import asyncio
import importlib.util
import json
import platform
import subprocess
import time
from pathlib import Path

import aiohttp

ROOT = Path(__file__).resolve().parents[1]
EXPECTED_SHA = "aa29962d501236ecd2a0c7a8470c5977df3fe057"


async def probe():
    repo = ROOT / "references/LAPS"
    sha = subprocess.check_output(
        ["git", "-C", str(repo), "rev-parse", "HEAD"], text=True
    ).strip()
    if sha != EXPECTED_SHA:
        raise RuntimeError("LAPS reference revision changed")
    source = repo / "bench_laps_prefill_throughput/bench_prefill_only.py"
    spec = importlib.util.spec_from_file_location("laps_benchmark_reference", source)
    module = importlib.util.module_from_spec(spec)
    # Do not add __pycache__ inside the unchanged reference checkout.
    exec(compile(source.read_bytes(), str(source), "exec"), module.__dict__)

    first_sent = asyncio.Event()
    release_tail = asyncio.Event()
    handler_done = asyncio.Event()
    observations = {}
    handler_errors = []

    async def handle(reader, writer):
        try:
            header = await asyncio.wait_for(reader.readuntil(b"\r\n\r\n"), 2)
            lengths = [
                line.partition(b":")[2].strip()
                for line in header.split(b"\r\n")
                if line.lower().startswith(b"content-length:")
            ]
            if len(lengths) != 1 or int(lengths[0]) > 4096:
                raise AssertionError("unexpected synthetic request framing")
            request = json.loads(await reader.readexactly(int(lengths[0])))
            assert request["sampling_params"]["max_new_tokens"] == 1
            assert header.startswith(b"POST /generate HTTP/1.1\r\n")
            # The text value is present in the first write. Completing the JSON
            # object is deliberately gated, so resp.json() must remain pending.
            body = b'{"text":"X","meta_info":{"completion_tokens":1}}'
            prefix, tail = body[:-1], body[-1:]
            writer.write(
                b"HTTP/1.1 200 OK\r\nContent-Type: application/json\r\n"
                + f"Content-Length: {len(body)}\r\n".encode()
                + b"Connection: close\r\n\r\n"
                + prefix
            )
            await writer.drain()
            observations["server_first_body_write"] = time.monotonic()
            first_sent.set()
            await asyncio.wait_for(release_tail.wait(), 2)
            observations["server_final_body_write"] = time.monotonic()
            writer.write(tail)
            await writer.drain()
        except Exception as error:
            handler_errors.append(type(error).__name__)
        finally:
            writer.close()
            await writer.wait_closed()
            handler_done.set()

    server = await asyncio.start_server(handle, "127.0.0.1", 0)
    port = server.sockets[0].getsockname()[1]
    task = None
    try:
        async with aiohttp.ClientSession(trust_env=False) as session:
            started = time.monotonic()
            task = asyncio.create_task(
                module.send_request(session, f"http://127.0.0.1:{port}", "fixture", 1, 3)
            )
            await asyncio.wait_for(first_sent.wait(), 2)
            await asyncio.sleep(0.1)
            pending_before_final_byte = not task.done()
            release_tail.set()
            chars, reported_ttft, success = await asyncio.wait_for(task, 2)
            returned_at = time.monotonic()
            await asyncio.wait_for(handler_done.wait(), 2)
        assert not handler_errors, handler_errors
        assert pending_before_final_byte
        assert success and chars == len("fixture")
        assert returned_at >= observations["server_final_body_write"]
        return {
            "check": "original_laps_send_request_measurement_boundary",
            "source_sha": sha,
            "source_path": str(source.relative_to(ROOT)),
            "python": platform.python_version(),
            "aiohttp": aiohttp.__version__,
            "network": "one synthetic loopback HTTP request",
            "inference_calls": 0,
            "pending_before_final_json_byte": pending_before_final_byte,
            "success": success,
            "server_first_body_write_ms": round(
                1000 * (observations["server_first_body_write"] - started), 3
            ),
            "server_final_body_write_ms": round(
                1000 * (observations["server_final_body_write"] - started), 3
            ),
            "function_return_ms": round(1000 * (returned_at - started), 3),
            "function_reported_ttft_ms": round(1000 * reported_ttft, 3),
            "passed": True,
            "limitations": [
                "Server write times are not measured client first-token arrival",
                "Verifies full JSON completion boundary in this function only",
                "Does not reproduce LAPS paper, engine or gateway performance",
                "Probe aiohttp version is separate from paper runtime",
            ],
        }
    finally:
        release_tail.set()
        if task is not None and not task.done():
            task.cancel()
            await asyncio.gather(task, return_exceptions=True)
        server.close()
        await server.wait_closed()


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--output", required=True, type=Path)
    args = parser.parse_args()
    result = asyncio.run(probe())
    args.output.parent.mkdir(parents=True, exist_ok=True)
    args.output.write_text(json.dumps(result, ensure_ascii=False, indent=2) + "\n")
    print(json.dumps(result, ensure_ascii=False, indent=2))


if __name__ == "__main__":
    main()
