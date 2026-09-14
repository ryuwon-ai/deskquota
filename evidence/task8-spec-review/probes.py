"""Independent bounded Task8 payload and original-artifact probes. Synthetic only."""
import asyncio
import collections
import hashlib
import json
import math
import pathlib
import sys
import time

ROOT = pathlib.Path(__file__).resolve().parents[2]
OUT = pathlib.Path(__file__).resolve().parent
sys.path.insert(0, str(ROOT / 'product/scripts'))
from benchmark_http import request_once


async def payload_probes():
    content = b'data: {"choices":[{"delta":{"content":"fixture:probe"}}]}\n\n'
    done = b'data: [DONE]\n\n'
    metadata = b'{"object":"list","data":[{"id":"synthetic"}]}'
    cases = [
        ('generation', content + done, False, 'completed', None),
        ('metadata', metadata, True, 'completed', None),
        ('wrong_content', content.replace(b'fixture:probe', b'wrong') + done, False, 'error', None),
        ('wrong_metadata', metadata.replace(b'synthetic', b'other'), True, 'error', None),
        ('missing_terminal', content, False, 'error', None),
        ('invalid_json', b'data: {broken}\n\n' + done, False, 'error', None),
        ('truncated_content_length', content + done, False, 'error', 'length'),
        ('truncated_trailer', content + done, False, 'error', 'trailer'),
    ]
    results = []
    received = 0
    for name, body, is_metadata, expected, malformed in cases:
        async def respond(reader, writer):
            nonlocal received
            head = await reader.readuntil(b'\r\n\r\n')
            length = next(int(line.split(b':')[1]) for line in head.split(b'\r\n') if line.lower().startswith(b'content-length:'))
            await reader.readexactly(length)
            received += 1
            mime = 'application/json' if is_metadata else 'text/event-stream'
            if malformed == 'trailer':
                wire = f'HTTP/1.1 200 OK\r\nContent-Type: {mime}\r\nTransfer-Encoding: chunked\r\n\r\n{len(body):x}\r\n'.encode() + body + b'\r\n0\r\n'
            else:
                wire = f'HTTP/1.1 200 OK\r\nContent-Type: {mime}\r\nContent-Length: {len(body) + (1 if malformed == "length" else 0)}\r\n\r\n'.encode() + body
            writer.write(wire)
            await writer.drain()
            writer.close()
            await writer.wait_closed()
        server = await asyncio.start_server(respond, '127.0.0.1', 0)
        row = dict(id='probe', root=0, input_bytes=3, output_reservation=8, offset_s=0, timeout_s=.5, metadata=is_metadata)
        try:
            result = await request_once(row, server.sockets[0].getsockname()[1], 'direct', time.monotonic())
            assert result['outcome'] == expected, (name, result)
            if expected == 'completed':
                assert result['payload_valid'] is True
                marks = [result['first_http_ms'], result['first_body_ms']]
                if not is_metadata:
                    marks += [result['first_output_delta_ms'], result['terminal_marker_ms']]
                marks += [result['body_eof_ms'], result['elapsed_ms']]
                assert marks == sorted(marks)
            results.append(dict(case=name, expected=expected, result=result))
        finally:
            server.close()
            await server.wait_closed()
    assert received == 8
    return dict(cases=results, actual_http_received=received, owned_servers_closed=True)


def audit():
    pilot = ROOT / 'product/artifacts/pilot.json'
    manifest = json.loads(pilot.read_text())
    assert manifest['status'] == 'completed_composite_matrix'
    assert manifest['seeds'] == [1, 2, 3, 4, 5] and manifest['windows'] == 5
    expected = {(phase, seed, arm) for phase in ('no_wait', 'quota') for seed in range(1,6) for arm in ('direct','production_rr','benchmark_fifo','benchmark_rr')}
    observed = set()
    totals = collections.Counter()
    by_arm = {}
    runs = []
    fixture_files = {name: json.loads((ROOT / 'product/fixtures/workloads' / (name+'.json')).read_text()) for name in ('burst','mixed_lengths','shared_quota','cancellation')}
    for entry in manifest['runs']:
        path = pathlib.Path(entry['path'])
        if not path.is_absolute(): path = ROOT / 'product' / path
        data = path.read_bytes()
        assert hashlib.sha256(data).hexdigest() == entry['sha256']
        run = json.loads(data)
        assert run['id'] == entry['id'] and run['summary'] == entry['summary']
        key = (run['phase'], run['seed'], run['arm'])
        assert key not in observed
        observed.add(key)
        plans = {r['id']: r for r in run['submitted']}
        assert len(plans) == len(run['submitted']) == 100
        assert collections.Counter(o['id'] for o in run['outcomes']) == collections.Counter(plans.keys())
        assert len({a['id'] for a in run['attempts']}) == len(run['attempts'])
        assert all(a['outcome'] in ('completed','rejected','disconnected') for a in run['attempts'])
        for attempt in run['attempts']:
            plan = plans[attempt['ingress_id']]
            body = b'' if plan.get('metadata') else json.dumps(dict(model='synthetic',messages=[dict(role='user',content='x'*plan['input_bytes'])],max_tokens=plan['output_reservation'],stream=True),separators=(',',':')).encode()
            estimated = 0 if plan.get('metadata') else len(body)+plan['output_reservation']
            actual = 0 if plan.get('metadata') else math.ceil(len(body)*plan['actual_ratio'])+max(1,math.ceil(plan['output_reservation']*plan['actual_ratio']))
            assert (len(body), estimated, actual) == (attempt['body_bytes'],attempt['estimated_cost'],attempt['actual_cost_fixture_units'])
        if run['phase'] == 'quota':
            assert run['measurement_duration_s'] == 300 and run['quota_windows'] == 5
            for plan in plans.values():
                name = plan['workload']; index = int(plan['id'].rsplit('-',1)[1])
                template = fixture_files[name]['requests'][index]
                assert all(plan[k] == v for k,v in template.items())
                base = 60*plan['window']+fixture_files[name]['window_offset_s']+template['offset_ms']/1000
                assert 0 <= plan['offset_s']-base <= .010000001
            limit = run['start_monotonic_s'] + 300
            within = sum(o['outcome']=='completed' and o['ended_s'] <= limit for o in run['outcomes'])
            assert within == run['summary']['all']['within_measurement_completed']
            count = collections.Counter(o['outcome'] for o in run['outcomes'])
            assert dict(count) == run['summary']['all']['outcomes']
            arm = by_arm.setdefault(run['arm'],collections.Counter())
            arm.update(count); arm.update(within300=within, submitted=100, attempts=len(run['attempts']))
        else:
            assert len(run['warmup_outcomes']) == len(run['warmup_attempts']) == 5
            assert all(o['outcome']=='completed' and o['payload_valid'] for o in run['outcomes']+run['warmup_outcomes'])
            if run['arm'] != 'direct':
                for name in ('initial_status','post_warmup_status','final_status'):
                    assert run[name]['admission']['retained'] == 0
            totals['warmup'] += 5
        totals[run['phase']+'_submitted'] += len(plans)
        runs.append(dict(id=run['id'], sha256=entry['sha256']))
    assert observed == expected and len(runs)==40
    return dict(run_count=40, counts=dict(totals), quota_arms=by_arm, runs=runs, passed=True)


if __name__ == '__main__':
    result = dict(payload=asyncio.run(payload_probes()), artifacts=audit())
    (OUT / 'independent-probes.json').write_text(json.dumps(result,indent=2)+'\n')
    print(json.dumps(dict(payload_cases=8, actual_http_received=8, artifact_runs=40, counts=result['artifacts']['counts'], quota_arms=result['artifacts']['quota_arms'], passed=True)))
