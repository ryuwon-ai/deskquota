"""Bounded read-only quality review; all writes under this script's directory."""
import ast
from collections import Counter, defaultdict
from copy import deepcopy
import hashlib
import json
import math
from pathlib import Path
import shutil
import subprocess
import sys

HERE = Path(__file__).resolve().parent
ROOT = HERE.parents[1]
PRODUCT = ROOT / 'product'
def load(p): return json.loads(p.read_text())
def sha(p): return hashlib.sha256(p.read_bytes()).hexdigest()
def save(name, data):
    with (HERE / name).open('x') as f: json.dump(data, f, indent=2, allow_nan=False); f.write('\n')
paths = set(load(ROOT / 'evidence/accounting-task2-spec-review/hashes-before.json'))
paths.update(str(p.relative_to(ROOT)) for p in (PRODUCT / 'artifacts/accounting-ablation/task2').rglob('*') if p.is_file())
paths.update(['scripts/observe-accounting-first-pair-queue.py', 'evidence/accounting-first-pair-queue-observation.json'])
before = {p:sha(ROOT/p) for p in sorted(paths)}
save('hashes-before.json',before)
manifest_path = PRODUCT/'artifacts/accounting-ablation/pilot.json'
manifest=load(manifest_path)
cmd=[sys.executable,str(ROOT/'scripts/audit-accounting-results.py'),'--manifest',str(manifest_path),'--expected-pairs','5','--output',str(HERE/'independent-audit.json')]
proc=subprocess.run(cmd,capture_output=True,text=True)
save('auditor-cli.json',dict(command=cmd,exit_code=proc.returncode,stdout=proc.stdout,stderr=proc.stderr))
assert proc.returncode==0
runs=[]
counts=defaultdict(Counter); groups=defaultdict(Counter); usage=defaultdict(Counter); latency=defaultdict(list); completions=defaultdict(set)
for entry in manifest['runs']:
    p=PRODUCT/entry['path']; assert sha(p)==entry['sha256']; r=load(p); runs.append(r)
    arm=r['arm']; submitted={s['id']:s for s in r['submitted']}; attempts={a['ingress_id']:a for a in r['attempts']}
    assert len(attempts)==len(r['attempts'])
    assert len(submitted)==len(r['outcomes'])==100
    for a in r['attempts']:
        s=submitted[a['ingress_id']]
        payload={'model':'synthetic','messages':[{'role':'user','content':'x'*s['input_bytes']}],'max_tokens':s['output_reservation'],'stream':True}
        size=0 if s.get('metadata') else len(json.dumps(payload,separators=(',',':')).encode())
        cost=0 if s.get('metadata') else math.ceil(size*s['actual_ratio'])+max(1,math.ceil(s['output_reservation']*s['actual_ratio']))
        assert a['body_bytes']==size and a['actual_cost_fixture_units']==cost
    successful=[]
    for o in r['outcomes']:
        s=submitted[o['id']]; fixed=o['ended_s']<=r['start_monotonic_s']+300
        assert fixed is o['within_measurement']; period='fixed' if fixed else 'drain'; kind=o['outcome']
        counts[arm]['submitted']+=1; counts[arm]['final_'+kind]+=1; counts[arm][period+'_'+kind]+=1
        for field in ['root','length']:
            g=groups[(arm,f'{field}:{s[field]}')];g['submitted']+=1;g['final_'+kind]+=1;g[period+'_'+kind]+=1
        usage[arm][o['usage']['status']]+=1
        if kind=='completed':
            assert o['status']==200 and o['payload_valid'] is True and attempts[o['id']]['outcome']=='completed'
            successful.append(o['elapsed_ms'])
            if fixed:completions[(r['seed'],arm)].add(o['id'])
            if s.get('metadata'): assert o['usage']=={'status':'not_applicable'}
            else:
                inp=math.ceil(attempts[o['id']]['body_bytes']*s['actual_ratio']);out=max(1,math.ceil(s['output_reservation']*s['actual_ratio']))
                assert o['usage']=={'status':'observed','prompt_tokens':inp,'completion_tokens':out}
                assert all(type(o['usage'][k]) is int and 0<=o['usage'][k]<2**64 for k in ['prompt_tokens','completion_tokens'])
        else:
            assert o['usage']=={'status':'not_observed'}
            if kind=='rejected':assert o['status']==429 and attempts[o['id']]['outcome']=='rejected'
        if s.get('cancel_after_ms') is not None: assert kind=='cancelled'
    successful.sort(); latency[arm].append({'seed':r['seed'],'n':len(successful),'p50':successful[math.ceil(len(successful)*.5)-1]/1000,'p95':successful[math.ceil(len(successful)*.95)-1]/1000})
report=(ROOT/'reports/accounting-ablation-results.md').read_text()
arm_order=['production_rr','production_actual']
for line in report.splitlines():
    cells=[c.strip() for c in line.strip('|').split('|')]
    if cells[0] not in ['long','short','root 0','root 1','root 2','root 3']:continue
    group='length:'+cells[0] if cells[0] in ['long','short'] else 'root:'+cells[0].split()[1]
    fields=['submitted','fixed_completed','fixed_cancelled','fixed_timeout','drain_completed','drain_rejected','drain_timeout'] if len(cells)==8 else ['final_completed','final_cancelled','final_rejected','final_timeout','final_error']
    assert len(cells)==len(fields)+1
    for field,value in zip(fields,cells[1:]):
        actual=[groups[(arm,group)][field] for arm in arm_order]
        expected=[int(x.strip()) for x in value.split('/')]
        if len(expected)==1:expected*=2
        assert actual==expected,(group,field,actual,expected)
assert [counts[a]['fixed_completed'] for a in arm_order]==[285,290]
assert [counts[a]['final_completed'] for a in arm_order]==[397,410]
save('raw-recalculation.json',{'counts':dict(counts),'groups':{str(k):v for k,v in groups.items()},'usage':dict(usage),'success_only_latency':dict(latency),'subgroup_report_cells_match':True,'pair_delta':[len(completions[(s,arm_order[1])])-len(completions[(s,arm_order[0])]) for s in range(1,6)]})
# Two focused real CLI copies: mutable material only in this reviewer directory.
negative=[]
for name in ['cutoff_flag','numeric_fixture_mismatch']:
    folder=HERE/name;folder.mkdir();dest=folder/'pilot-runs';dest.mkdir();m=deepcopy(manifest)
    for entry in m['runs']:
        original=PRODUCT/entry['path'];target=dest/original.name;shutil.copyfile(original,target);shutil.copyfile(original.with_suffix('.events.jsonl'),target.with_suffix('.events.jsonl'));entry['path']=str(target)
    e=m['runs'][0];p=Path(e['path']);r=load(p);o=next(o for o in r['outcomes'] if o['usage']['status']=='observed')
    if name=='cutoff_flag':o['within_measurement']=not o['within_measurement']
    else:o['usage']['prompt_tokens']+=1
    p.write_text(json.dumps(r));e['sha256']=sha(p);mp=folder/'pilot.json';mp.write_text(json.dumps(m))
    output=folder/'result.json';cmd=[sys.executable,str(ROOT/'scripts/audit-accounting-results.py'),'--manifest',str(mp),'--expected-pairs','5','--output',str(output)]
    proc=subprocess.run(cmd,capture_output=True,text=True);result=load(output)
    negative.append(dict(name=name,command=cmd,exit_code=proc.returncode,stdout=proc.stdout,stderr=proc.stderr,result=result))
    assert proc.returncode==1 and result['passed'] is False
save('negative-cli.json',negative)
after={p:sha(ROOT/p) for p in sorted(paths)};save('hashes-after.json',after)
assert before==after
save('verification.json',{'passed':True,'files_unchanged':len(before),'raw_runs':10,'pairs':5,'ingress':1000,'independent_auditor_exit':0,'negative_cli_rejected':2,'subgroup_report_cells_match':True,'all_failed_usage_not_observed':True,'rejected_client_mock_linkage_correct':True,'execution_finished':True,'status':'HOLD','new_runtime_build_suite_or_product_edit':False,'cleanup_basis':'Existing owner/parent evidence, not remeasured.'})
print(json.dumps(load(HERE/'verification.json')))
