#!/usr/bin/env python3
"""Exercise reference proxy streaming/disconnect over ephemeral loopback TCP.

The reference startup/DB writer is disabled; only its HTTP request path is under
test. The upstream is an event-gated SSE fixture, never a model or paid API.
"""

import argparse
import asyncio
import json
import socket
import subprocess
import sys
from pathlib import Path

import httpx
import uvicorn
from starlette.applications import Starlette
from starlette.responses import StreamingResponse
from starlette.routing import Route

ROOT = Path(__file__).resolve().parents[1]
REFERENCE = ROOT / "references" / "overlaat"
sys.path.insert(0, str(REFERENCE))
from overlaat import queue_proxy as proxy
from overlaat.scheduler import Scheduler


async def wait_until(predicate, seconds=2.0):
    async with asyncio.timeout(seconds):
        while not predicate():
            await asyncio.sleep(0.005)


async def main(abort_on_disconnect):
    release_upstream = asyncio.Event()
    upstream_closed = asyncio.Event()
    arrivals = []
    events = []
    servers, server_tasks, sockets = [], [], []
    next_call = None

    async def upstream(request):
        payload = await request.json()
        tag = payload["tag"]
        arrivals.append(tag)

        async def body():
            try:
                yield b'data: {"choices":[{"delta":{"content":"fixture"}}]}\n\n'
                if tag == "held":
                    await release_upstream.wait()
                yield b'data: {"usage":{"prompt_tokens":1,"completion_tokens":1}}\n\ndata: [DONE]\n\n'
            finally:
                if tag == "held":
                    upstream_closed.set()

        return StreamingResponse(body(), media_type="text/event-stream")

    async def start(app):
        sock = socket.socket(socket.AF_INET, socket.SOCK_STREAM)
        sock.bind(("127.0.0.1", 0))
        sock.listen(128)
        sockets.append(sock)
        server = uvicorn.Server(uvicorn.Config(
            app, host="127.0.0.1", lifespan="off", http="h11",
            log_level="error", access_log=False, timeout_graceful_shutdown=1,
        ))
        servers.append(server)
        server_tasks.append(asyncio.create_task(server.serve(sockets=[sock])))
        await wait_until(lambda: server.started)
        return f"http://127.0.0.1:{sock.getsockname()[1]}"

    try:
        upstream_url = await start(Starlette(routes=[Route("/{path:path}", upstream, methods=["POST"])]))
        proxy.CAPS = {"model": 1}
        proxy.SCHEDULER_ON = True
        proxy.SCHED = Scheduler(budget=1.0, caps=proxy.CAPS)
        proxy.ABORT_ON_DISCONNECT = {"model": abort_on_disconnect}
        proxy.emit_event = lambda event: events.append(dict(event))
        proxy.app.state.client = httpx.AsyncClient(base_url=upstream_url, timeout=3, trust_env=False)
        proxy_url = await start(proxy.app)
        async with httpx.AsyncClient(base_url=proxy_url, timeout=3, trust_env=False) as client:
            async with client.stream("POST", "/v1/chat/completions", json={"model":"model", "stream":True, "tag":"held"}) as response:
                lines = response.aiter_lines()
                first_line = await anext(lines)
                assert response.status_code == 200
                assert first_line.startswith('data: {"choices"')
                first_event_before_release = not release_upstream.is_set() and not upstream_closed.is_set()
                assert first_event_before_release
                next_call = asyncio.create_task(client.post("/v1/chat/completions", json={"model":"model", "tag":"next"}))
                try:
                    await wait_until(lambda: proxy.METRICS["model"]["queue_depth"] == 1)
                except TimeoutError:
                    print(json.dumps({"stage":"queue-entry", "first_status":response.status_code, "first_line":first_line, "arrivals":arrivals, "in_flight":proxy.METRICS["model"]["in_flight"], "used":proxy.SCHED.used, "upstream_closed":upstream_closed.is_set(), "next_done":next_call.done(), "events":[event.get("outcome") for event in events]}), file=sys.stderr)
                    raise
                before_close = {"arrivals": list(arrivals), "in_flight": proxy.METRICS["model"]["in_flight"], "used": proxy.SCHED.used, "queue_depth": proxy.METRICS["model"]["queue_depth"]}
            before_release = None
            if not abort_on_disconnect:
                try:
                    await asyncio.wait_for(asyncio.shield(next_call), timeout=0.25)
                except TimeoutError:
                    pass
                before_release = {"arrivals": list(arrivals), "in_flight": proxy.METRICS["model"]["in_flight"], "used": proxy.SCHED.used, "queue_depth": proxy.METRICS["model"]["queue_depth"], "upstream_fixture_closed": upstream_closed.is_set(), "fixture_eof_allowed": release_upstream.is_set(), "next_completed": next_call.done(), "event_outcomes": [event.get("outcome") for event in events]}
                release_upstream.set()
            try:
                await wait_until(lambda: next_call.done(), seconds=2.0)
                next_result = await next_call
                next_status = next_result.status_code
            except TimeoutError:
                next_status = "not_completed_within_2s"
            observation = {
                "scope": "Reference HTTP path over TCP; lifespan and DB writer disabled; fake upstream",
                "reference_sha": subprocess.check_output(["git", "-C", str(REFERENCE), "rev-parse", "HEAD"], text=True).strip(),
                "transport": "Uvicorn h11 over loopback TCP",
                "abort_on_disconnect": abort_on_disconnect,
                "first_event_before_upstream_release": first_event_before_release,
                "before_client_close": before_close,
                "after_client_close_before_fixture_release": before_release,
                "terminal_snapshot": {"arrivals": list(arrivals), "in_flight": proxy.METRICS["model"]["in_flight"], "used": proxy.SCHED.used, "queue_depth": proxy.METRICS["model"]["queue_depth"], "upstream_fixture_closed": upstream_closed.is_set(), "next_status": next_status, "event_outcomes": [event.get("outcome") for event in events]},
                "limitations": ["No real engine cancellation claim", "No throughput or latency benchmark", "No startup/persistence/Windows validation"],
            }
            print(json.dumps(observation, indent=2))
    finally:
        release_upstream.set()
        if next_call is not None and not next_call.done():
            next_call.cancel()
            await asyncio.gather(next_call, return_exceptions=True)
        for server in servers:
            server.should_exit = True
        if server_tasks:
            await asyncio.wait_for(asyncio.gather(*server_tasks, return_exceptions=True), timeout=5)
        if hasattr(proxy.app.state, "client"):
            await proxy.app.state.client.aclose()
        for sock in sockets:
            sock.close()


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--abort-on-disconnect", choices=["true", "false"], default="true")
    args = parser.parse_args()
    asyncio.run(main(args.abort_on_disconnect == "true"))
