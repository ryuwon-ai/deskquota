"""Sequential loopback comparison. Canonical fixture costs survive proxy reserialization."""
import argparse
import asyncio
import collections
import contextlib
import json
import math
import os
import platform
from pathlib import Path
import random
import sys
import tempfile
import time

ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT / "product/scripts"))
import benchmark as bench
from benchmark_http import Client, Mock, encode, request_once

CACHE = Path('/Users/ryuwon/Library/Caches/deskquota-reference-runtimes/2026-09-15')
BASELINE = Path('/Users/ryuwon/Library/Caches/deskquota-adoption-2026-09-15/baseline')
CURRENT = Path('/Users/ryuwon/Library/Caches/deskquota-adoption-2026-09-15/candidate')


def payload(row):
    return encode({"model": "synthetic", "messages": [{"role": "user", "content": "x" * row["input_bytes"]}],
                   "max_tokens": row["output_reservation"], "stream": True})


def workload(seed, profile, cost_contract='byte_reserved'):
    rng = random.Random(seed)
    rows = []
    for n in range(18 if profile != 'no_wait' else 100):
        size = rng.choice([96, 800, 1900]) if profile != 'no_wait' else 96
        row = dict(id=f'g{n}', root=n % 4, input_bytes=size,
                   output_reservation=rng.choice([64, 256, 768]) if profile != 'no_wait' else 32,
                   actual_ratio=1, service_ms=rng.choice([20, 400, 2500]) if profile != 'no_wait' else 0,
                   offset_s=round(rng.uniform(0, 32), 4) if profile != 'no_wait' else 0,
                   length='long' if size >= 1000 else 'short', cost_case='matching',
                   workload=profile, window=0, workflow_id=f'pair{n // 2}', timeout_s=125)
        rows.append(row)
    if profile == 'mixed':
        rows += [dict(rows[0], id=f'm{n}', root=n, metadata=True, input_bytes=0,
                      output_reservation=0, service_ms=10, offset_s=6 + n * 4,
                      length='short', workflow_id=f'metadata{n}') for n in range(4)]
    if profile != 'no_wait':
        rows[7]['cancel_after_ms'] = 100
    for row in rows:
        divisor = 4 if cost_contract == 'quarter_actual' else 1
        row['canonical_input_units'] = 0 if row.get('metadata') else math.ceil(len(payload(row)) / divisor)
        row['canonical_output_units'] = 0 if row.get('metadata') else math.ceil(row['output_reservation'] / divisor)
    return rows


class CanonicalMock(Mock):
    """Reuse fixture lifecycle/client; this wire loop uses fixed per-ingress admission costs."""
    def __init__(self, quota, cap):
        super().__init__(quota)
        self.slots = asyncio.Semaphore(cap)
        self.protocol_errors = []
        self.discovery_requests = []

    async def handle(self, reader, writer):
        self.writers.add(writer)
        try:
            while True:
                try:
                    head = await reader.readuntil(b'\r\n\r\n')
                except (asyncio.IncompleteReadError, ConnectionError):
                    break
                lines = head.decode('ascii').split('\r\n')
                headers = {k.lower(): v.strip() for k, v in (line.split(':', 1) for line in lines[1:] if line)}
                size = int(headers.get('content-length', '0'))
                if not 0 <= size <= 2 * 1024 * 1024:
                    raise ValueError('fixture body exceeds bound')
                body = await reader.readexactly(size)
                ingress = headers.get('x-benchmark-ingress', '')
                if not ingress and lines[0].split()[:2] == ['GET', '/v1/models']:
                    self.discovery_requests.append({'received_s': time.monotonic(), 'path': '/v1/models'})
                    data = encode({'object': 'list', 'data': [{'id': 'synthetic', 'object': 'model', 'created': 1, 'owned_by': 'fixture'}]})
                    writer.write(f'HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {len(data)}\r\n\r\n'.encode() + data)
                    await writer.drain()
                    continue
                if ingress not in self.plans:
                    raise ValueError('unmatched upstream attempt')
                plan = self.plans[ingress]
                metadata = plan.get('metadata', False)
                parsed = json.loads(body) if body else {}
                if not metadata:
                    assert parsed['messages'] == json.loads(payload(plan))['messages']
                    assert parsed.get('max_tokens', parsed.get('max_completion_tokens')) == plan['output_reservation']
                    assert parsed['stream'] is True
                now = time.monotonic()
                self.ordinals[ingress] += 1
                attempt = dict(id=f'{ingress}/{self.ordinals[ingress]}', ingress_id=ingress,
                               received_s=now, body_bytes=len(body), model=parsed.get('model'),
                               request_path=lines[0].split()[1], outcome='pending',
                               actual_cost_fixture_units=plan['canonical_input_units'] + plan['canonical_output_units'])
                self.attempts.append(attempt)
                task = asyncio.current_task()
                self.tasks.add(task)
                try:
                    budget = self.budget()
                    cost = attempt['actual_cost_fixture_units']
                    accepted = self.quota is None or (budget['rpm_used'] < self.quota['rpm'] and
                                                      budget['tpm_used_fixture_units'] + cost <= self.quota['tpm'])
                    if not accepted:
                        delay = max(.001, self.debits[0][0] + 60 - now) if self.debits else 60
                        data = encode({'error': {'type': 'rate_limit_error', 'code': 'rate_limit_exceeded', 'message': 'synthetic quota'}})
                        writer.write((f'HTTP/1.1 429 Too Many Requests\r\nContent-Type: application/json\r\nContent-Length: {len(data)}\r\nRetry-After: {math.ceil(delay)}\r\nretry-after-ms: {math.ceil(delay * 1000)}\r\n\r\n').encode() + data)
                        attempt['outcome'] = 'rejected'
                    else:
                        self.debits.append((now, cost))
                        async with self.slots:
                            attempt['service_started_s'] = time.monotonic()
                            await asyncio.sleep(plan['service_ms'] / 1000)
                            attempt['first_write_s'] = time.monotonic()
                            if metadata:
                                data = encode({'object': 'list', 'data': [{'id': 'synthetic'}]})
                                writer.write(f'HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {len(data)}\r\n\r\n'.encode() + data)
                            else:
                                writer.write(b'HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\nTransfer-Encoding: chunked\r\n\r\n')
                                common = {'id': 'chatcmpl-' + ingress, 'object': 'chat.completion.chunk', 'created': 1, 'model': 'synthetic'}
                                events = [dict(common, choices=[{'index': 0, 'delta': {'role': 'assistant', 'content': 'fixture:' + ingress}, 'finish_reason': None}], usage=None),
                                          dict(common, choices=[{'index': 0, 'delta': {}, 'finish_reason': 'stop'}], usage=None),
                                          dict(common, choices=[], usage={'prompt_tokens': plan['canonical_input_units'], 'completion_tokens': plan['canonical_output_units'], 'total_tokens': cost})]
                                for value in events:
                                    frame = b'data: ' + encode(value) + b'\n\n'
                                    writer.write(f'{len(frame):x}\r\n'.encode() + frame + b'\r\n')
                                end = b'data: [DONE]\n\n'
                                writer.write(f'{len(end):x}\r\n'.encode() + end + b'\r\n0\r\n\r\n')
                            attempt['outcome'] = 'completed'
                    await writer.drain()
                except (OSError, ConnectionError):
                    attempt['outcome'] = 'disconnected'
                finally:
                    attempt['ended_s'] = time.monotonic()
                    self.tasks.discard(task)
                    self.budget_samples.append(self.budget())
        except (OSError, ValueError, AssertionError, asyncio.IncompleteReadError) as error:
            self.protocol_errors.append(type(error).__name__)
        finally:
            self.writers.discard(writer)
            writer.close()
            with contextlib.suppress(OSError):
                await writer.wait_closed()


class RoutedClient(Client):
    def __init__(self, port, arm, root):
        super().__init__(port)
        self.arm, self.root = arm, root

    async def exchange(self, method, path, body, headers):
        headers = dict(headers)
        if self.arm == 'bifrost':
            path = '/openai' + path
            headers['x-bf-eh-x-benchmark-ingress'] = headers['x-benchmark-ingress']
        if self.arm == 'hivemind':
            headers['x-hivemind-agent-id'] = f'r{self.root}'
        return await super().exchange(method, path, body, headers)


async def launch_external(arm, upstream, port, quota, temp, log):
    provenance = {}
    if arm in ('hivemind', 'litellm'):
        name = 'hivemind-head' if arm == 'hivemind' else arm
        args = [str(CACHE/f'python/{name}-venv/bin/python'), str(ROOT/f'evidence/reference-python-2026-09-15/{name}-launch.py'),
                '--listen-port', str(port), '--mock-port', str(upstream),
                '--rpm', str(quota['rpm'] if quota else 1000000), '--tpm', str(quota['tpm'] if quota else 1000000000)]
        health = '/_health' if arm == 'hivemind' else '/health/liveliness'
        source = ROOT/'evidence/reference-python-2026-09-15'
        provenance = {'launcher_sha256': bench.digest(args[1]),
                      'dependency_freeze_sha256': bench.digest(source/f'{name}-requirements.txt'),
                      'preparation_manifest': json.loads((source/'manifest.json').read_text()),
                      'limits': {'rpm': quota['rpm'] if quota else 1000000, 'tpm': quota['tpm'] if quota else 1000000000},
                      'concurrency': 2, 'retries': 0}
        if arm == 'litellm': provenance['config_template'] = (source/'litellm-config.yaml').read_text()
    elif arm == 'bifrost':
        config_name = 'bifrost-quota-config.json' if quota else 'bifrost-config.json'
        config = json.loads((ROOT/'evidence/reference-native-2026-09-15'/config_name).read_text())
        config['providers']['openai']['network_config']['base_url'] = f'http://127.0.0.1:{upstream}'
        if quota:
            config['config_store']['config']['path'] = str(temp/'config.db')
        (temp/'config.json').write_text(json.dumps(config))
        # The prep report records exact local files required to avoid bootstrap network requests.
        for name in ('pricing.json', 'model_parameters.json'):
            (temp/name).write_text('{}')
        args = [str(CACHE/'native/bifrost-http-source'), '-app-dir', str(temp), '-host', '127.0.0.1', '-port', str(port)]
        health = '/health'
        provenance = {'effective_config': config,
                      'source_manifest': json.loads((ROOT/'evidence/reference-native-2026-09-15/sources.json').read_text())}
    else:
        raise ValueError(arm)
    env = {k: v for k, v in os.environ.items() if k in ('PATH', 'HOME', 'LANG', 'LC_ALL', 'TMPDIR', 'SYSTEMROOT')}
    env.update(NO_PROXY='127.0.0.1,localhost', DO_NOT_TRACK='1', OTEL_SDK_DISABLED='true', HF_HUB_OFFLINE='1')
    proc = await asyncio.create_subprocess_exec(*args, stdout=log, stderr=log, cwd=temp, env=env)
    try:
        for _ in range(400):
            if proc.returncode is not None:
                raise RuntimeError(f'{arm} exited during startup')
            client = Client(port)
            try:
                response = await asyncio.wait_for(client.exchange('GET', health, b'', {}), 1)
                if response['status'] == 200:
                    if arm == 'litellm':
                        provenance['effective_config'] = (CACHE/f'python/litellm-runtime/config-{port}.yaml').read_text()
                    return proc, {'command': args, 'binary_sha256': bench.digest(args[0]), **provenance,
                                  'config_note': 'provider contracts differ; see preparation reports'}
            except (OSError, ValueError, asyncio.IncompleteReadError, TimeoutError):
                pass
            finally:
                await client.close()
            await asyncio.sleep(.1)
        raise RuntimeError(f'{arm} readiness timeout')
    except BaseException:
        await bench.abort_startup(proc)
        raise


def summarize(rows, outcomes):
    submitted = {r['id']: r for r in rows}
    assert len(submitted) == len(rows) == len(outcomes)
    assert set(submitted) == {r['id'] for r in outcomes}
    groups = {}
    for name in ('all', 'metadata', 'generation_short', 'generation_long', 'generation_all'):
        selected = [r for r in outcomes if name == 'all' or
                    (name == 'metadata' and submitted[r['id']].get('metadata')) or
                    (name.startswith('generation') and not submitted[r['id']].get('metadata') and
                     (name == 'generation_all' or name.endswith(submitted[r['id']]['length'])))]
        times = [r['elapsed_ms'] for r in selected if r['outcome'] == 'completed']
        groups[name] = {'submitted': len(selected), 'outcomes': dict(collections.Counter(r['outcome'] for r in selected)),
                        'within_window_completed': sum(r['outcome'] == 'completed' and r['within_measurement'] for r in selected),
                        'success_latency_ms': bench.distribution(times), 'success_mean_ms': sum(times)/len(times) if times else None}
    return groups


async def run(args):
    directory = args.output.resolve()
    directory.mkdir(parents=True, exist_ok=False)
    rows = workload(args.seed, args.profile, args.cost_contract)
    quota = None if args.profile == 'no_wait' else {'rpm': 16, 'tpm': 6000}
    mock = CanonicalMock(quota, args.cap)
    mock.plans = {r['id']: r for r in rows}
    await mock.start()
    proc = monitor = pooled_client = None
    port = mock.port
    outcomes, resources = [], []
    result = {'arm': args.arm, 'profile': args.profile, 'seed': args.seed, 'cap': args.cap,
              'cost_contract': args.cost_contract, 'host': platform.platform(), 'load_start': os.getloadavg(),
              'submitted': rows, 'quota': quota, 'client_retries': 0, 'gateway_retries': 0,
              'cache': 'off', 'scope': 'Synthetic canonical-cost requests, not model tokens or agent tasks',
              'script_sha256': bench.digest(Path(__file__)), 'started_utc': time.strftime('%Y-%m-%dT%H:%M:%SZ', time.gmtime())}
    with tempfile.TemporaryDirectory(prefix='deskquota-compare-') as raw_temp:
        temp = Path(raw_temp)
        try:
            if args.arm.startswith('deskquota'):
                bench.CAP = args.cap
                old = args.arm == 'deskquota_old_backfill'
                binary, reference = (BASELINE/'llmgw', BASELINE/'bench_gateway') if old else (CURRENT/'llmgw', CURRENT/'examples/bench_gateway')
                policy = 'benchmark_rr' if args.arm == 'deskquota_rr' else 'benchmark_backfill'
                proc, port, result['config'] = await bench.start_gateway(policy, binary, reference, mock.port, quota, temp, lambda _: None)
                if quota:
                    await asyncio.sleep(60.1)
            elif args.arm != 'direct':
                probe = await asyncio.start_server(lambda r, w: w.close(), '127.0.0.1', 0)
                port = probe.sockets[0].getsockname()[1]
                probe.close()
                await probe.wait_closed()
                with (directory/'gateway.log').open('wb') as log:
                    proc, result['config'] = await launch_external(args.arm, mock.port, port, quota, temp, log)
            if proc:
                await asyncio.sleep(1)
                result['idle_resource'] = await bench.owned_resources(proc.pid)
            if args.profile == 'no_wait':
                pooled_client = RoutedClient(port, args.arm, 0)
                result['warmup_outcomes'] = []
                for i in range(5):
                    warmup = dict(rows[0], id=f'warmup{i}')
                    mock.plans[warmup['id']] = warmup
                    outcome = await request_once(warmup, port, 'gateway' if args.arm.startswith('deskquota') else 'direct', time.monotonic(), pooled_client)
                    result['warmup_outcomes'].append(outcome)
                    assert outcome['outcome'] == 'completed', 'warmup did not validate'
                result['warmup_attempts'], mock.attempts = mock.attempts, []
            # Each arm starts a fresh measured quota window; discovery is separately recorded.
            mock.debits.clear()
            start = mock.origin = time.monotonic()
            if proc: result['workload_start_resource'] = await bench.owned_resources(proc.pid)
            result['measurement_start_s'] = start
            async def observe():
                while proc:
                    value = await bench.owned_resources(proc.pid)
                    if value: resources.append(value)
                    await asyncio.sleep(1)
            monitor = asyncio.create_task(observe())
            async def submit(row, client=None):
                await asyncio.sleep(max(0, start + row['offset_s'] - time.monotonic()))
                internal = args.arm.startswith('deskquota')
                own = client is None
                if own: client = RoutedClient(port, args.arm, row['root'])
                outcome = await request_once(row, port, 'gateway' if internal else 'direct', start, client)
                if own: await client.close()
                outcome['within_measurement'] = outcome['ended_s'] <= start + 60
                outcomes.append(outcome)
            if args.profile == 'no_wait':
                for row in rows: await submit(row, pooled_client)
            else:
                await asyncio.gather(*(submit(row) for row in rows))
                await asyncio.sleep(max(0, start + 60 - time.monotonic()))
            await mock.drain()
            if proc: result['workload_end_resource'] = await bench.owned_resources(proc.pid)
            if mock.protocol_errors:
                raise RuntimeError('fixture protocol mismatch invalidates this comparison run')
            result['status'] = 'completed'
        except BaseException as error:
            result.update(status='failed', error_type=type(error).__name__, error=str(error))
            raise
        finally:
            if pooled_client: await pooled_client.close()
            if monitor:
                monitor.cancel()
                await asyncio.gather(monitor, return_exceptions=True)
            result.update(outcomes=outcomes, attempts=mock.attempts, resources=resources,
                          protocol_errors=mock.protocol_errors, budget_samples=mock.budget_samples,
                          discovery_requests=mock.discovery_requests,
                          resource_scope='gateway PID only; child memory is not included')
            if len(outcomes) == len(rows): result['summary'] = summarize(rows, outcomes)
            if proc:
                await bench.abort_startup(proc)
                result['owned_gateway_exit_code'] = proc.returncode
            await mock.close()
            result['owned_processes_cleaned'] = proc is None or proc.returncode is not None
            result['load_end'] = os.getloadavg()
            bench.write_json(directory/'result.json', result)
    print(json.dumps({'arm': args.arm, 'status': result['status'], 'summary': result.get('summary'), 'attempts': len(mock.attempts), 'protocol_errors': mock.protocol_errors}))


async def self_check():
    rows = workload(101, 'no_wait')[:3]
    rows = [dict(rows[0], id=f'check{i}') for i in range(3)]
    cost = rows[0]['canonical_input_units'] + rows[0]['canonical_output_units']
    mock = CanonicalMock({'rpm': 2, 'tpm': cost * 2}, 2)
    mock.plans = {r['id']: r for r in rows}
    await mock.start()
    client = Client(mock.port)
    try:
        for i, row in enumerate(rows):
            body = payload(row) if i != 1 else json.dumps(json.loads(payload(row)), indent=4).encode()
            response = await client.exchange('POST', '/v1/chat/completions', body, {'x-benchmark-ingress': row['id']})
            assert response['status'] == (429 if i == 2 else 200)
            if i < 2:
                assert response['usage']['status'] == 'observed', response['usage']
                assert response['output'] == 'fixture:' + row['id']
                assert response['terminal_marker_s'] is not None
        assert mock.attempts[0]['body_bytes'] != mock.attempts[1]['body_bytes']
        assert mock.attempts[0]['actual_cost_fixture_units'] == mock.attempts[1]['actual_cost_fixture_units'] == cost
        assert not mock.protocol_errors
        assert [a['outcome'] for a in mock.attempts] == ['completed', 'completed', 'rejected']
    finally:
        await client.close()
        await mock.close()
    print('PASS: canonical cost unaffected by JSON formatting; quota rejects third request; nullable SSE usage and completion validated; no gateway/API called.')


if __name__ == '__main__':
    if sys.argv[1:] == ['--self-check']:
        asyncio.run(self_check())
        raise SystemExit(0)
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--arm', choices=['direct', 'deskquota_rr', 'deskquota_backfill', 'deskquota_old_backfill', 'bifrost', 'hivemind', 'litellm'], required=True)
    parser.add_argument('--profile', choices=['generation', 'mixed', 'no_wait'], required=True)
    parser.add_argument('--seed', type=int, default=101)
    parser.add_argument('--cost-contract', choices=['byte_reserved', 'quarter_actual'], default='byte_reserved')
    parser.add_argument('--cap', type=int, choices=[1, 2], default=2)
    parser.add_argument('--output', type=Path, required=True)
    args = parser.parse_args()
    if args.cap != 2 and args.arm in ('hivemind', 'litellm', 'bifrost'):
        parser.error('external launchers are pinned to concurrency2; cap1 is a DeskQuota/direct negative control')
    if args.profile == 'mixed' and args.arm in ('hivemind', 'litellm', 'bifrost'):
        parser.error('external primary comparison is generation-only; different model catalog contracts require a separate probe')
    asyncio.run(run(args))
