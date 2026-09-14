import collections, hashlib, json, os, sys, time
from pathlib import Path
ROOT=Path(__file__).resolve().parents[2];OUT=Path(__file__).resolve().parent
sys.path.insert(0,str(ROOT/'scripts'));sys.dont_write_bytecode=True
import benchmark as b
before=json.loads((OUT/'before-identities.json').read_text())
assert all(b.digest(Path(p))==sha for p,sha in before.items())
archives={'product-task8-measured-source.tar.gz':'622225adc3c885d4a7a6ed659ca5b9e32604b1318fb5606354dd36b4ebdce075','product-task8-final-review-source.tar.gz':'b2a1813489d6f146b7b550994bfc57331622dca3cf379baf701e2ef71bc60f23'}
assert all(b.digest(ROOT.parent/'evidence'/p)==sha for p,sha in archives.items())
identity=b.identities(ROOT/'target/release/llmgw',ROOT/'target/bench/release/examples/bench_gateway')
previous=json.loads((ROOT/'artifacts/task8-development/final-review-source-hold.json').read_text())['identity']
changed=[p for p in identity['sources'] if identity['sources'][p]!=previous['sources'].get(p)]
assert changed==['scripts/benchmark.py','docs/benchmark-method.md'] or set(changed)=={'scripts/benchmark.py','docs/benchmark-method.md'}
assert set(identity['sources'])==set(previous['sources'])
manifest=json.loads((OUT/'smoke.json').read_text());assert manifest['identity']==identity
outcomes=collections.Counter(); attempt_outcomes=collections.Counter();pids=set();rows=[]
for entry in manifest['runs']:
    path=Path(entry['path']);path=path if path.is_absolute() else ROOT/path
    assert b.digest(path)==entry['sha256']
    run=json.loads(path.read_text());b.validate_run(run)
    assert run['owned_processes_cleaned']
    outcomes.update(r['outcome'] for r in run['outcomes'])
    attempt_outcomes.update(r['terminal'] for r in run['attempts'])
    rows.append({'id':run['id'],'ingress':len(run['submitted']),'attempts':len(run['attempts']),'owned_processes_cleaned':True})
    for line in path.with_suffix('.events.jsonl').read_text().splitlines():
        event=json.loads(line)
        if 'pid' in event:pids.add(event['pid'])
for name in ('startup-cancel-result.json','startup-cancel-green-result.json'):
    pids.update(json.loads((OUT/name).read_text())['owned_gateway_pids'])
for row in json.loads((OUT/'lifecycle-green-result.json').read_text()):pids.update(row['owned_pids'])
pids.add(json.loads((OUT/'startup-exit-green-result.json').read_text())['owned_pid'])
alive=[]
for pid in sorted(pids):
    try:os.kill(pid,0);alive.append(pid)
    except ProcessLookupError:pass
assert not alive,alive
result={'status':'PASS','immutable_original_files_verified':len(before),'original_pilot_sha256':b.digest(ROOT/'artifacts/pilot.json'),'archives_unchanged':archives,'changed_source_paths':changed,'smoke_runs':rows,'smoke_outcomes':dict(outcomes),'smoke_attempt_terminal':dict(attempt_outcomes),'owned_pids_checked':sorted(pids),'owned_pids_still_present':alive,'no_rust_build_or_full_matrix_repeated':True}
(OUT/'final-audit.json').write_text(json.dumps(result,indent=2)+'\n')
hold={'phase':'task8-quality-fix-HOLD','utc':time.strftime('%Y-%m-%dT%H:%M:%SZ',time.gmtime()),'identity':identity,'extra_source_files':{'.gitignore':b.digest(ROOT/'.gitignore')},'original_measured_source_sha256':archives['product-task8-measured-source.tar.gz'],'pre_fix_review_source_sha256':archives['product-task8-final-review-source.tar.gz'],'changed_source_paths':changed}
(OUT/'final-source-hold.json').write_text(json.dumps(hold,indent=2)+'\n')
print(json.dumps(result,indent=2));print('source files',len(identity['sources']),'+ .gitignore')
