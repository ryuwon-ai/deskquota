"""Reproduce review gaps against the current driver; no HTTP or product execution."""
import copy
import importlib.util
import json
from pathlib import Path
import sys

root = Path(__file__).resolve().parents[3]
spec = importlib.util.spec_from_file_location('workflow', root / 'scripts/benchmark-workflow-completion.py')
module = importlib.util.module_from_spec(spec)
spec.loader.exec_module(module)
source = json.loads((Path(__file__).parent / 'self-check-final.json').read_text())['samples'][0]
source.update(origin_s=source['workflows'][0]['released_s'], seed=101, gateway_retries=0)
source['submitted'][0]['deadline_secs'] = source['workflows'][0]['deadline_s'] - source['origin_s']

def corruptions(valid):
    extended = copy.deepcopy(valid)
    extended['workflows'][0]['deadline_s'] += 100
    extended['nodes'][0]['deadline_s'] += 100
    zero_elapsed = copy.deepcopy(valid)
    zero_elapsed['nodes'][0]['elapsed_ms'] = 0
    replay = copy.deepcopy(valid)
    first = replay['client_attempts'][0]
    last = replay['client_attempts'][-1]
    merged = copy.deepcopy(last)
    merged.update(id=first['id'], index=1, sent_s=first['sent_s'])
    replay['client_attempts'] = [merged]
    cancelled = copy.deepcopy(valid)
    cancelled.update(arm='deskquota_managed', profile='cancel_control', cap=1)
    cancelled['submitted'][0].update(cancel_by_control=True, deadline_secs=5)
    cancelled['nodes'][0].update(outcome='cancelled', deadline_s=cancelled['origin_s'] + 5)
    cancelled['workflows'][0].update(outcome='cancelled', deadline_s=cancelled['origin_s'] + 5)
    gate_at = cancelled['client_attempts'][0]['sent_s']
    cancelled['cancellation_control'] = dict(cancel_at_s=gate_at, gate_released_s=gate_at,
        submitted_at_s={cancelled['nodes'][0]['id']: cancelled['nodes'][0]['eligible_s']}, cancelled_provider_attempts=0)
    return [('extended original deadline', extended), ('zero node elapsed', zero_elapsed),
            ('hidden gateway retry in one client exchange', replay), ('cancelled node reaches provider after gate', cancelled)]

module.validate_run(source)
results = []
for name, run in corruptions(source):
    try:
        module.validate_run(run)
    except (ValueError, KeyError) as error:
        results.append(dict(case=name, rejected=True, reason=str(error)))
    else:
        results.append(dict(case=name, rejected=False))
print(json.dumps(dict(driver_sha256=module.bench.digest(module.__file__), cases=results), indent=2))
sys.exit(0 if all(r['rejected'] for r in results) else 1)
