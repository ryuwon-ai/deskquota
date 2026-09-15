"""Run the real comparison cleanup with pure stubs; no sockets or subprocesses."""
import asyncio
import collections
import contextlib
import importlib.util
import json
from pathlib import Path
import tempfile
from types import SimpleNamespace

HERE = Path(__file__).resolve().parent
ROOT = next(p for p in HERE.parents if (p / "scripts/compare-native-gateways.py").is_file())
spec = importlib.util.spec_from_file_location("comparison", ROOT / "scripts/compare-native-gateways.py")
m = importlib.util.module_from_spec(spec)
spec.loader.exec_module(m)
real_sleep = asyncio.sleep
events, clients = [], []
proc = SimpleNamespace(pid=-1, returncode=None)
mock = SimpleNamespace(port=0, plans={}, attempts=[], debits=collections.deque(),
                       resources=[], protocol_errors=[], budget_samples=[], discovery_requests=[])


async def noop():
    pass


async def close_mock():
    events.append("mock_closed")


class FakeClient:
    def __init__(self, port, arm, root):
        self.root, self.closed = root, False
        clients.append(self)

    async def close(self):
        self.closed = True


async def start_gateway(*args):
    return proc, 0, {"stub": True}


async def resources(pid):
    return None


async def abort(fake_proc):
    fake_proc.returncode = 0
    events.append("proc_reaped")


async def main():
    initial_tasks = set(asyncio.all_tasks())
    entered = asyncio.Event()

    async def request(row, *args):
        if row["id"] == "g0":
            mock.attempts.append({"ingress_id": "g0", "outcome": "completed"})
            return {"id": "g0", "outcome": "completed", "ended_s": m.time.monotonic()}
        entered.set()
        try:
            await asyncio.Event().wait()
        finally:
            events.append(row["id"] + "_cancelled")

    async def control(port):
        await entered.wait()
        return {"admission": {"barrier_root": None, "queue_length": 1, "active": 0}}

    async def short_sleep(delay):
        await real_sleep(min(delay * .001, .05))

    mock.start, mock.drain, mock.close = noop, noop, close_mock
    m.CanonicalMock, m.RoutedClient, m.request_once = lambda *_: mock, FakeClient, request
    m.bench.start_gateway, m.bench.control = start_gateway, control
    m.bench.owned_resources, m.bench.abort_startup = resources, abort
    m.tempfile = SimpleNamespace(TemporaryDirectory=lambda **_: contextlib.nullcontext(str(HERE)))
    with tempfile.TemporaryDirectory(prefix="stub-", dir=HERE) as temp:
        args = SimpleNamespace(output=Path(temp)/"run", arm="deskquota_backfill",
                               profile="metadata_barrier", seed=101, cap=2, cost_contract="byte_reserved")
        asyncio.sleep = short_sleep
        try:
            try:
                await m.run(args)
            except ExceptionGroup as error:
                assert any(isinstance(e, AssertionError) for e in error.exceptions)
            else:
                raise AssertionError("forced barrier failure was not propagated")
        finally:
            asyncio.sleep = real_sleep
        result = json.loads((args.output/"result.json").read_text())
    await real_sleep(0)
    assert result["status"] == "failed" and result["owned_processes_cleaned"]
    assert "g1_cancelled" in events and events[-2:] == ["proc_reaped", "mock_closed"]
    assert clients and all(c.closed for c in clients)
    assert not (set(asyncio.all_tasks()) - initial_tasks)
    (HERE/"forced-run-result.json").write_text(json.dumps(result, indent=2)+"\n")
    summary = {"passed": True, "scope": "pure stubs; no socket, gateway, API, or subprocess",
               "script_sha256": m.bench.digest(Path(__file__)), "events": events,
               "clients_closed": [c.root for c in clients], "pending_sibling_tasks": 0}
    (HERE/"result.json").write_text(json.dumps(summary, indent=2)+"\n")
    print(json.dumps(summary))


if __name__ == "__main__":
    asyncio.run(asyncio.wait_for(main(), 3))
