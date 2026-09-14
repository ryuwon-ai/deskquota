import pathlib,json,hashlib,collections,os,sys,tarfile
root=pathlib.Path(__file__).resolve().parents[2];out=pathlib.Path(__file__).resolve().parent;sys.path.insert(0,str(root/'product/scripts'));sys.dont_write_bytecode=True
import benchmark as b
pilot=json.loads((root/'product/artifacts/pilot.json').read_text()); totals=collections.Counter();quota={};pids=json.loads((root/'product/artifacts/task8-development/completion-audit.json').read_text());original_pids=[pids['owned_harness_pid'],*pids['owned_gateway_pids']]
def absent(pid):
 try:os.kill(pid,0);return False
 except ProcessLookupError:return True
with tarfile.open(root/'evidence/product-task8-measured-source.tar.gz') as t:
 for name,sha in pilot['identity']['sources'].items():assert hashlib.sha256(t.extractfile('product/'+name).read()).hexdigest()==sha
for e in pilot['runs']:
 p=pathlib.Path(e['path']);p=p if p.is_absolute() else root/'product'/p
 assert hashlib.sha256(p.read_bytes()).hexdigest()==e['sha256'];r=json.loads(p.read_text());assert r['id']==e['id'];b.validate_run(r);assert b.aggregate(r)==r['summary']==e['summary']
 ids={x['id'] for x in r['submitted']};outcomeids=[x['id'] for x in r['outcomes']];assert set(outcomeids)==ids and len(outcomeids)==len(ids)
 totals['runs']+=1;totals['measured_ingress']+=len(ids);totals['warmup']+=len(r.get('warmup_outcomes',[]));totals['measured_mock_attempts']+=len(r['attempts'])
 if r['phase']=='quota':
  q=quota.setdefault(r['arm'],collections.Counter());q['submitted']+=len(ids);q['mock_attempts']+=len(r['attempts']);q['gateway_starts']+=r.get('gateway_started_attempts') or 0
  assert r['measurement_duration_s']==300
  for x in r['outcomes']:
   within=x['ended_s']<=r['start_monotonic_s']+300;assert within==x['within_measurement'];q[x['outcome']]+=1
   if x['outcome']=='completed':q['within_300' if within else 'drain_completed']+=1
 else:assert r['phase']=='no_wait' and all(x['outcome']=='completed' for x in r['outcomes'])
result={'audit_passed':True,'scope':'read-only original artifact arithmetic and hashes; no fresh original HTTP execution','totals':dict(totals),'quota':{k:dict(v) for k,v in quota.items()},'original_pids_checked':original_pids,'original_pids_present':[pid for pid in original_pids if not absent(pid)]};assert not result['original_pids_present'];(out/'original-artifact-audit.json').write_text(json.dumps(result,indent=2)+'\n');print(json.dumps(result,indent=2))
