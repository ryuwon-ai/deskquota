#!/usr/bin/env python3
"""Independent, offline audit: never imports the driver or executes a gateway."""
import argparse
import collections
import copy
import hashlib
import json
import math
from pathlib import Path
import random
import statistics
from email.utils import parsedate_to_datetime

ROOT = Path(__file__).resolve().parents[3]
EVIDENCE = ROOT / 'evidence/workflow-completion-2026-09-16'
TERMINAL = {'completed', 'permanent_failure', 'retry_budget_exhausted',
            'deadline_exceeded', 'cancelled', 'dependency_failed'}


def check(condition, message):
    if not condition:
        raise AssertionError(message)


def near(a, b, message, tolerance=1e-5):
    check(abs(a - b) <= tolerance, f'{message}: {a} != {b}')


def digest(data):
    return hashlib.sha256(data).hexdigest()


def encoded(value):
    return json.dumps(value, separators=(',', ':')).encode()


def expected_body(row):
    body = {'model': 'synthetic', 'messages': [{'role': 'user', 'content': 'x' * row['input_bytes']}],
            'max_tokens': row['output_reservation'], 'stream': True}
    if row.get('predecessor'):
        body['messages'].append({'role': 'assistant', 'content': 'fixture:' + row['predecessor']})
    if row.get('usage_required'):
        body['stream_options'] = {'include_usage': True}
    return body


def original_rows(seed):
    # Reconstruct the frozen original generator, independently of driver imports.
    rng, result = random.Random(seed), []
    for i in range(18):
        size = rng.choice([96, 800, 1900])
        row = dict(id=f'g{i}', root=i % 4, input_bytes=size,
                   output_reservation=rng.choice([64, 256, 768]), actual_ratio=1,
                   service_ms=rng.choice([20, 400, 2500]), offset_s=round(rng.uniform(0, 32), 4),
                   length='long' if size >= 1000 else 'short', cost_case='matching',
                   workload='generation', window=0, workflow_id=f'g{i}', timeout_s=125,
                   deadline_secs=125, predecessor=None)
        row.update(canonical_input_units=math.ceil(len(encoded(expected_body(row))) / 4),
                   canonical_output_units=math.ceil(row['output_reservation'] / 4))
        if i == 7:
            row['cancel_after_ms'] = 100
        result.append(row)
    return result


def distribution(values):
    ordered = sorted(values)
    return {'n': len(values), **{name: ordered[math.ceil(q * len(values)) - 1] if values else None
                              for name, q in [('p50', .5), ('p95', .95), ('p99', .99)]},
            'max': max(values) if values else None}


def response_usage(row, response):
    check(response['status'] == 200, 'semantic success requires HTTP200')
    text, finish, usages, done = [], [], [], 0
    for frame in response['body_utf8'].replace('\r\n', '\n').split('\n\n'):
        if not frame.strip() or frame.startswith(':'):
            continue
        check(frame.startswith('data: ') and done == 0, 'SSE framing or data after DONE')
        if frame[6:] == '[DONE]':
            done += 1
            continue
        event = json.loads(frame[6:])
        check(isinstance(event, dict), 'SSE object')
        if event.get('usage') is not None:
            usages.append(event['usage'])
        for choice in event.get('choices', []):
            check(choice['index'] == 0, 'unexpected choice')
            value = choice.get('delta', {}).get('content')
            if value is not None:
                check(isinstance(value, str), 'nontext output')
                text.append(value)
            if choice.get('finish_reason') is not None:
                finish.append(choice['finish_reason'])
    check(''.join(text) == 'fixture:' + row['id'] and finish == ['stop'] and done == 1,
          'output/finish/DONE semantic mismatch')
    check(response.get('body_eof_s') is not None, 'missing EOF')
    if not usages:
        check(not row.get('usage_required'), 'requested usage absent')
        return {'status': 'missing_unrequested'}
    check(len(usages) == 1, 'duplicate usage')
    usage = usages[0]
    expected = {'prompt_tokens': row['canonical_input_units'], 'completion_tokens': row['canonical_output_units']}
    check(isinstance(usage, dict), 'usage object')
    for key, value in expected.items():
        check(type(usage.get(key)) is int and usage[key] == value, 'usage value mismatch')
    if 'total_tokens' in usage:
        check(type(usage['total_tokens']) is int and usage['total_tokens'] == sum(expected.values()), 'usage total mismatch')
    return {'status': 'observed', **expected}


def retry_bound(attempt, node_id, seed):
    hints, veto = [], False
    for key, value in attempt['response'].get('headers', []):
        key, value = key.lower(), value.strip(' \t')
        veto |= key == 'x-should-retry' and value == 'false'
        if key not in ('retry-after', 'retry-after-ms'):
            continue
        try:
            delay = float(value) / (1000 if key.endswith('-ms') else 1)
        except ValueError:
            try:
                check(key == 'retry-after', 'invalid milliseconds')
                delay = max(0, parsedate_to_datetime(value).timestamp() - attempt['retry_computed_wall_s'])
            except (AssertionError, ValueError, TypeError, OverflowError):
                continue
        if math.isfinite(delay) and delay >= 0:
            hints.append(delay)
    delay = max(hints) if hints else .5 * 2 ** (attempt['index'] - 1) * random.Random(
        f"{seed}/{node_id}/{attempt['index']}").uniform(.75, 1)
    return veto, delay


def audit(run):
    check(run['status'] == 'completed', 'runner failed')
    check(run['gateway_retries'] == 0 and run['client_attempt_limit'] == run['upstream_attempt_limit'] == 4, 'retry policy changed')
    check(run['cache'] == 'off' and run['fresh_fixture_and_gateway_instances'], 'cache/fresh-instance contract')
    rows, nodes, flows = ({r['id']: r for r in run[key]} for key in ('submitted', 'nodes', 'workflows'))
    check(len(rows) == len(run['submitted']) == len(nodes) == len(run['nodes']) and rows.keys() == nodes.keys(), 'node denominator')
    check(len(flows) == len(run['workflows']) and flows.keys() == {r['workflow_id'] for r in rows.values()}, 'workflow denominator')
    if run['profile'].startswith('original18'):
        check(run['submitted'] == original_rows(run['seed']), 'original18 row/body/cost drift')
        check(sum(r['canonical_input_units'] + r['canonical_output_units'] for r in rows.values()) == 4924, 'original total cost')
    clients, providers, mapping = collections.defaultdict(list), collections.defaultdict(list), collections.Counter()
    for key in ('client_attempts', 'upstream_attempts'):
        check(len(run[key]) == len({a['id'] for a in run[key]}), key + ' duplicate IDs')
    for attempt in run['client_attempts']:
        row = rows[attempt['node_id']]
        check(attempt['request_body_sha256'] == digest(encoded(expected_body(row))), 'client body changed')
        clients[row['id']].append(attempt)
    for attempt in run['upstream_attempts']:
        row = rows[attempt['ingress_id']]
        expected = expected_body(row)
        actual = copy.deepcopy(attempt['request_json'])
        if 'max_tokens' not in actual and 'max_completion_tokens' in actual:
            actual['max_tokens'] = actual.pop('max_completion_tokens')
        if not row.get('usage_required') and actual.get('stream_options') == {'include_usage': True}:
            actual.pop('stream_options')
        check(actual == expected and attempt['request_semantic_valid'], 'provider request semantic drift')
        check(attempt['expected_body_sha256'] == digest(encoded(expected)), 'provider expected hash drift')
        if run['arm'].startswith('deskquota'):
            check(attempt['request_body_exact'] and attempt['request_body_sha256'] == attempt['expected_body_sha256'], 'DeskQuota wire drift')
        check(attempt['actual_cost_fixture_units'] == row['canonical_input_units'] + row['canonical_output_units'], 'provider cost drift')
        check(attempt['outcome'] in ('completed', 'rejected', 'disconnected'), 'provider nonterminal')
        matches = [a for a in clients[row['id']] if a['sent_s'] <= attempt['received_s'] <= a.get('response', {}).get('body_eof_s', a['ended_s'])]
        check(len(matches) == 1, 'provider attempt outside a single client exchange')
        mapping[matches[0]['id']] += 1
        check(mapping[matches[0]['id']] == 1, 'hidden gateway/SDK retry')
        providers[row['id']].append(attempt)
        if 'first_write_s' in attempt and not row.get('gated'):
            check(attempt['first_write_s'] - attempt['service_started_s'] + 1e-6 >= row['service_ms'] / 1000, 'fixture service shortened')
    control = run.get('cancellation_control')
    for node_id, node in nodes.items():
        row, flow = rows[node_id], flows[rows[node_id]['workflow_id']]
        first = next(r for r in run['submitted'] if r['workflow_id'] == row['workflow_id'])
        release = control['submitted_at_s'][node_id] if control else run['origin_s'] + first['offset_s']
        deadline = run['origin_s'] + 5 if control else release + first['deadline_secs']
        near(flow['released_s'], release, 'release')
        near(flow['deadline_s'], deadline, 'workflow original deadline')
        near(node['deadline_s'], deadline, 'node original deadline')
        check(node['outcome'] in TERMINAL and max(len(clients[node_id]), len(providers[node_id])) <= 4, 'outcome/attempt ceiling')
        prior = nodes.get(row.get('predecessor'))
        if node['outcome'] == 'dependency_failed':
            check(prior and prior['outcome'] != 'completed' and node.get('not_started') and not clients[node_id] and not providers[node_id], 'invalid dependency failure')
            continue
        eligible = prior['ended_s'] + row['tool_delay_s'] if prior else release
        near(node['eligible_s'], eligible, 'dependency eligibility')
        near(node['elapsed_ms'], 1000 * (node['ended_s'] - eligible), 'node elapsed')
        check(prior is None or prior['outcome'] == 'completed', 'dependency executed without success')
        previous = None
        for i, attempt in enumerate(clients[node_id], 1):
            check(attempt['index'] == i and eligible <= attempt['sent_s'] <= deadline and attempt['ended_s'] >= attempt['sent_s'], 'client clock/index')
            response = attempt.get('response')
            if response:
                check(attempt['sent_s'] <= response['first_http_s'] <= response['body_eof_s'] <= attempt['ended_s'], 'HTTP boundary clock')
            if previous:
                check('retry_at_s' in previous and attempt['sent_s'] + 1e-6 >= previous['retry_at_s'], 'retry before lower bound')
            if 'retry_at_s' in attempt:
                veto, delay = retry_bound(attempt, node_id, run['seed'])
                check(not veto and response['status'] in (408, 429, 500, 502, 503, 504) and not response['output'] and response['terminal_marker_s'] is None, 'unsafe client retry')
                near(delay, attempt['retry_delay_s'], 'retry selected delay')
                near(attempt['retry_at_s'], attempt['retry_computed_monotonic_s'] + delay, 'retry clock')
            previous = attempt
        if node['outcome'] == 'completed':
            check(node['ended_s'] <= deadline and node.get('payload_valid'), 'late or unvalidated completion')
            check(response_usage(row, clients[node_id][-1]['response']) == node['usage'], 'reported semantic/usage result')
            check(sum(a['outcome'] == 'completed' for a in providers[node_id]) == 1, 'completion provider linkage')
        if node['outcome'] == 'cancelled':
            check(row.get('cancel_after_ms') or row.get('cancel_by_control'), 'unaccounted cancellation')
        if row.get('cancel_by_control'):
            check(node['outcome'] == 'cancelled' and not providers[node_id], 'cancelled queued request reached provider')
    latencies, outcomes = [], collections.Counter()
    for flow in flows.values():
        selected = [nodes[r['id']] for r in run['submitted'] if r['workflow_id'] == flow['id']]
        expected = next((n['outcome'] for n in selected if n['outcome'] != 'completed'), 'completed')
        check(flow['outcome'] == expected, 'workflow outcome differs from nodes')
        check(flow['ended_s'] >= max(n['ended_s'] for n in selected), 'workflow precedes node completion')
        elapsed = 1000 * (flow['ended_s'] - flow['released_s'])
        near(flow['elapsed_ms'], elapsed, 'workflow elapsed')
        outcomes[expected] += 1
        if expected == 'completed':
            check(flow['ended_s'] <= flow['deadline_s'], 'late workflow')
            latencies.append(elapsed)
    ledger, tokens, updated, snapshots = [], float(run['quota']['rpm']), None, []
    rpm, tpm = run['quota']['rpm'], run['quota']['tpm']
    bucket = run['quota_contract'] == 'continuous_rpm_bucket_and_rolling_tpm'
    for attempt in sorted(run['upstream_attempts'], key=lambda a: a['received_s']):
        now, cost = attempt['received_s'], attempt['actual_cost_fixture_units']
        if run['profile'].startswith('recoverable') and now < run['origin_s'] + 1:
            check(attempt['status'] == int(run['profile'][-3:]), 'outage response mismatch')
            continue
        ledger = [(t, c) for t, c in ledger if t + 60 > now]
        tokens = min(rpm, tokens + (now - updated) * rpm / 60) if updated is not None else tokens
        updated = now
        used = sum(c for _, c in ledger)
        rd = max(0, (1 - tokens) * 60 / rpm) if bucket else max(0, ledger[0][0] + 60 - now) if len(ledger) >= rpm else 0
        td = 0
        if used + cost > tpm:
            remaining, td = used, 60
            for stamp, debit in ledger:
                remaining -= debit
                if remaining + cost <= tpm:
                    td = max(0, stamp + 60 - now)
                    break
        allowed = max(rd, td) <= 1e-9 and cost <= tpm
        check((attempt['status'] == 200) == allowed, 'quota admission/debit discrepancy')
        snapshots.append(dict(at_s=now, cost=cost, rolling_rpm_used=len(ledger), rolling_tpm_used=used,
                              rpm_tokens=tokens if bucket else None, accepted=allowed, retry_delay_s=max(rd, td)))
        if allowed:
            ledger.append((now, cost))
            if bucket:
                tokens -= 1
    check(len(snapshots) == len(run['budget_samples']), 'quota snapshot denominator')
    for actual, expected in zip(run['budget_samples'], snapshots):
        for key, value in expected.items():
            if isinstance(value, float):
                near(actual[key], value, 'quota snapshot ' + key)
            else:
                check(actual[key] == value, 'quota snapshot ' + key)
    intervals = sorted((stamp, delta) for a in run['upstream_attempts'] if 'service_started_s' in a
                       for stamp, delta in [(a['service_started_s'], 1), (a['ended_s'], -1)])
    active, peak = 0, 0
    for _, delta in intervals:
        active += delta
        peak = max(peak, active)
    check(active == 0 and peak <= run['cap'] and peak == run['peak_provider_service'], 'service concurrency bound')
    check(not run['protocol_errors'] and not run['fixture_pending_tasks'] and run['owned_processes_cleaned'] and not run.get('cleanup_errors'), 'cleanup failure')
    if run['arm'].startswith('deskquota'):
        status = run['status_after']
        check(status['active'] == status['draining'] == status['admission']['queue_length'] == status['admission']['active'] == 0, 'DeskQuota active/queue tail')
    if control:
        check(run['cap'] == 1 and run['arm'].startswith('deskquota') and control['cancelled_provider_attempts'] == 0, 'cancellation scope')
        check(control['cancel_at_s'] < control['gate_released_s'], 'cancel occurs after gate')
        check(any(o['admission']['queue_length'] == 2 for o in control['observations']), 'queued cancellation precondition')
    completed = [f for f in flows.values() if f['outcome'] == 'completed']
    result = dict(planned_workflows=len(flows), planned_nodes=len(nodes), workflow_outcomes=dict(outcomes),
                  node_outcomes=dict(collections.Counter(n['outcome'] for n in nodes.values())),
                  completed_by_seconds={str(t): sum(x <= 1000 * t for x in latencies) for t in (2, 5, 10)},
                  completed_by_deadline=len(latencies), success_workflow_latency_ms=distribution(latencies),
                  all_work_completion_ms=1000 * (max(f['ended_s'] for f in completed) - min(f['released_s'] for f in flows.values())) if len(completed) == len(flows) else None,
                  client_http_attempts=len(run['client_attempts']), upstream_attempts=len(run['upstream_attempts']),
                  provider_rejected_attempts=sum(a['outcome'] == 'rejected' for a in run['upstream_attempts']),
                  admitted_fixture_token_units=sum(a['actual_cost_fixture_units'] for a in run['upstream_attempts'] if a['status'] == 200),
                  completed_usage=dict(collections.Counter(n['usage']['status'] for n in nodes.values() if n['outcome'] == 'completed')))
    groups = {}
    for field in ('root', 'length'):
        for value in {r[field] for r in rows.values()}:
            selected = [nodes[r['id']] for r in rows.values() if r[field] == value]
            groups[f'{field}:{value}'] = {'planned': len(selected),
                'outcomes': dict(collections.Counter(n['outcome'] for n in selected)),
                'success_latency_ms': distribution([1000 * (n['ended_s'] - n['eligible_s']) for n in selected if n['outcome'] == 'completed'])}
    result['groups'] = groups
    for key, value in result.items():
        check(run['summary'][key] == value, 'summary raw recomputation differs: ' + key)
    result['success_workflow_mean_ms'] = statistics.mean(latencies) if latencies else None
    result['successful_node_mean_ms_by_group'] = {}
    for field in ('root', 'length'):
        for value in {r[field] for r in rows.values()}:
            samples = [1000 * (nodes[r['id']]['ended_s'] - nodes[r['id']]['eligible_s'])
                       for r in rows.values() if r[field] == value and nodes[r['id']]['outcome'] == 'completed']
            result['successful_node_mean_ms_by_group'][f'{field}:{value}'] = statistics.mean(samples) if samples else None
    result['attempts_by_node'] = {node_id: {'client': len(clients[node_id]), 'provider': len(providers[node_id])} for node_id in rows}
    result['local_only_client_attempts'] = len(run['client_attempts']) - len(run['upstream_attempts'])
    if run.get('warmup'):
        warmup = run['warmup']
        check(len(warmup['nodes']) == len(warmup['client_attempts']) == len(warmup['upstream_attempts']) == 3, 'warmup denominator')
        check(all(n['outcome'] == 'completed' for n in warmup['nodes']), 'warmup unsuccessful')
        check(not {a['ingress_id'] for a in warmup['upstream_attempts']} & rows.keys(), 'warmup leaked into measurement')
        for node, client, provider in zip(warmup['nodes'], warmup['client_attempts'], warmup['upstream_attempts']):
            row = dict(run['submitted'][0], id=node['id'])
            check(response_usage(row, client['response']) == node['usage'], 'warmup semantic completion')
            check(provider['ingress_id'] == client['node_id'] == node['id'], 'warmup linkage')
        result['separate_warmup'] = {'nodes': 3, 'clients': 3, 'provider': 3}
    return result


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument('--output', type=Path, required=True)
    parser.add_argument('--self-check', action='store_true', help='Corrupt one saved successful run; no HTTP calls')
    args = parser.parse_args()
    if args.self_check:
        path = EVIDENCE / 'measurements/paired/1-deskquota_managed-original18_bucket/result.json'
        original = json.loads(path.read_text())
        audit(original)
        cases = {}
        mutations = {
            'summary_denominator': lambda r: r['summary'].update(planned_nodes=17),
            'node_elapsed': lambda r: r['nodes'][0].update(elapsed_ms=0),
            'extended_deadline': lambda r: r['nodes'][0].update(deadline_s=r['nodes'][0]['deadline_s'] + 100),
            'semantic_output': lambda r: r['client_attempts'][0]['response'].update(body_utf8=r['client_attempts'][0]['response']['body_utf8'].replace('fixture:', 'wrong:')),
            'hidden_replay': lambda r: r['upstream_attempts'].append(dict(r['upstream_attempts'][0], id='hidden/2')),
            'quota_debit': lambda r: r['budget_samples'][1].update(rolling_tpm_used=0),
            'missing_cleanup': lambda r: r.update(fixture_pending_tasks=1),
        }
        for name, mutate in mutations.items():
            changed = copy.deepcopy(original)
            mutate(changed)
            try:
                audit(changed)
            except AssertionError as error:
                cases[name] = str(error)
            else:
                raise AssertionError('auditor accepted ' + name)
        result = {'status': 'passed', 'unmodified_baseline': 'passed', 'rejected_corruptions': cases,
                  'auditor_sha256': digest(Path(__file__).read_bytes()), 'provider_api_calls': 0}
        args.output.write_text(json.dumps(result, indent=2) + '\n')
        print(json.dumps(result, indent=2))
        return 0
    hold = json.loads((EVIDENCE / 'timing-source-hold.json').read_text())['files']
    files, records, errors, configs = {}, [], [], collections.defaultdict(list)
    matrices = sorted(EVIDENCE.glob('matrix-*.json'))
    for matrix in matrices:
        for entry in json.loads(matrix.read_text()):
            files[entry['result']] = entry
    for name, entry in files.items():
        try:
            path = ROOT / name
            run = json.loads(path.read_text())
            check(run['script_sha256'] == run['script_sha256_after'] == hold['scripts/benchmark-workflow-completion.py'], 'driver hash drift')
            check(run['reused_sources'] == run['reused_sources_after'] and all(hold[k] == v for k, v in run['reused_sources'].items()), 'helper hash drift')
            if run.get('gateway'):
                check(run['gateway']['binary_sha256'] == run['gateway']['binary_sha256_after'], 'binary changed midrun')
            result = audit(run)
            records.append({'result': name, 'sha256': digest(path.read_bytes()), 'group': entry['group'],
                            'repeat': entry['repeat'], 'arm': run['arm'], 'profile': run['profile'], 'summary': result})
            if run['arm'].startswith('deskquota'):
                config = copy.deepcopy(run['gateway']['effective_config'])
                config.pop('listen'); config['upstream'].pop('api_base')
                quota = config.pop('quota')
                configs[run['profile']].append((run['arm'], run['gateway']['binary_sha256'], config, quota))
        except (AssertionError, KeyError, ValueError, TypeError, OSError) as error:
            errors.append({'result': name, 'error': str(error)})
    for profile, entries in configs.items():
        try:
            check(len({x[1] for x in entries}) == 1, 'DeskQuota binary differs')
            check(all(x[2] == entries[0][2] for x in entries), 'DeskQuota settings differ beyond quota/ephemeral bindings')
            for arm, _, _, quota in entries:
                known = arm == 'deskquota_local'
                rpm, tpm = (16, 6000) if profile.startswith('original18') else (1000000, 1000000000)
                check(quota == ({'rpm': {'kind': 'known', 'value': rpm}, 'tpm': {'kind': 'known', 'value': tpm}} if known else
                                {'rpm': {'kind': 'unknown'}, 'tpm': {'kind': 'unknown'}}), 'DeskQuota quota config changed')
        except AssertionError as error:
            errors.append({'profile': profile, 'error': str(error)})
    grouped = collections.defaultdict(list)
    for row in records:
        grouped[(row['group'], row['profile'], row['arm'])].append(row['summary'])
    groups = []
    for (group, profile, arm), summaries in sorted(grouped.items()):
        metric = lambda key: [s[key] for s in summaries]
        extent = lambda values: {'median': statistics.median(values), 'min': min(values), 'max': max(values)} if values else None
        p95 = [s['success_workflow_latency_ms']['p95'] for s in summaries if s['success_workflow_latency_ms']['p95'] is not None]
        makespan = [s['all_work_completion_ms'] for s in summaries if s['all_work_completion_ms'] is not None]
        node_metrics = {}
        for name in {name for s in summaries for name in s['groups']}:
            node_metrics[name] = {key: extent([s['groups'][name]['success_latency_ms'][key] for s in summaries
                                              if s['groups'][name]['success_latency_ms'][key] is not None]) for key in ('p50', 'p95', 'p99')}
            node_metrics[name]['mean'] = extent([s['successful_node_mean_ms_by_group'][name] for s in summaries
                                                 if s['successful_node_mean_ms_by_group'][name] is not None])
        groups.append({'group': group, 'profile': profile, 'arm': arm, 'runs': len(summaries),
                       'planned_workflows': sum(metric('planned_workflows')), 'completed_workflows': sum(metric('completed_by_deadline')),
                       'client_attempts': sum(metric('client_http_attempts')), 'provider_attempts': sum(metric('upstream_attempts')),
                       'provider_rejections': sum(metric('provider_rejected_attempts')),
                       'p95_per_run_ms': {'median': statistics.median(p95), 'min': min(p95), 'max': max(p95)} if p95 else None,
                       'successful_workflow_ms_per_run': {
                           **{key: extent([s['success_workflow_latency_ms'][key] for s in summaries
                                          if s['success_workflow_latency_ms'][key] is not None]) for key in ('p50', 'p95', 'p99')},
                           'mean': extent([s['success_workflow_mean_ms'] for s in summaries if s['success_workflow_mean_ms'] is not None])},
                       'successful_node_ms_per_run_by_group': node_metrics,
                       'makespan_per_run_ms': {'median': statistics.median(makespan), 'min': min(makespan), 'max': max(makespan)} if makespan else None,
                       'completed_by_seconds_per_run': [s['completed_by_seconds'] for s in summaries]})
    output = {'status': 'passed' if not errors else 'failed', 'auditor_sha256': digest(Path(__file__).read_bytes()),
              'scope': 'Offline independent raw recomputation; no gateway or driver validator executed. Snapshot of completed matrix entries only.',
              'records': records, 'groups': groups, 'errors': errors,
              'limitations': ['One fixed seed; nearest-rank tails from small within-run samples, not population quantiles.',
                              'not_exhausted is a cap2 burst with high quota, not pure proxy overhead.',
                              'Managed versus local uses existing quota contracts, not a new algorithm.',
                              'Resource data covers gateway PID only, not children; this script does not assert total product resource superiority.',
                              'Synthetic fixture units and outcomes do not measure actual model quality or provider limits.']}
    args.output.parent.mkdir(parents=True, exist_ok=True)
    args.output.write_text(json.dumps(output, indent=2, sort_keys=True) + '\n')
    print(json.dumps({'status': output['status'], 'runs': len(records), 'errors': errors, 'groups': groups}, indent=2))
    return 0 if not errors else 1


if __name__ == '__main__':
    raise SystemExit(main())
