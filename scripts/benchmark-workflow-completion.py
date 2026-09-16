#!/usr/bin/env python3
"""Retry-inclusive native workflow comparison; loopback fixtures only.

One invocation is one fresh profile/arm. Run usage_contract separately before
performance; its instance and quota state cannot leak into a measured run.
"""
import argparse
import asyncio
import collections
import contextlib
import copy
from email.utils import parsedate_to_datetime, formatdate
import hashlib
import importlib.util
import json
import math
import os
from pathlib import Path
import platform
import random
import sys
import tempfile
import time
import tomllib

ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT / 'product/scripts'))
import benchmark as bench
from benchmark_accounting import build_gateway_config
from benchmark_http import Client, encode

spec = importlib.util.spec_from_file_location('native_comparison', ROOT / 'scripts/compare-native-gateways.py')
reference = importlib.util.module_from_spec(spec)
spec.loader.exec_module(reference)
ARMS = ('direct', 'deskquota_local', 'deskquota_managed', 'bifrost', 'litellm',
        'bifrost_native_quota', 'litellm_native_quota')
PROFILES = ('original18_rolling', 'original18_bucket', 'not_exhausted',
            'recoverable429', 'recoverable503', 'chains', 'cancel_control', 'usage_contract')
TERMINAL = {'completed', 'permanent_failure', 'retry_budget_exhausted',
            'deadline_exceeded', 'cancelled', 'dependency_failed'}
TRANSIENT = {408, 429, 500, 502, 503, 504}


def sha(data):
    return hashlib.sha256(data).hexdigest()


def request_body(row):
    body = json.loads(reference.payload(row))
    if row.get('predecessor'):
        body['messages'].append({'role': 'assistant', 'content': 'fixture:' + row['predecessor']})
    if row.get('usage_required'):
        body['stream_options'] = {'include_usage': True}
    return encode(body)


def rows_for(profile, seed):
    if profile.startswith('original18'):
        rows = reference.workload(seed, 'generation', 'quarter_actual')
        for row in rows:
            row['workflow_id'] = row['id']
            row['deadline_secs'] = 125
            row['predecessor'] = None
        return rows
    rows = []
    count = 12 if profile == 'chains' else 1 if profile == 'usage_contract' else 3 if profile == 'cancel_control' else 8
    for i in range(count):
        workflow = i // 3 if profile == 'chains' else i
        row = dict(id=f'n{i}', workflow_id=f'w{workflow}', root=workflow % 4,
                   input_bytes=96, output_reservation=32, actual_ratio=.25,
                   service_ms=(400 if i % 3 == 0 else 20) if profile == 'chains' else
                              (20 if i % 2 == 0 else 100),
                   offset_s=i / 10 if profile.startswith('recoverable') else 0,
                   length='long' if profile == 'chains' and i % 3 == 0 else 'short',
                   deadline_secs=3 if profile == 'not_exhausted' else 5,
                   predecessor=f'n{i - 1}' if profile == 'chains' and i % 3 else None,
                   tool_delay_s=.05 if profile == 'chains' and i % 3 else 0,
                   usage_required=profile == 'usage_contract')
        if profile.startswith('recoverable'):
            row['service_ms'] = 100
        row['canonical_input_units'] = math.ceil(len(request_body(row)) / 4)
        row['canonical_output_units'] = 8
        rows.append(row)
    if profile == 'cancel_control':
        rows[0].update(gated=True, service_ms=0, root=0)
        rows[1].update(root=0, cancel_by_control=True)
        rows[2].update(root=1)
    return rows


class Ledger:
    """Fixture contract only: successful admissions debit, rejections are attempts."""
    def __init__(self, rpm, tpm, bucket=False):
        self.rpm, self.tpm, self.bucket = rpm, tpm, bucket
        self.debits = collections.deque()
        self.tokens = float(rpm)
        self.updated = None

    def admit(self, now, cost):
        while self.debits and self.debits[0][0] + 60 <= now:
            self.debits.popleft()
        if self.updated is not None:
            self.tokens = min(self.rpm, self.tokens + max(0, now - self.updated) * self.rpm / 60)
        self.updated = now
        used = sum(cost for _, cost in self.debits)
        rpm_delay = max(0, (1 - self.tokens) * 60 / self.rpm) if self.bucket else (
            max(0, self.debits[0][0] + 60 - now) if len(self.debits) >= self.rpm else 0)
        tpm_delay = 0
        remaining = used
        if used + cost > self.tpm:
            tpm_delay = 60
            for stamp, debit in self.debits:
                remaining -= debit
                if remaining + cost <= self.tpm:
                    tpm_delay = max(0, stamp + 60 - now)
                    break
        delay = max(rpm_delay, tpm_delay)
        accepted = delay <= 1e-9 and cost <= self.tpm
        snapshot = dict(at_s=now, rolling_rpm_used=len(self.debits), rolling_tpm_used=used,
                        rpm_tokens=self.tokens if self.bucket else None, cost=cost,
                        accepted=accepted, retry_delay_s=delay)
        if accepted:
            self.debits.append((now, cost))
            if self.bucket:
                self.tokens -= 1
        return accepted, max(.001, delay), snapshot


def response_body(row):
    common = dict(id='chatcmpl-' + row['id'], object='chat.completion.chunk', created=1, model='synthetic')
    events = [dict(common, choices=[dict(index=0, delta={'role': 'assistant', 'content': 'fixture:' + row['id']}, finish_reason=None)], usage=None)]
    if not row.get('missing_finish'):
        events.append(dict(common, choices=[dict(index=0, delta={}, finish_reason='stop')], usage=None))
    if not row.get('omit_usage'):
        usage = dict(prompt_tokens=row['canonical_input_units'], completion_tokens=row['canonical_output_units'])
        if row.get('wrong_usage'):
            usage['prompt_tokens'] += 1
        usage['total_tokens'] = usage['prompt_tokens'] + usage['completion_tokens']
        events.append(dict(common, choices=[], usage=usage))
    body = b''.join(b'data: ' + encode(event) + b'\n\n' for event in events)
    return body if row.get('missing_done') else body + b'data: [DONE]\n\n'


def normalized_request(actual, row):
    actual = copy.deepcopy(actual)
    if 'max_tokens' not in actual and 'max_completion_tokens' in actual:
        actual['max_tokens'] = actual.pop('max_completion_tokens')
    if not row.get('usage_required') and actual.get('stream_options') == {'include_usage': True}:
        del actual['stream_options']
    return actual


class Fixture(reference.CanonicalMock):
    """Keep the canonical fixture lifecycle; add explicit quota/outage contracts."""
    def __init__(self, rows, profile, cap=2, exact=False):
        quota = {'rpm': 16, 'tpm': 6000} if profile.startswith('original18') else {'rpm': 1000000, 'tpm': 1000000000}
        super().__init__(quota, cap)
        self.plans = {row['id']: row for row in rows}
        self.ledger = Ledger(**quota, bucket=profile == 'original18_bucket')
        self.profile, self.exact = profile, exact
        self.gate = asyncio.Event()
        self.connections = set()
        self.active_service = 0
        self.peak_service = 0

    def expected_request(self, body, row):
        expected = json.loads(request_body(row))
        actual = normalized_request(json.loads(body), row)
        if actual != expected:
            raise ValueError('semantic request changed: ' + json.dumps(actual, sort_keys=True))
        if self.exact and body != request_body(row):
            raise ValueError('DeskQuota changed request bytes')

    async def handle(self, reader, writer):
        connection = asyncio.current_task()
        self.connections.add(connection)
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
                if not 0 <= size <= 1024 * 1024:
                    raise ValueError('request size bound')
                body = await reader.readexactly(size)
                ingress = headers.get('x-benchmark-ingress', '')
                if not ingress and lines[0].split()[:2] == ['GET', '/v1/models']:
                    self.discovery_requests.append({'at_s': time.monotonic(), 'path': '/v1/models'})
                    data = encode({'object': 'list', 'data': [{'id': 'synthetic', 'object': 'model', 'created': 1, 'owned_by': 'fixture'}]})
                    await self.write_response(writer, 200, data, [])
                    continue
                if ingress not in self.plans:
                    raise ValueError('unknown fixture ingress')
                row = self.plans[ingress]
                now = time.monotonic()
                self.ordinals[ingress] += 1
                ordinal = self.ordinals[ingress]
                cost = row['canonical_input_units'] + row['canonical_output_units']
                attempt = dict(id=f'{ingress}/{ordinal}', ingress_id=ingress, received_s=now,
                               request_body_sha256=sha(body), expected_body_sha256=sha(request_body(row)),
                               request_semantic_valid=False, request_json=json.loads(body), request_body_exact=body == request_body(row),
                               actual_cost_fixture_units=cost, outcome='pending')
                self.attempts.append(attempt)
                self.tasks.add(connection)
                try:
                    self.expected_request(body, row)
                    attempt['request_semantic_valid'] = True
                    status = 200
                    response_headers = []
                    scripted = row.get('responses', [])
                    if ordinal <= len(scripted):
                        status, response_headers = scripted[ordinal - 1]
                    elif self.profile.startswith('recoverable') and now < self.origin + 1:
                        status = int(self.profile[-3:])
                        response_headers = [('retry-after-ms', str(math.ceil((self.origin + 1 - now) * 1000)))]
                    if status == 200:
                        accepted, delay, snapshot = self.ledger.admit(now, cost)
                        self.budget_samples.append(snapshot)
                        if not accepted:
                            status = 429
                            response_headers = [('Retry-After', str(math.ceil(delay))), ('retry-after-ms', str(math.ceil(delay * 1000)))]
                    attempt.update(status=status, response_headers=response_headers)
                    if status != 200:
                        data = encode({'error': {'type': 'rate_limit_error' if status == 429 else 'api_error',
                                                'code': 'rate_limit_exceeded' if status == 429 else 'fixture_error',
                                                'message': 'synthetic complete pre-output error'}})
                        await self.write_response(writer, status, data, response_headers)
                        attempt['outcome'] = 'rejected'
                    else:
                        async with self.slots:
                            self.active_service += 1
                            self.peak_service = max(self.peak_service, self.active_service)
                            attempt['service_started_s'] = time.monotonic()
                            try:
                                if row.get('gated'):
                                    await self.gate.wait()
                                await asyncio.sleep(row['service_ms'] / 1000)
                                attempt['first_write_s'] = time.monotonic()
                                data = response_body(row)
                                writer.write(b'HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\nTransfer-Encoding: chunked\r\n\r\n')
                                writer.write(f'{len(data):x}\r\n'.encode() + data + b'\r\n')
                                if row.get('partial'):
                                    await writer.drain()
                                    writer.close()
                                else:
                                    writer.write(b'0\r\n\r\n')
                                    await writer.drain()
                                attempt['outcome'] = 'completed'
                            finally:
                                self.active_service -= 1
                except ValueError:
                    attempt['outcome'] = 'contract_error'
                    raise
                except (ConnectionError, OSError):
                    attempt['outcome'] = 'disconnected'
                except asyncio.CancelledError:
                    attempt['outcome'] = 'cancelled_cleanup'
                    raise
                finally:
                    attempt['ended_s'] = time.monotonic()
                    self.tasks.discard(connection)
        except (OSError, asyncio.IncompleteReadError):
            pass
        except Exception as error:
            self.protocol_errors.append(str(error))
        finally:
            self.connections.discard(connection)
            self.writers.discard(writer)
            writer.close()
            with contextlib.suppress(OSError):
                await writer.wait_closed()

    async def write_response(self, writer, status, data, headers):
        writer.write((f'HTTP/1.1 {status} Fixture\r\nContent-Type: application/json\r\nContent-Length: {len(data)}\r\n' +
                      ''.join(f'{k}: {v}\r\n' for k, v in headers) + '\r\n').encode() + data)
        await writer.drain()

    async def close(self):
        self.server.close()
        await self.server.wait_closed()
        self.gate.set()
        for writer in list(self.writers):
            writer.close()
        if self.connections:
            done, pending = await asyncio.wait(list(self.connections), timeout=3)
            for task in pending:
                task.cancel()
            await asyncio.gather(*pending, return_exceptions=True)
        if self.tasks or self.connections:
            raise RuntimeError('fixture tasks survived cleanup')


def retry_delay(headers, node_id, index, seed, now_wall):
    delays = []
    forbidden = False
    for key, value in headers:
        key, value = key.lower(), value.strip(' \t')
        if key == 'x-should-retry' and value == 'false':
            forbidden = True
        if key not in ('retry-after', 'retry-after-ms'):
            continue
        try:
            delay = float(value) / (1000 if key.endswith('-ms') else 1)
        except ValueError:
            if key != 'retry-after':
                continue
            try:
                delay = max(0, parsedate_to_datetime(value).timestamp() - now_wall)
            except (ValueError, TypeError, OverflowError):
                continue
        if math.isfinite(delay) and delay >= 0:
            delays.append(delay)
    jitter = random.Random(f'{seed}/{node_id}/{index}').uniform(.75, 1)
    return forbidden, max(delays) if delays else (.5 * 2 ** (index - 1) * jitter), 'header_max' if delays else 'seeded_backoff'


def validate_response(row, response):
    if response['status'] != 200:
        raise ValueError('not a success response')
    body = response.get('body_utf8')
    if body is None:
        body = response['body'].decode('utf-8')
    output, usage, finishes, terminal = '', None, [], 0
    for frame in body.replace('\r\n', '\n').split('\n\n'):
        if not frame.strip() or frame.startswith(':'):
            continue
        if not frame.startswith('data: '):
            raise ValueError('unrecognized SSE frame')
        data = frame[6:]
        if terminal:
            raise ValueError('data after terminal')
        if data == '[DONE]':
            terminal += 1
            continue
        event = json.loads(data)
        if not isinstance(event, dict) or not isinstance(event.get('choices', []), list):
            raise ValueError('invalid SSE object/choices')
        if event.get('usage') is not None:
            if usage is not None:
                raise ValueError('duplicate usage')
            usage = event['usage']
        for choice in event.get('choices', []):
            if not isinstance(choice, dict) or not isinstance(choice.get('delta', {}), dict):
                raise ValueError('invalid SSE choice/delta')
            if choice.get('index') != 0:
                raise ValueError('unexpected choice')
            content = choice.get('delta', {}).get('content')
            if content is not None:
                if not isinstance(content, str):
                    raise ValueError('invalid content')
                output += content
            if choice.get('finish_reason') is not None:
                finishes.append(choice['finish_reason'])
    if output != 'fixture:' + row['id'] or finishes != ['stop'] or terminal != 1 or response.get('body_eof_s') is None:
        raise ValueError('output/finish/DONE/EOF mismatch')
    if usage is None:
        if row.get('usage_required'):
            raise ValueError('requested usage missing')
        return {'status': 'missing_unrequested'}
    expected = {'prompt_tokens': row['canonical_input_units'], 'completion_tokens': row['canonical_output_units']}
    if not isinstance(usage, dict) or any(type(usage.get(k)) is not int or usage[k] != v for k, v in expected.items()):
        raise ValueError('usage mismatch')
    if 'total_tokens' in usage and (type(usage['total_tokens']) is not int or usage['total_tokens'] != sum(expected.values())):
        raise ValueError('total usage mismatch')
    return {'status': 'observed', **expected}


async def execute_node(row, port, arm, origin, eligible, deadline, seed, attempts, client=None):
    owned = client is None
    client = client or reference.RoutedClient(port, arm.split('_native_quota')[0], row['root'])
    node = dict(id=row['id'], workflow_id=row['workflow_id'], eligible_s=eligible,
                deadline_s=deadline, outcome='permanent_failure', scheduling_lag_ms=max(0, time.monotonic() - eligible) * 1000)
    cancelled_at = eligible + row['cancel_after_ms'] / 1000 if row.get('cancel_after_ms') else None
    path = (f"/r/r{row['root']}" if arm.startswith('deskquota') else '') + '/v1/chat/completions'
    body = request_body(row)
    try:
        for index in range(1, 5):
            now = time.monotonic()
            cutoff = min(deadline, cancelled_at) if cancelled_at else deadline
            if now >= cutoff:
                node['outcome'] = 'cancelled' if cancelled_at and cutoff == cancelled_at else 'deadline_exceeded'
                break
            attempt = dict(id=f"{row['id']}/client{index}", node_id=row['id'], index=index,
                           sent_s=now, request_body_sha256=sha(body), outcome='pending')
            attempts.append(attempt)
            try:
                response = await asyncio.wait_for(client.exchange('POST', path, body, {'x-benchmark-ingress': row['id']}), cutoff - now)
                response['body_utf8'] = response.pop('body').decode('utf-8')
                attempt.update(response=response, status=response['status'], outcome='response')
                if response['status'] == 200:
                    node['usage'] = validate_response(row, response)
                    node.update(outcome='completed', payload_valid=True)
                    break
                retry_wall = time.time()
                retry_monotonic = time.monotonic()
                forbidden, delay, reason = retry_delay(response.get('headers', []), row['id'], index, seed, retry_wall)
                if forbidden or response['status'] not in TRANSIENT or response['output'] or response['terminal_marker_s'] is not None:
                    node['outcome'] = 'permanent_failure'
                    attempt['retry_reason'] = 'explicit_false' if forbidden else 'terminal_status_or_output'
                    break
                if index == 4:
                    node['outcome'] = 'retry_budget_exhausted'
                    break
                retry_at = retry_monotonic + delay
                attempt.update(retry_reason=reason, retry_delay_s=delay, retry_at_s=retry_at,
                               retry_computed_wall_s=retry_wall, retry_computed_monotonic_s=retry_monotonic)
                if retry_at >= cutoff:
                    node['outcome'] = 'cancelled' if cancelled_at and cutoff == cancelled_at else 'deadline_exceeded'
                    attempt['retry_suppressed'] = 'hint_past_original_cutoff'
                    break
                # A closed/error connection never carries a retry; independent nodes continue.
                await client.close()
                await asyncio.sleep(max(0, retry_at - time.monotonic()))
                attempt['retry_woke_s'] = time.monotonic()
            except TimeoutError:
                attempt['outcome'] = 'cancelled' if cancelled_at and cutoff == cancelled_at else 'deadline_exceeded'
                node['outcome'] = attempt['outcome']
                await client.close(reset=True)
                break
            except asyncio.CancelledError:
                attempt['outcome'] = 'cancelled'
                node['outcome'] = 'cancelled'
                await client.close(reset=True)
                break
            except (OSError, ValueError, asyncio.IncompleteReadError) as error:
                attempt.update(outcome='contract_or_transport_failure', error_type=type(error).__name__, error=str(error),
                               partial_response_observation='not retained when shared exchange raises; never replayed')
                node['error'] = str(error)
                await client.close(reset=True)
                break
            finally:
                attempt['ended_s'] = time.monotonic()
    finally:
        if owned:
            await client.close()
        node['ended_s'] = time.monotonic()
        node['elapsed_ms'] = (node['ended_s'] - eligible) * 1000
    return node


async def execute_workflows(rows, port, arm, origin, seed, records=None):
    nodes, workflows, attempts = (records['nodes'], records['workflows'], records['client_attempts']) if records is not None else ([], [], [])
    groups = collections.defaultdict(list)
    for row in rows:
        groups[row['workflow_id']].append(row)

    async def workflow(group):
        release = origin + group[0]['offset_s']
        deadline = release + group[0]['deadline_secs']
        await asyncio.sleep(max(0, release - time.monotonic()))
        predecessor = None
        for row in group:
            if predecessor and predecessor['outcome'] != 'completed':
                node = dict(id=row['id'], workflow_id=row['workflow_id'], outcome='dependency_failed',
                            not_started=True, deadline_s=deadline, eligible_s=None, ended_s=time.monotonic())
            else:
                eligible = predecessor['ended_s'] + row['tool_delay_s'] if predecessor else release
                await asyncio.sleep(max(0, eligible - time.monotonic()))
                node = await execute_node(row, port, arm, origin, eligible, deadline, seed, attempts)
            nodes.append(node)
            predecessor = node
        failed = next((node for node in nodes if node['workflow_id'] == group[0]['workflow_id'] and node['outcome'] != 'completed'), None)
        ended = time.monotonic()
        workflows.append(dict(id=group[0]['workflow_id'], released_s=release, deadline_s=deadline,
                              ended_s=ended, elapsed_ms=(ended - release) * 1000,
                              outcome=failed['outcome'] if failed else 'completed'))
    tasks = [asyncio.create_task(workflow(group)) for group in groups.values()]
    try:
        await asyncio.wait_for(asyncio.gather(*tasks), max(r['offset_s'] + r['deadline_secs'] for r in rows) + 2)
    finally:
        for task in tasks:
            if not task.done():
                task.cancel()
        await asyncio.gather(*tasks, return_exceptions=True)
    return nodes, workflows, attempts


def validate_run(run):
    rows = {row['id']: row for row in run['submitted']}
    nodes = {node['id']: node for node in run['nodes']}
    workflows = {workflow['id']: workflow for workflow in run['workflows']}
    if len(rows) != len(run['submitted']) or len(nodes) != len(run['nodes']) or set(rows) != set(nodes):
        raise ValueError('node denominator mismatch')
    if len(workflows) != len(run['workflows']) or set(workflows) != {r['workflow_id'] for r in rows.values()}:
        raise ValueError('workflow denominator mismatch')
    if run.get('gateway_retries', 0) != 0:
        raise ValueError('primary comparison requires gateway retries zero')
    origin = run['origin_s']
    control = run.get('cancellation_control') if run.get('profile') == 'cancel_control' else None
    if run.get('profile') == 'cancel_control' and (not isinstance(control, dict) or set(control.get('submitted_at_s', {})) != set(rows)):
        raise ValueError('cancellation control lacks observed submissions')
    if control is not None and (not run['arm'].startswith('deskquota') or run.get('cap') != 1):
        raise ValueError('cancellation control requires DeskQuota cap1')
    clients, upstream = collections.defaultdict(list), collections.defaultdict(list)
    if len({a['id'] for a in run['client_attempts']}) != len(run['client_attempts']):
        raise ValueError('duplicate client attempt')
    if len({a['id'] for a in run['upstream_attempts']}) != len(run['upstream_attempts']):
        raise ValueError('duplicate upstream attempt')
    for attempt in run['client_attempts']:
        if attempt['node_id'] not in rows or attempt['outcome'] == 'pending':
            raise ValueError('unknown/pending client attempt')
        if attempt['request_body_sha256'] != sha(request_body(rows[attempt['node_id']])):
            raise ValueError('changed client request')
        clients[attempt['node_id']].append(attempt)
    upstream_per_exchange = collections.Counter()
    for attempt in run['upstream_attempts']:
        if attempt['ingress_id'] not in rows or attempt['outcome'] not in {'completed', 'rejected', 'disconnected'}:
            raise ValueError('unknown/pending upstream attempt')
        row = rows[attempt['ingress_id']]
        actual = normalized_request(attempt['request_json'], row)
        if actual != json.loads(request_body(row)) or not attempt['request_semantic_valid']:
            raise ValueError('changed upstream request')
        if attempt['actual_cost_fixture_units'] != row['canonical_input_units'] + row['canonical_output_units']:
            raise ValueError('changed canonical cost')
        if attempt['expected_body_sha256'] != sha(request_body(rows[attempt['ingress_id']])):
            raise ValueError('changed expected request')
        if run['arm'].startswith('deskquota') and (not attempt['request_body_exact'] or attempt['request_body_sha256'] != attempt['expected_body_sha256']):
            raise ValueError('DeskQuota bytes mismatch')
        matches = [client for client in clients[attempt['ingress_id']]
                   if client['sent_s'] <= attempt['received_s'] <= client.get('response', {}).get('body_eof_s', client['ended_s'])]
        if len(matches) != 1:
            raise ValueError('upstream attempt lacks exactly one client exchange')
        upstream_per_exchange[matches[0]['id']] += 1
        if upstream_per_exchange[matches[0]['id']] > 1:
            raise ValueError('gateway or SDK replay despite primary retries zero')
        upstream[attempt['ingress_id']].append(attempt)
    for id, node in nodes.items():
        row = rows[id]
        if node['outcome'] not in TERMINAL or max(len(clients[id]), len(upstream[id])) > 4:
            raise ValueError('terminal outcome/attempt budget')
        workflow = workflows[row['workflow_id']]
        first_row = next(r for r in run['submitted'] if r['workflow_id'] == row['workflow_id'])
        release = control['submitted_at_s'][id] if control is not None else origin + first_row['offset_s']
        deadline = origin + 5 if control is not None else release + first_row['deadline_secs']
        if abs(workflow['released_s'] - release) > 1e-6 or abs(workflow['deadline_s'] - deadline) > 1e-6:
            raise ValueError('workflow changed original release or deadline')
        if control is not None and not origin <= release < deadline:
            raise ValueError('control submission outside original bound')
        predecessor = row.get('predecessor')
        if node['outcome'] == 'dependency_failed':
            if node.get('eligible_s') is not None:
                raise ValueError('failed dependency has eligibility')
        else:
            eligible = nodes[predecessor]['ended_s'] + row['tool_delay_s'] if predecessor else release
            if abs(node['eligible_s'] - eligible) > 1e-6 or abs(node['elapsed_ms'] - (node['ended_s'] - eligible) * 1000) > 1e-5:
                raise ValueError('node changed original eligibility or elapsed time')
        if node['deadline_s'] != workflow['deadline_s'] or (node.get('eligible_s') is not None and node['ended_s'] < node['eligible_s']):
            raise ValueError('node/workflow clock mismatch')
        previous = None
        for index, attempt in enumerate(clients[id], 1):
            if attempt['index'] != index or not node['eligible_s'] <= attempt['sent_s'] <= node['deadline_s'] or attempt['ended_s'] < attempt['sent_s']:
                raise ValueError('attempt index/deadline')
            response = attempt.get('response')
            if response and not attempt['sent_s'] <= response['first_http_s'] <= response['body_eof_s'] <= attempt['ended_s']:
                raise ValueError('HTTP boundary clocks inconsistent')
            if previous:
                if previous.get('retry_at_s') is None or attempt['sent_s'] + 1e-6 < previous['retry_at_s']:
                    raise ValueError('retry before lower bound')
            if attempt.get('retry_at_s') is not None:
                forbidden, delay, reason = retry_delay(attempt['response'].get('headers', []), id, index, run.get('seed', 101), attempt['retry_computed_wall_s'])
                if forbidden or attempt['response']['status'] not in TRANSIENT or abs(delay - attempt['retry_delay_s']) > 1e-6 or abs(attempt['retry_at_s'] - attempt['retry_computed_monotonic_s'] - delay) > 1e-6:
                    raise ValueError('retry decision does not match recorded headers')
                if attempt.get('retry_woke_s', attempt['retry_at_s']) + 1e-6 < attempt['retry_at_s']:
                    raise ValueError('retry woke early')
            previous = attempt
        if node['outcome'] == 'completed':
            if not clients[id] or node['ended_s'] > node['deadline_s'] or not node.get('payload_valid'):
                raise ValueError('late/unverified completion')
            if validate_response(row, clients[id][-1]['response']) != node.get('usage'):
                raise ValueError('reported usage mismatch')
            if sum(a['outcome'] == 'completed' for a in upstream[id]) != 1:
                raise ValueError('completed node/upstream linkage')
        if node['outcome'] == 'dependency_failed' and (clients[id] or upstream[id] or not node.get('not_started')):
            raise ValueError('dependency failure attempted')
        if row.get('cancel_by_control') and (node['outcome'] != 'cancelled' or upstream[id]):
            raise ValueError('control cancelled node reached provider or was not cancelled')
        if node['outcome'] == 'cancelled' and not (row.get('cancel_after_ms') or row.get('cancel_by_control')):
            raise ValueError('unaccounted cancellation')
        predecessor = row.get('predecessor')
        if predecessor and clients[id]:
            prior = nodes[predecessor]
            if prior['outcome'] != 'completed' or clients[id][0]['sent_s'] + 1e-6 < prior['ended_s'] + row['tool_delay_s']:
                raise ValueError('dependency executed early')
    for workflow in workflows.values():
        selected = [n for n in nodes.values() if n['workflow_id'] == workflow['id']]
        if workflow['ended_s'] < max(n['ended_s'] for n in selected) or abs(workflow['elapsed_ms'] - (workflow['ended_s'] - workflow['released_s']) * 1000) > 1e-5:
            raise ValueError('workflow clock mismatch')
        if workflow['outcome'] not in TERMINAL:
            raise ValueError('workflow nonterminal')
        if (workflow['outcome'] == 'completed') != all(n['outcome'] == 'completed' for n in selected):
            raise ValueError('workflow completion mismatch')
        if workflow['outcome'] == 'completed' and workflow['ended_s'] > workflow['deadline_s']:
            raise ValueError('workflow deadline exceeded')
    if control is not None:
        cancelled_count = sum(len(upstream[id]) for id, row in rows.items() if row.get('cancel_by_control'))
        if control['cancelled_provider_attempts'] != cancelled_count or cancelled_count:
            raise ValueError('final cancelled-provider count mismatch')
    if run.get('quota'):
        ledger = Ledger(**run['quota'], bucket=run.get('quota_contract') == 'continuous_rpm_bucket_and_rolling_tpm')
        for attempt in sorted(run['upstream_attempts'], key=lambda a: a['received_s']):
            if attempt['status'] == 200 and not ledger.admit(attempt['received_s'], attempt['actual_cost_fixture_units'])[0]:
                raise ValueError('independent provider quota overspend')
        if run.get('peak_provider_service', 0) > run['cap']:
            raise ValueError('provider service cap exceeded')
    if run.get('protocol_errors') or run.get('fixture_pending_tasks', 0):
        raise ValueError('fixture failure/pending tail')
    return True


def summarize(run):
    completed = [row for row in run['workflows'] if row['outcome'] == 'completed']
    nodes = {r['id']: r for r in run['nodes']}
    groups = {}
    for field in ('root', 'length'):
        for value in sorted({r[field] for r in run['submitted']}, key=str):
            selected = [nodes[r['id']] for r in run['submitted'] if r[field] == value]
            groups[f'{field}:{value}'] = {'planned': len(selected), 'outcomes': dict(collections.Counter(r['outcome'] for r in selected)),
                                        'success_latency_ms': bench.distribution([r['elapsed_ms'] for r in selected if r['outcome'] == 'completed'])}
    return dict(planned_workflows=len(run['workflows']), planned_nodes=len(run['nodes']),
                workflow_outcomes=dict(collections.Counter(r['outcome'] for r in run['workflows'])),
                node_outcomes=dict(collections.Counter(r['outcome'] for r in run['nodes'])),
                completed_by_seconds={str(t): sum(r['elapsed_ms'] <= t * 1000 for r in completed) for t in (2, 5, 10)},
                completed_by_deadline=len(completed), success_workflow_latency_ms=bench.distribution([r['elapsed_ms'] for r in completed]),
                all_work_completion_ms=(max(r['ended_s'] for r in completed) - min(r['released_s'] for r in run['workflows'])) * 1000 if len(completed) == len(run['workflows']) else None,
                client_http_attempts=len(run['client_attempts']), upstream_attempts=len(run['upstream_attempts']),
                provider_rejected_attempts=sum(a['outcome'] == 'rejected' for a in run['upstream_attempts']),
                admitted_fixture_token_units=sum(a['actual_cost_fixture_units'] for a in run['upstream_attempts'] if a['status'] == 200),
                completed_usage=dict(collections.Counter(n.get('usage', {}).get('status') for n in run['nodes'] if n['outcome'] == 'completed')),
                scheduling_lag_ms=bench.distribution([n['scheduling_lag_ms'] for n in run['nodes'] if 'scheduling_lag_ms' in n]),
                groups=groups)


async def free_port():
    server = await asyncio.start_server(lambda r, w: w.close(), '127.0.0.1', 0)
    port = server.sockets[0].getsockname()[1]
    server.close()
    await server.wait_closed()
    return port


def safe_environment():
    env = {k: v for k, v in os.environ.items() if k in ('PATH', 'HOME', 'LANG', 'LC_ALL', 'TMPDIR', 'SYSTEMROOT')}
    env.update(NO_PROXY='127.0.0.1,localhost', DO_NOT_TRACK='1', OTEL_SDK_DISABLED='true',
               HF_HUB_OFFLINE='1', TRANSFORMERS_OFFLINE='1', LITELLM_LOCAL_MODEL_COST_MAP='True', LITELLM_MODE='DEV')
    return env


async def launch(args, fixture, temp, directory, result):
    arm = args.arm
    if arm == 'direct':
        return None, fixture.port
    port = await free_port()
    if arm in ('bifrost', 'bifrost_native_quota', 'litellm_native_quota'):
        kind = arm.split('_native_quota')[0]
        quota = fixture.quota if arm.endswith('_native_quota') else None
        with (directory / 'gateway.log').open('wb') as log:
            proc, result['gateway'] = await reference.launch_external(kind, fixture.port, port, quota, temp, log)
        result['gateway']['policy'] = 'historical_native_quota_diagnostic' if quota else 'provider_managed'
        result['gateway']['effective_config_sha256'] = sha(encode(result['gateway']['effective_config']))
        return proc, port
    if arm == 'litellm':
        config = dict(model_list=[dict(model_name='synthetic', litellm_params=dict(model='openai/synthetic',
                      api_base=f'http://127.0.0.1:{fixture.port}/v1', api_key='synthetic-loopback-only',
                      max_retries=0, max_parallel_requests=2))],
                      router_settings=dict(routing_strategy='simple-shuffle', num_retries=0, default_max_parallel_requests=2,
                                           timeout=120, enable_pre_call_checks=False),
                      litellm_settings=dict(num_retries=0, cache=False, telemetry=False, request_timeout=120),
                      general_settings=dict(forward_client_headers_to_llm_api=True, disable_spend_logs=True))
        # JSON is valid YAML; stdlib avoids adding a YAML dependency to the harness.
        path = temp / 'litellm-config.yaml'
        path.write_text(json.dumps(config, indent=2) + '\n')
        binary = reference.CACHE / 'python/litellm-venv/bin/litellm'
        command = [str(binary), '--host', '127.0.0.1', '--port', str(port), '--config', str(path), '--telemetry', 'False', '--num_workers', '1']
        result['gateway'] = dict(command=command, binary_sha256=bench.digest(binary), effective_config=config,
                                 effective_config_sha256=bench.digest(path), policy='provider_managed',
                                 dependency_freeze_sha256=bench.digest(ROOT / 'evidence/reference-python-2026-09-15/litellm-requirements.txt'))
        health = '/health/liveliness'
    else:
        cap = 1 if args.profile == 'cancel_control' else 2
        text, _ = build_gateway_config(arm='production_actual', listen_port=port, upstream_port=fixture.port,
                                      quota=fixture.quota, launched_binary={}, cap=cap, roots=4)
        text = 'startup_hold_secs = 0\n' + text
        text = text.replace('id = "synthetic"\n', 'id = "synthetic"\ninput_estimator = "cl100k_base"\ninput_token_overhead = 256\n')
        if arm == 'deskquota_managed':
            for quota_name in ('rpm', 'tpm'):
                text = text.replace(f'[quota.{quota_name}]\nkind = "known"\nvalue = {fixture.quota[quota_name]}', f'[quota.{quota_name}]\nkind = "unknown"')
        path = temp / 'gateway.toml'
        path.write_text(text)
        result['gateway'] = dict(binary_sha256=bench.digest(args.binary), effective_config=tomllib.loads(text),
                                 effective_config_sha256=bench.digest(path), policy=arm, features=['bpe'])
        lookup = await asyncio.create_subprocess_exec(str(args.binary), 'doctor', '--config', str(path), '--json',
                    stdout=asyncio.subprocess.PIPE, stderr=asyncio.subprocess.PIPE, env=safe_environment())
        try:
            output, error = await asyncio.wait_for(lookup.communicate(), 10)
        finally:
            await bench.abort_startup(lookup)
        if lookup.returncode:
            raise RuntimeError('doctor failed: ' + error.decode())
        state = Path(json.loads(output)['state_directory'])
        state.mkdir(mode=0o700)
        token = state / 'control-token'
        token.write_text('synthetic-benchmark-control')
        token.chmod(0o600)
        command = [str(args.binary), 'run', '--config', str(path)]
        result['gateway']['command'] = command
        health = None
    with (directory / 'gateway.log').open('wb') as log:
        proc = await asyncio.create_subprocess_exec(*command, stdout=log, stderr=log, cwd=temp, env=safe_environment())
    try:
        for _ in range(400):
            if proc.returncode is not None:
                raise RuntimeError(f'{arm} exited during startup; see gateway.log')
            client = Client(port)
            try:
                if health:
                    response = await asyncio.wait_for(client.exchange('GET', health, b'', {}), 1)
                    if response['status'] == 200:
                        return proc, port
                else:
                    status = await bench.control(port)
                    if status['identity']['fingerprint'] != result['gateway']['effective_config_sha256'] or status['admission']['accounting'] != 'actual':
                        raise ValueError('runtime config/accounting mismatch')
                    result['status_before'] = status
                    return proc, port
            except (OSError, asyncio.IncompleteReadError, TimeoutError):
                pass
            finally:
                await client.close()
            await asyncio.sleep(.05)
        raise TimeoutError('gateway readiness')
    except BaseException:
        await bench.abort_startup(proc)
        raise


async def cancel_control(rows, fixture, port, arm, origin, seed):
    attempts, tasks, observations = [], [], []
    submitted_at = {}
    async def wait_for(predicate):
        end = time.monotonic() + 3
        while time.monotonic() < end:
            status = await bench.control(port)
            observations.append({'at_s': time.monotonic(), 'admission': status['admission']})
            if predicate(status['admission']):
                return
            await asyncio.sleep(.01)
        raise AssertionError('control queue precondition missing')
    def submit(row):
        submitted_at[row['id']] = time.monotonic()
        task = asyncio.create_task(execute_node(row, port, arm, origin, submitted_at[row['id']], origin + 5, seed, attempts))
        tasks.append(task)
        return task
    try:
        submit(rows[0])
        await wait_for(lambda a: a['active'] == 1 and fixture.active_service == 1)
        cancelled = submit(rows[1])
        await wait_for(lambda a: a['queue_length'] == 1)
        submit(rows[2])
        await wait_for(lambda a: a['queue_length'] == 2)
        cancel_at = time.monotonic()
        cancelled.cancel()
        await cancelled  # execute_node sends a real RST and returns the cancelled outcome.
        await wait_for(lambda a: a['queue_length'] == 1)
        if any(a['ingress_id'] == rows[1]['id'] for a in fixture.attempts):
            raise AssertionError('queued cancelled node reached provider')
        gate_released = time.monotonic()
        fixture.gate.set()
        nodes = await asyncio.wait_for(asyncio.gather(*tasks), 3)
        await wait_for(lambda a: a['queue_length'] == 0 and a['active'] == 0)
        await fixture.drain()
        cancelled_count = sum(a['ingress_id'] == rows[1]['id'] for a in fixture.attempts)
        if cancelled_count:
            raise AssertionError('cancelled node reached provider after gate release')
        if [n['outcome'] for n in nodes] != ['completed', 'cancelled', 'completed']:
            raise AssertionError('cancellation outcomes')
        workflows = [dict(id=n['workflow_id'], released_s=n['eligible_s'], deadline_s=n['deadline_s'], ended_s=n['ended_s'],
                          elapsed_ms=n['elapsed_ms'], outcome=n['outcome']) for n in nodes]
        return nodes, workflows, attempts, {'cancel_at_s': cancel_at, 'gate_released_s': gate_released,
                 'submitted_at_s': submitted_at, 'observations': observations,
                 'cancelled_provider_attempts': cancelled_count, 'scope': 'functional cap1 control, outside performance denominator'}
    finally:
        fixture.gate.set()
        for task in tasks:
            if not task.done():
                task.cancel()
        await asyncio.gather(*tasks, return_exceptions=True)


async def run(args):
    directory = args.output.resolve()
    directory.mkdir(parents=True, exist_ok=False)
    rows = rows_for(args.profile, args.seed)
    fixture = Fixture(rows, args.profile, 1 if args.profile == 'cancel_control' else 2, args.arm.startswith('deskquota'))
    await fixture.start()
    proc = monitor = None
    resources = []
    result = dict(arm=args.arm, profile=args.profile, seed=args.seed, submitted=rows,
                  client_attempt_limit=4, upstream_attempt_limit=4, gateway_retries=0, cache='off',
                  quota=fixture.quota, quota_contract='continuous_rpm_bucket_and_rolling_tpm' if fixture.ledger.bucket else 'rolling_60_seconds',
                  cap=1 if args.profile == 'cancel_control' else 2,
                  scope='Synthetic completion with original deadlines and client retries; no model quality or real-provider claim',
                  fresh_fixture_and_gateway_instances=True, host=platform.platform(),
                  started_utc=time.strftime('%Y-%m-%dT%H:%M:%SZ', time.gmtime()), load_start=os.getloadavg(),
                  script_sha256=bench.digest(__file__), reused_sources={str(p.relative_to(ROOT)): bench.digest(p) for p in
                    [ROOT / 'scripts/compare-native-gateways.py', ROOT / 'product/scripts/benchmark_http.py', ROOT / 'product/scripts/benchmark.py', ROOT / 'product/scripts/benchmark_accounting.py']},
                  nodes=[], workflows=[], client_attempts=[])
    with tempfile.TemporaryDirectory(prefix='deskquota-workflow-') as tempdir:
        temp = Path(tempdir)
        try:
            proc, port = await launch(args, fixture, temp, directory, result)
            if proc:
                result['idle_resource'] = await bench.owned_resources(proc.pid)
            if args.profile == 'not_exhausted':
                warmup_client = reference.RoutedClient(port, args.arm.split('_native_quota')[0], 0)
                warmup_rows = [dict(rows[0], id=f'warmup{i}', workflow_id=f'warmup{i}', service_ms=0) for i in range(3)]
                fixture.plans.update({r['id']: r for r in warmup_rows})
                warmup_attempts = []
                try:
                    warmup_nodes = []
                    for row in warmup_rows:
                        start = time.monotonic()
                        warmup_nodes.append(await execute_node(row, port, args.arm, start, start, start + 3, args.seed, warmup_attempts, warmup_client))
                    if any(n['outcome'] != 'completed' for n in warmup_nodes):
                        raise RuntimeError('pooled warmup failed')
                    result['warmup'] = {'nodes': warmup_nodes, 'client_attempts': warmup_attempts, 'upstream_attempts': fixture.attempts,
                                        'scope': 'separate denominator, high limits; not requested-usage preflight'}
                    fixture.attempts = []
                    fixture.budget_samples = []
                    fixture.ledger = Ledger(**fixture.quota)
                finally:
                    await warmup_client.close()
            origin = fixture.origin = time.monotonic()
            result['origin_s'] = origin
            if proc:
                result['workload_start_resource'] = await bench.owned_resources(proc.pid)
            async def observe():
                while proc:
                    sample = await bench.owned_resources(proc.pid)
                    if sample:
                        resources.append(sample)
                    await asyncio.sleep(.5)
            monitor = asyncio.create_task(observe())
            if args.profile == 'cancel_control':
                nodes, workflows, attempts, control = await cancel_control(rows, fixture, port, args.arm, origin, args.seed)
                result['cancellation_control'] = control
            else:
                nodes, workflows, attempts = await execute_workflows(rows, port, args.arm, origin, args.seed, result)
            result.update(nodes=nodes, workflows=workflows, client_attempts=attempts)
            await fixture.drain()
            if proc:
                result['workload_end_resource'] = await bench.owned_resources(proc.pid)
                if args.arm.startswith('deskquota'):
                    result['status_after'] = await bench.control(port)
            result['status'] = 'completed'
        except BaseException as failure:
            result.update(status='failed', error_type=type(failure).__name__, error=str(failure))
        finally:
            if monitor:
                monitor.cancel()
                await asyncio.gather(monitor, return_exceptions=True)
            cleanup_errors = []
            if proc:
                try:
                    await bench.abort_startup(proc)
                except Exception as failure:
                    cleanup_errors.append(str(failure))
                result['owned_gateway_exit_code'] = proc.returncode
            try:
                await fixture.close()
            except Exception as failure:
                cleanup_errors.append(str(failure))
            if cleanup_errors:
                result.update(status='failed', cleanup_errors=cleanup_errors)
            result.update(upstream_attempts=fixture.attempts, protocol_errors=fixture.protocol_errors,
                          budget_samples=fixture.budget_samples, discovery_requests=fixture.discovery_requests,
                          resources=resources, resource_scope='gateway PID RSS/CPU; no child RSS aggregation',
                          fixture_pending_tasks=len(fixture.tasks) + len(fixture.connections), peak_provider_service=fixture.peak_service,
                          owned_processes_cleaned=proc is None or proc.returncode is not None, load_end=os.getloadavg())
            if result.get('gateway'):
                command = result['gateway']['command']
                result['gateway']['binary_sha256_after'] = bench.digest(command[0])
                if result['gateway']['binary_sha256'] != result['gateway']['binary_sha256_after']:
                    result.update(status='failed', error='binary changed during run')
            result['script_sha256_after'] = bench.digest(__file__)
            result['reused_sources_after'] = {name: bench.digest(ROOT / name) for name in result['reused_sources']}
            config_names = ('gateway.toml', 'litellm-config.yaml', 'config.json')
            result['written_configs_after'] = {name: {'sha256': bench.digest(temp / name), 'text': (temp / name).read_text()} for name in config_names if (temp / name).is_file()}
            try:
                validate_run(result)
                if result['reused_sources'] != result['reused_sources_after']:
                    raise ValueError('reused source changed during run')
                if result['script_sha256'] != result['script_sha256_after']:
                    raise ValueError('driver changed during run')
                result['summary'] = summarize(result)
                result['audit_valid'] = True
            except (KeyError, ValueError) as failure:
                result.update(status='failed', audit_valid=False, audit_error=str(failure))
            bench.write_json(directory / 'result.json', result)
    print(json.dumps({key: result.get(key) for key in ('arm', 'profile', 'status', 'audit_valid', 'summary', 'error', 'audit_error')}))
    return 0 if result['status'] == 'completed' and result.get('audit_valid') else 1


async def self_check(output):
    checks, samples = [], []
    # Pure ledger trace, independent expected admissions and exact recovery boundary.
    rolling, bucket = Ledger(2, 100), Ledger(2, 100, True)
    assert [rolling.admit(t, 10)[0] for t in (0, 0, 30, 60)] == [True, True, False, True]
    assert [bucket.admit(t, 10)[0] for t in (0, 0, 30, 60)] == [True, True, True, True]
    tpm = Ledger(100, 15, True)
    assert tpm.admit(0, 10)[0] and not tpm.admit(1, 10)[0] and tpm.admit(60, 10)[0]
    checks.append('independent rolling/bucket/TPM refill trace')
    original = rows_for('original18_bucket', 101)
    assert sum(r['canonical_input_units'] + r['canonical_output_units'] for r in original) == 4924
    assert all(request_body(r) == reference.payload(r) for r in original)
    checks.append('original18 bytes/usage4924/arrival/cancellation preserved')

    async def exercise(changes=None, deadline=3):
        row = rows_for('usage_contract', 101)[0]
        row.update(id='check', workflow_id='check', usage_required=False, service_ms=0)
        row.update(changes or {})
        row['deadline_secs'] = deadline
        fixture = Fixture([row], 'self_check')
        await fixture.start()
        origin = time.monotonic()
        attempts = []
        try:
            node = await execute_node(row, fixture.port, 'direct', origin, origin, origin + deadline, 101, attempts)
            await fixture.drain()
        finally:
            await fixture.close()
        workflow = dict(id='check', released_s=origin, deadline_s=origin + deadline, ended_s=node['ended_s'], elapsed_ms=node['elapsed_ms'], outcome=node['outcome'])
        run = dict(arm='direct', origin_s=origin, seed=101, gateway_retries=0, submitted=[row], nodes=[node], workflows=[workflow], client_attempts=attempts,
                   upstream_attempts=fixture.attempts, protocol_errors=fixture.protocol_errors, fixture_pending_tasks=0)
        validate_run(run)
        samples.append(run)
        return run

    for status in (429, 503):
        run = await exercise({'responses': [(status, [('retry-after-ms', '30')])]})
        assert run['nodes'][0]['outcome'] == 'completed' and len(run['client_attempts']) == len(run['upstream_attempts']) == 2
        assert run['nodes'][0]['elapsed_ms'] >= 30
        assert run['client_attempts'][1]['sent_s'] >= run['client_attempts'][0]['retry_at_s']
        checks.append(f'loopback {status} then200 preserves latency/retry lower bound/logical denominator')
    for changes in ({'responses': [(400, [])]}, {'responses': [(429, [('x-should-retry', 'true'), ('x-should-retry', '\tfalse ')])]}):
        run = await exercise(changes)
        assert run['nodes'][0]['outcome'] == 'permanent_failure' and len(run['client_attempts']) == 1
    checks.append('permanent400 and duplicate explicitfalse terminate once')
    run = await exercise({'responses': [(503, [])]})
    assert run['nodes'][0]['outcome'] == 'completed' and run['nodes'][0]['elapsed_ms'] >= 375
    checks.append('absent-header seeded backoff; response headers additive')
    date = formatdate(time.time() + 1.1, usegmt=True)
    run = await exercise({'responses': [(429, [('Retry-After', date), ('retry-after-ms', '20')])]})
    assert run['nodes'][0]['outcome'] == 'completed' and run['client_attempts'][1]['sent_s'] >= run['client_attempts'][0]['retry_at_s']
    checks.append('HTTPdate and duplicate timing maximum lower bound')
    run = await exercise({'responses': [(429, [('Retry-After', '10')])]}, .1)
    assert run['nodes'][0]['outcome'] == 'deadline_exceeded' and len(run['client_attempts']) == 1
    checks.append('hint past original deadline never starts second attempt')
    for flag in ('partial', 'missing_finish', 'missing_done', 'wrong_usage'):
        run = await exercise({flag: True})
        assert run['nodes'][0]['outcome'] == 'permanent_failure' and len(run['client_attempts']) == 1
    missing = await exercise({'omit_usage': True})
    assert missing['nodes'][0]['outcome'] == 'completed' and missing['nodes'][0]['usage']['status'] == 'missing_unrequested'
    missing_required = await exercise({'omit_usage': True, 'usage_required': True})
    assert missing_required['nodes'][0]['outcome'] == 'permanent_failure'
    wrong_required = await exercise({'wrong_usage': True, 'usage_required': True})
    assert wrong_required['nodes'][0]['outcome'] == 'permanent_failure'
    cancelled = await exercise({'cancel_after_ms': 10, 'service_ms': 50})
    assert cancelled['nodes'][0]['outcome'] == 'cancelled' and len(cancelled['client_attempts']) == 1
    checks.append('partial/missing finish/DONE/wrong usage terminal; missing unrequested usage allowed; requested required')
    checks.append('real RST cancellation has one client attempt and bounded provider cleanup')

    # Local-like errors must not masquerade as provider traffic.
    separate_provider = Fixture([], 'self_check')
    await separate_provider.start()
    async def local_reject(reader, writer):
        try:
            head = await reader.readuntil(b'\r\n\r\n')
            length = int(next(line.split(b':', 1)[1] for line in head.split(b'\r\n') if line.lower().startswith(b'content-length:')))
            await reader.readexactly(length)
            data = encode({'error': {'type': 'rate_limit_error'}})
            writer.write(f'HTTP/1.1 429 Fixture\r\nContent-Length: {len(data)}\r\nretry-after-ms: 1\r\n\r\n'.encode() + data)
            await writer.drain()
        finally:
            writer.close()
            await writer.wait_closed()
    server = await asyncio.start_server(local_reject, '127.0.0.1', 0)
    try:
        row = rows_for('usage_contract', 101)[0]
        row['deadline_secs'] = 2
        attempts = []
        start = time.monotonic()
        node = await execute_node(row, server.sockets[0].getsockname()[1], 'direct', start, start, start + 2, 101, attempts)
        assert node['outcome'] == 'retry_budget_exhausted' and len(attempts) == 4 and separate_provider.attempts == []
        local_run = dict(arm='direct', origin_s=start, seed=101, gateway_retries=0, submitted=[row], nodes=[node],
            workflows=[dict(id=row['workflow_id'], released_s=start, deadline_s=start + 2, ended_s=node['ended_s'],
                            elapsed_ms=node['elapsed_ms'], outcome=node['outcome'])],
            client_attempts=attempts, upstream_attempts=separate_provider.attempts, protocol_errors=[], fixture_pending_tasks=0)
        validate_run(local_run)
        samples.append(local_run)
    finally:
        server.close()
        await server.wait_closed()
        await separate_provider.close()
    checks.append('four local429 client attempts; zero separate provider attempts')

    rows = rows_for('chains', 101)
    for row in rows:
        row['service_ms'] = 0
    rows[0]['responses'] = [(400, [])]
    fixture = Fixture(rows, 'chains')
    await fixture.start()
    origin = time.monotonic()
    try:
        nodes, workflows, attempts = await execute_workflows(rows, fixture.port, 'direct', origin, 101)
        await fixture.drain()
    finally:
        await fixture.close()
    chain_run = dict(arm='direct', origin_s=origin, seed=101, gateway_retries=0, submitted=rows, nodes=nodes, workflows=workflows, client_attempts=attempts,
                     upstream_attempts=fixture.attempts, protocol_errors=fixture.protocol_errors, fixture_pending_tasks=0)
    validate_run(chain_run)
    assert collections.Counter(n['outcome'] for n in nodes) == {'completed': 9, 'permanent_failure': 1, 'dependency_failed': 2}
    checks.append('dependency ordering/tool delay/failure descendants and independent workflows')
    samples.append(chain_run)

    valid = samples[0]
    def extended_deadline(run):
        run['workflows'][0]['deadline_s'] += 100
        run['nodes'][0]['deadline_s'] += 100
    def hidden_replay(run):
        first, last = run['client_attempts'][0], run['client_attempts'][-1]
        merged = copy.deepcopy(last)
        merged.update(id=first['id'], index=1, sent_s=first['sent_s'])
        run['client_attempts'] = [merged]
    def late_control_attempt(run):
        run.update(arm='deskquota_managed', profile='cancel_control', cap=1)
        run['submitted'][0].update(cancel_by_control=True, deadline_secs=5)
        run['nodes'][0].update(outcome='cancelled', deadline_s=run['origin_s'] + 5)
        run['workflows'][0].update(outcome='cancelled', deadline_s=run['origin_s'] + 5)
        gate_at = run['client_attempts'][0]['sent_s']
        run['cancellation_control'] = dict(cancel_at_s=gate_at, gate_released_s=gate_at,
            submitted_at_s={run['nodes'][0]['id']: run['nodes'][0]['eligible_s']}, cancelled_provider_attempts=0)
    changes = [
        ('extended original deadline', extended_deadline),
        ('zero node elapsed', lambda r: r['nodes'][0].update(elapsed_ms=0)),
        ('hidden gateway replay', hidden_replay),
        ('late cancelled-provider attempt after gate', late_control_attempt),
        ('missing workflow', lambda r: r['workflows'].clear()),
        ('duplicate outcome', lambda r: r['nodes'].append(copy.deepcopy(r['nodes'][0]))),
        ('unknown upstream', lambda r: r['upstream_attempts'][0].update(ingress_id='unknown')),
        ('pending provider', lambda r: r['upstream_attempts'][0].update(outcome='pending')),
        ('changed upstream semantics', lambda r: r['upstream_attempts'][0]['request_json'].update(model='different')),
        ('changed provider cost', lambda r: r['upstream_attempts'][0].update(actual_cost_fixture_units=99999)),
        ('changed body', lambda r: r['client_attempts'][0].update(request_body_sha256='bad')),
        ('changed output', lambda r: r['client_attempts'][-1]['response'].update(body_utf8='data: [DONE]\n\n')),
        ('changed usage', lambda r: r['nodes'][0]['usage'].update(prompt_tokens=999)),
        ('late success', lambda r: r['nodes'][0].update(ended_s=r['nodes'][0]['deadline_s'] + 1)),
        ('forged retry hint', lambda r: r['client_attempts'][0].update(retry_delay_s=0)),
        ('early retry', lambda r: r['client_attempts'][1].update(sent_s=r['client_attempts'][0]['sent_s'])),
        ('excess attempts', lambda r: r['client_attempts'].extend([copy.deepcopy(r['client_attempts'][0])] * 4)),
        ('unaccounted cancellation', lambda r: r['nodes'][0].update(outcome='cancelled')),
        ('pending fixture', lambda r: r.update(fixture_pending_tasks=1)),
    ]
    for name, mutate in changes:
        corrupt = copy.deepcopy(valid)
        mutate(corrupt)
        try:
            validate_run(corrupt)
        except (ValueError, KeyError):
            checks.append('reject ' + name)
        else:
            raise AssertionError('accepted ' + name)
    result = dict(status='passed', checks=checks, samples=samples, gateway_executed=False, provider_api_calls=0,
                  script_sha256=bench.digest(__file__), shared_client_sha256=bench.digest(ROOT / 'product/scripts/benchmark_http.py'))
    if output:
        output.parent.mkdir(parents=True, exist_ok=True)
        if output.exists():
            raise FileExistsError(output)
        bench.write_json(output, result)
    print(json.dumps({'status': 'passed', 'checks': len(checks), 'gateway_executed': False}))


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--self-check', action='store_true')
    parser.add_argument('--arm', choices=ARMS)
    parser.add_argument('--profile', choices=PROFILES)
    parser.add_argument('--seed', type=int, default=101)
    parser.add_argument('--binary', type=Path, help='Explicit BPE-enabled normal release binary for both DeskQuota configurations')
    parser.add_argument('--output', type=Path, help='New directory for a run; new JSON file for self-check')
    args = parser.parse_args()
    if args.self_check:
        asyncio.run(self_check(args.output))
    else:
        if not args.arm or not args.profile or not args.output:
            parser.error('--arm, --profile and --output are required')
        if args.arm.startswith('deskquota') and (not args.binary or not args.binary.is_file()):
            parser.error('DeskQuota requires --binary pointing to an existing BPE release binary')
        if args.profile == 'cancel_control' and not args.arm.startswith('deskquota'):
            parser.error('cancel_control observes DeskQuota control status only; cap1, no peer performance claim')
        raise SystemExit(asyncio.run(run(args)))
