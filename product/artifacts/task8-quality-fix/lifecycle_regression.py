"""Bounded real-child startup ownership regressions, never submit data requests."""
import asyncio
import json
from pathlib import Path
import sys
import tempfile

ROOT = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(ROOT / 'scripts'))
sys.dont_write_bytecode = True
import benchmark as b
from benchmark_http import Mock

OUT = Path(__file__).resolve().parent
BINARY = ROOT / 'target/release/llmgw'
REFERENCE = ROOT / 'target/bench/release/examples/bench_gateway'

async def check():
    results = []
    original_spawn, original_control, original_abort = asyncio.create_subprocess_exec, b.control, b.abort_startup
    owned = []
    async def spawn(*args, **kwargs):
        proc = await original_spawn(*args, **kwargs)
        if args[0] == str(BINARY): owned.append(proc)
        return proc
    asyncio.create_subprocess_exec = spawn
    try:
        for index, mode in enumerate(('startup_failure', 'repeated_readiness_cancellation', 'normal_transfer')):
            previous = len(owned)
            ready_entered, cleanup_entered, release_cleanup = asyncio.Event(), asyncio.Event(), asyncio.Event()
            fault = RuntimeError('injected readiness failure')
            async def health(port, path='status', method='GET'):
                if path == 'health' and mode != 'normal_transfer':
                    ready_entered.set()
                    if mode == 'startup_failure': raise fault
                    await asyncio.Event().wait()
                return await original_control(port, path, method)
            async def gated_abort(proc):
                cleanup_entered.set()
                await release_cleanup.wait()
                await original_abort(proc)
            b.control = health
            b.abort_startup = gated_abort if mode == 'repeated_readiness_cancellation' else original_abort
            with tempfile.TemporaryDirectory(prefix=mode+'-', dir=OUT) as tmp:
                if mode == 'normal_transfer':
                    mock = Mock(None)
                    await mock.start()
                    events = []
                    proc = None
                    try:
                        proc, port, _ = await asyncio.wait_for(b.start_gateway('production_rr', BINARY, REFERENCE, mock.port, None, tmp, events.append), 5)
                        assert proc.returncode is None, 'startup cleaned successfully transferred child'
                        assert events == [{'event': 'gateway_started', 'pid': proc.pid}]
                        assert (await b.control(port, 'health')) is not None
                        await b.stop_gateway(proc, port)
                        assert proc.returncode == 0
                        assert not mock.attempts
                    finally:
                        if proc is not None and proc.returncode is None: await original_abort(proc)
                        await mock.close()
                    detail = {'ownership_transferred_alive': True, 'normal_exit': 0}
                else:
                    task = asyncio.create_task(b.run_arm('production_rr', 920+index, 1, BINARY, REFERENCE, 'smoke', Path(tmp)))
                    if mode == 'repeated_readiness_cancellation':
                        await asyncio.wait_for(ready_entered.wait(), 5)
                        task.cancel('original cancellation')
                        await asyncio.wait_for(cleanup_entered.wait(), 5)
                        task.cancel('second cancellation')
                        release_cleanup.set()
                    try:
                        await asyncio.wait_for(task, 10)
                    except asyncio.CancelledError as error:
                        assert mode == 'repeated_readiness_cancellation'
                        assert error.args == ('original cancellation',), error.args
                    except RuntimeError as error:
                        assert error is fault, 'original startup exception replaced'
                    else: raise AssertionError('fault path returned success')
                    journal = [json.loads(line) for line in next(Path(tmp).glob('*.events.jsonl')).read_text().splitlines()]
                    failure = json.loads(next(Path(tmp).glob('*.failed.json')).read_text())
                    assert any(e['event'] == 'gateway_started' and e['pid'] == owned[-1].pid for e in journal)
                    assert not failure['attempts'] and not failure['outcomes']
                    detail = {'original_exception_preserved': True, 'failure': failure['failure'], 'pid_journaled': True}
                children = owned[previous:]
                assert len(children) == 1 and all(p.returncode is not None for p in children)
                results.append(dict(case=mode, owned_pids=[p.pid for p in children], returncodes=[p.returncode for p in children], owned_processes_cleaned=True, fresh_http_ingress=0, fresh_upstream=0, **detail))
    finally:
        asyncio.create_subprocess_exec, b.control, b.abort_startup = original_spawn, original_control, original_abort
        for proc in owned:
            if proc.returncode is None: await original_abort(proc)
        (OUT/'lifecycle-green-result.json').write_text(json.dumps(results, indent=2)+'\n')
    print(json.dumps(results, indent=2))
    print('lifecycle regression PASS')

asyncio.run(check())
