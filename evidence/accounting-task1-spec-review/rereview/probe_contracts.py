"""Independent SPEC probes. Only copied synthetic artifacts / owned temp fixtures."""
import argparse, asyncio, copy, hashlib, json, math, sys, tempfile, tomllib
from pathlib import Path
from unittest.mock import patch
ROOT=Path(__file__).resolve().parents[3]
PRODUCT=ROOT/'product'
sys.path.insert(0,str(PRODUCT/'scripts'))
import benchmark as b
import benchmark_accounting as a
OUT=Path(__file__).resolve().parent
manifest=json.loads((OUT/'smoke.json').read_text())
runs=[json.loads((PRODUCT/entry['path']).resolve().read_text()) for entry in manifest['runs']]
results=[]
def record(name,fn):
 try:
  value=fn(); results.append({'name':name,'accepted':True,'result':value})
 except Exception as exc:
  results.append({'name':name,'accepted':False,'exception':type(exc).__name__,'message':str(exc)})

def validate(run):
 return a.validate_accounting_run(run, expected_arm=run['arm'],expected_seed=1,expected_phase='smoke',expected_rows=b.workload(1,1,True),expected_duration_s=3,expected_windows=0,expected_quota=None,expected_binary=manifest['identity']['production'])
for source in runs:
 record('normal_'+source['arm'],lambda source=source:validate(source))
for label,mutate in [
 ('usage_missing',lambda o:o.update(usage={'status':'missing'})),
 ('usage_changed_integer',lambda o:o['usage'].update(prompt_tokens=o['usage']['prompt_tokens']+1)),
 ('usage_equal_float',lambda o:o['usage'].update(prompt_tokens=float(o['usage']['prompt_tokens']))),
 ('forged_cutoff_flag',lambda o:o.update(ended_s=runs[0]['start_monotonic_s']+3.000001,within_measurement=True)),
]:
 forged=copy.deepcopy(runs[0]); terminal=next(o for o in forged['outcomes'] if o['outcome']=='completed' and o['usage']['status']=='observed'); mutate(terminal)
 record(label,lambda forged=forged:validate(forged))

# Real raw TOML hashes, not arbitrary replacement strings; update the matching parsed snapshot.
for label,old,new in [('api_path','/v1','/other/v1?api-version=fixture'),('model_bound','max_output_tokens = 4096','max_output_tokens = 8192')]:
 forged=copy.deepcopy(runs[0]); c=forged['config']
 text=a._render_gateway_config(c['accounting'],c['listen_port'],c['upstream_port'],c['quota'],2,4).replace(old,new)
 c['raw_toml_sha256']=hashlib.sha256(text.encode()).hexdigest(); c['snapshot']=tomllib.loads(text)
 record('rehashed_'+label,lambda forged=forged:validate(forged))

record('duplicate_planned_seed',lambda:a.paired_summary(runs,[1,1],'smoke'))
record('duplicate_run',lambda:a.paired_summary(runs+[copy.deepcopy(runs[0])],[1],'smoke'))

async def main():
 # Controlled fast quota-shaped rows, no quota runtime. Uses current held smoke numerics.
 def args_for(output):
  return argparse.Namespace(binary=PRODUCT/'target/native/release/llmgw',reference_binary=OUT/'unused-reference',seeds='1',windows=5,output=output,smoke=False,phase='quota',mode='accounting',pilot_only=True)
 async def synthetic_run(arm,seed,windows,binary,reference,phase,directory):
  rows=b.workload(seed,windows)
  rowmap={row['id']:row for row in rows}
  outcomes=[]; attempts=[]
  from benchmark_http import encode
  for row in rows:
   body=b'' if row.get('metadata') else encode({'model':'synthetic','messages':[{'role':'user','content':'x'*row['input_bytes']}],'max_tokens':row['output_reservation'],'stream':True})
   prompt=math.ceil(len(body)*row['actual_ratio']); completion=0 if row.get('metadata') else max(1,math.ceil(row['output_reservation']*row['actual_ratio']))
   cancelled=row.get('cancel_after_ms') is not None
   outcome={'id':row['id'],'outcome':'cancelled' if cancelled else 'completed','ended_s':101+row['offset_s'],'within_measurement':True,'elapsed_ms':1,'scheduling_lag_ms':0,'usage':{'status':'not_observed'} if cancelled else {'status':'not_applicable'} if row.get('metadata') else {'status':'observed','prompt_tokens':prompt,'completion_tokens':completion}}
   if not cancelled:
    outcome['payload_valid']=True
    attempts.append({'id':row['id']+'/1','ingress_id':row['id'],'outcome':'completed','body_bytes':len(body),'actual_cost_fixture_units':prompt+completion})
   outcomes.append(outcome)
  _,config=a.build_gateway_config(arm=arm,listen_port=31001,upstream_port=31002,quota=b.QUOTA,launched_binary=manifest['identity']['production'])
  run={'id':f'{phase}-seed{seed}-{arm}','arm':arm,'phase':phase,'seed':seed,'submitted':rows,'outcomes':outcomes,'attempts':attempts,'config':config,'start_monotonic_s':100,'measurement_duration_s':300,'quota_windows':5,'mock_quota':b.QUOTA,'owned_processes_cleaned':True}
  run['summary']=b.aggregate(run)
  b.write_json(directory/(run['id']+'.json'),run)
  (directory/(run['id']+'.events.jsonl')).write_text('{"event":"validated"}\n')
  return run
 with tempfile.TemporaryDirectory(prefix='accounting-spec-pilot-') as temp:
  output=Path(temp)/'single.json'
  with patch.object(b,'run_arm',synthetic_run): await b.main(args_for(output))
  data=json.loads(output.read_text())
  results.append({'name':'single_seed_pilot','accepted':True,'result':{k:data[k] for k in ('status','completed_pairs','planned_pairs','matrix_complete')}})
  resume_args=args_for(output);resume_args.pilot_only=False
  with patch.object(b,'run_arm',side_effect=AssertionError('verified pair rerun')):
   await b.main(resume_args)
  resumed=json.loads(output.read_text())
  results.append({'name':'single_seed_full_resume','accepted':True,'result':{k:resumed[k] for k in ('status','completed_pairs','planned_pairs','matrix_complete')}})
 # Fully rehashed registered file resume including usage type forgery; no new spawn.
 with tempfile.TemporaryDirectory(prefix='accounting-spec-resume-') as temp:
  directory=Path(temp); copied=copy.deepcopy(manifest)
  copied['runs']=[]
  for source in runs:
   forged=copy.deepcopy(source)
   if source['arm']=='production_rr':
    term=next(o for o in forged['outcomes'] if o['usage']['status']=='observed')
    term['usage']['prompt_tokens']=float(term['usage']['prompt_tokens'])
   path=directory/(forged['id']+'.json'); b.write_json(path,forged)
   entry=copy.deepcopy(next(entry for entry in manifest['runs'] if entry['id']==forged['id']))
   entry['path']=str(path);entry['sha256']=b.digest(path);copied['runs'].append(entry)
  record('resume_rehashed_float_usage',lambda:list(b.resume_preflight(copied,directory,copied['schedule'],'accounting',5)))
asyncio.run(main())
by_name={entry['name']:entry for entry in results}
assert by_name['normal_production_rr']['accepted'] and by_name['normal_production_actual']['accepted']
for name in ('usage_missing','usage_changed_integer','usage_equal_float','forged_cutoff_flag','rehashed_api_path','rehashed_model_bound','duplicate_planned_seed','duplicate_run','resume_rehashed_float_usage'):
 assert not by_name[name]['accepted'], name
assert by_name['single_seed_pilot']['result']['matrix_complete'] is False
assert by_name['single_seed_pilot']['result']['status']=='accounting_pilot_completed'
assert by_name['single_seed_full_resume']['result']['matrix_complete'] is True
assert by_name['single_seed_full_resume']['result']['status']=='completed_accounting_matrix'
(OUT/'probe-contracts.json').write_text(json.dumps(results,indent=2)+'\n')
print(json.dumps([{k:v for k,v in r.items() if k!='result'}|({'result':r['result']} if r['name']=='single_seed_pilot' else {}) for r in results],indent=2))
