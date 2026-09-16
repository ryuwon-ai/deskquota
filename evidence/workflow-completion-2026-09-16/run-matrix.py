#!/usr/bin/env python3
"""Sequential task-local measurement runner. Each run gets a fresh instance."""
import hashlib
import json
from pathlib import Path
import subprocess
import sys
import time

BASE = Path(__file__).resolve().parent
ROOT = BASE.parents[1]
BINARY = '/Users/ryuwon/Library/Caches/deskquota-workflow-20260916/llmgw-bpe'
ARMS = ['direct', 'deskquota_local', 'deskquota_managed', 'bifrost', 'litellm']


def run(phase):
    plan = []
    if phase == 'initial':
        plan += [('preflight', 1, a, 'usage_contract') for a in ARMS]
        plan += [('strict', 1, 'direct', 'original18_rolling')]
        plan += [('paired', 1, a, 'original18_bucket') for a in ARMS[1:3]]
    elif phase == 'repeat':
        plan += [('strict', 1, a, 'original18_rolling') for a in
                 ['deskquota_local', 'deskquota_managed', 'bifrost_native_quota', 'litellm_native_quota']]
        for repeat in range(2, 6):
            pair = ARMS[1:3] if repeat % 2 else list(reversed(ARMS[1:3]))
            plan += [('paired', repeat, a, 'original18_bucket') for a in pair]
        peers = ['direct', 'bifrost', 'litellm']
        for repeat in range(1, 4):
            order = peers[repeat - 1:] + peers[:repeat - 1]
            plan += [('peers', repeat, a, 'original18_bucket') for a in order]
    elif phase == 'short':
        for profile in ['not_exhausted', 'recoverable429', 'recoverable503', 'chains']:
            for repeat in range(1, 4):
                order = ARMS[repeat - 1:] + ARMS[:repeat - 1]
                plan += [('short', repeat, a, profile) for a in order]
        plan += [('control', 1, a, 'cancel_control') for a in ARMS[1:3]]
    else:
        raise ValueError(phase)
    hold = json.loads((BASE / 'timing-source-hold.json').read_text())['files']
    def verify():
        for path, digest in hold.items():
            assert hashlib.sha256((ROOT / path).read_bytes()).hexdigest() == digest, path
    records = []
    record_path = BASE / f'matrix-{phase}.json'
    assert not record_path.exists(), record_path
    for group, repeat, arm, profile in plan:
        verify()
        directory = BASE / 'measurements' / group / f'{repeat}-{arm}-{profile}'
        command = [sys.executable, 'scripts/benchmark-workflow-completion.py', '--arm', arm,
                   '--profile', profile, '--binary', BINARY, '--output', str(directory)]
        print(f'START {group}/{repeat} {arm} {profile}', flush=True)
        started = time.monotonic()
        completed = subprocess.run(command, cwd=ROOT, capture_output=True, text=True, timeout=210)
        (directory / 'runner.log').write_text(completed.stdout + completed.stderr)
        result = json.loads((directory / 'result.json').read_text())
        record = dict(group=group, repeat=repeat, arm=arm, profile=profile, command=command,
                      seconds=time.monotonic() - started, exit_code=completed.returncode,
                      result=str((directory / 'result.json').relative_to(ROOT)),
                      status=result['status'], summary=result.get('summary'),
                      error=result.get('error'), audit_error=result.get('audit_error'))
        records.append(record)
        record_path.write_text(json.dumps(records, indent=2) + '\n')
        verify()
        summary = record['summary'] or {}
        print(json.dumps(dict(arm=arm, profile=profile, seconds=record['seconds'],
            status=record['status'], error=record['error'], audit_error=record['audit_error'],
            outcomes=summary.get('workflow_outcomes'),
            p95_ms=summary.get('success_workflow_latency_ms', {}).get('p95'),
            client=summary.get('client_http_attempts'), upstream=summary.get('upstream_attempts'))), flush=True)
        assert completed.returncode == 0 and result.get('audit_valid'), directory


if __name__ == '__main__':
    run(sys.argv[1])
