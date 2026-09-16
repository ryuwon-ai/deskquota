import collections,hashlib,json,statistics,sys
from pathlib import Path
root=Path(__file__).resolve().parents[2]
sys.path.insert(0,str(root/'product/scripts'))
import benchmark as b
from benchmark_accounting import normalize_config_snapshot
here=Path(__file__).resolve().parent

def read(name):
 p=here/(name+'.json');r=json.loads(p.read_text());assert r['passed'];b.validate_run(r)
 assert len(r['submitted'])==len(r['outcomes']);assert not r['protocol_errors'];assert r['cleanup']['gateway_exit']==0 and r['cleanup']['pending_mock_tasks']==0
 assert r['status_after']['active']==0 and r['status_after']['admission']['queue_length']==0
 assert int(r['status_after']['admission']['tpm_held'])==0
 assert len(r['attempts'])==r['status_after']['upstream_attempts']
 assert all(c==1 for c in collections.Counter(a['ingress_id'] for a in r['attempts']).values())
 expected={x['id']:x for x in r['submitted']}
 for o in r['outcomes']:
  assert abs(o['elapsed_ms']-(o['ended_s']-o['sent_s'])*1000)<1e-6
  if not o.get('warmup'):assert o['within_measurement']==(o['ended_s']<=r['measurement_start_s']+r['measurement_window_s'])
  if o['outcome']=='completed':
   assert o['body_exact'] and o['usage']==dict(status='observed',prompt_tokens=expected[o['id']]['canonical_input_units'],completion_tokens=expected[o['id']]['canonical_output_units'])
 r['_file']=p.name;r['_hash']=b.digest(p);return r

def outcomes(r):return [o for o in r['outcomes'] if not o.get('warmup')]
def stats(r):
 os=outcomes(r); good=[o for o in os if o['outcome']=='completed']
 return dict(ingress=len(os),completed=len(good),terminal=dict(collections.Counter(o['outcome'] for o in os)),within=sum(o['within_measurement'] for o in good),mean_ms=statistics.mean(o['elapsed_ms'] for o in good) if good else None,p95_ms=b.distribution([o['elapsed_ms'] for o in good])['p95'],attempts=len(r['attempts']),upstream_429=sum(a['outcome']=='rejected' for a in r['attempts']))

profiles=['original18','burst','conversation','mismatch','no_wait_known','no_wait_unknown'];runs=[];pairs=[]
for profile in profiles:
 for rep in range(3):
  a=read(f'r{rep}-{profile}-baseline');c=read(f'r{rep}-{profile}-candidate');runs.extend([a,c])
  assert a['submitted']==c['submitted'] and a['quota']==c['quota']
  ac=normalize_config_snapshot(a['config']);cc=normalize_config_snapshot(c['config']);cc['models'][0].pop('input_estimator');assert ac==cc
  shared={o['id'] for o in outcomes(a) if o['outcome']=='completed'}&{o['id'] for o in outcomes(c) if o['outcome']=='completed'}
  pairs.append(dict(profile=profile,repeat=rep,baseline=stats(a),candidate=stats(c),shared_success=len(shared),shared_p95_ms={name:b.distribution([o['elapsed_ms'] for o in outcomes(r) if o['id'] in shared])['p95'] for name,r in [('baseline',a),('candidate',c)]},files=[a['_file'],c['_file']]))
for rep in range(3):
 for profile in ('no_wait_known','no_wait_unknown'):
  for name in ('new-bytes','o200k'):runs.append(read(f'r{rep}-{profile}-{name}'))
source={r['_file']:r['_hash'] for r in runs}
result=dict(scope='Paired descriptive synthetic results, no provider/competitor/low-end/general throughput proof',pairs=pairs,files=source,binaries={k:json.loads((here/f'{k}.json').read_text()) for k in ['baseline','final-binary']},no_wait={})
for mode in ('legacy','utf8_bytes','cl100k_base','o200k_base'):
 for profile in ('no_wait_known','no_wait_unknown'):
  selected=[r for r in runs if r['encoding']==mode and r['profile']==profile];assert len(selected)==3
  data=[]
  for r in selected:
   byid={x['id']:x for x in r['submitted']}
   data.extend((len(byid[o['id']]['request_json'].encode()),o['elapsed_ms']) for o in outcomes(r) if o['outcome']=='completed')
  assert len(data)==300
  result['no_wait'][mode+'/'+profile]=dict(all_ms=b.distribution([t for n,t in data]),small_ms=b.distribution([t for n,t in data if n<4096]),large_ms=b.distribution([t for n,t in data if n>=4096]),idle_mib=[r['idle_resource']['rss_bytes']/2**20 for r in selected],after_mib=[r['resources_after']['rss_bytes']/2**20 for r in selected],readiness_ms=[r['readiness_ms'] for r in selected])
for p in pairs:print(p['profile'],p['repeat'],p['baseline']['terminal'],'->',p['candidate']['terminal'],'p95',round(p['baseline']['p95_ms'],3),'->',round(p['candidate']['p95_ms'],3),'429',p['baseline']['upstream_429'],'->',p['candidate']['upstream_429'])
(here/'summary.json').write_text(json.dumps(result,indent=2)+'\n')
print('PASS:48 runs,18 primary pairs,all input/output IDs,usage,attempts,config and cleanup validated')
