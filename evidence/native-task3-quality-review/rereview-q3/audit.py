import pathlib,json,hashlib,tarfile,datetime,sys
O=pathlib.Path(__file__).resolve().parent;R=O.parents[2];sha=lambda b:hashlib.sha256(b).hexdigest()
source=json.loads((R/'product/artifacts/native-task3/nonfinite-fix/source-manifest.json').read_text())['files'];protected=json.loads((R/'evidence/native-task2-quality-review/rereview/after.json').read_text())['protected'];phase=sys.argv[1]
if phase=='before':
 extra=[]
 for base in ['product/artifacts/native-task3','product/target/native-task3/debug/llmgw','product/target/native-task3/release/llmgw','product/target/native-task3-lock-fix/debug/llmgw','product/target/native-task3-lock-fix/release/llmgw','product/target/native-task3-spec-fix/debug/llmgw','product/target/native-task3-spec-fix/release/llmgw','product/target/native-task3-unowned-fix/debug/llmgw','product/target/native-task3-unowned-fix/release/llmgw','evidence/native-task3-spec-review','evidence/native-task3-quality-review','product/target/native-task3-quality-fix/debug/llmgw','product/target/native-task3-quality-fix/release/llmgw','product/target/native-task3-quality-fix/debug/deps/libllmgw-eba4665eb2ddaf45.rlib','product/target/native-task3-nonfinite-fix/debug/llmgw','product/target/native-task3-nonfinite-fix/release/llmgw','product/target/native-task3-nonfinite-fix/debug/deps/libllmgw-eba4665eb2ddaf45.rlib']:
  p=R/base
  for f in ([p] if p.is_file() else sorted(p.rglob('*'))):
   if f.is_file() and not f.is_relative_to(O):extra.append({'path':str(f.relative_to(R)),'bytes':f.stat().st_size,'sha256':sha(f.read_bytes())})
else:extra=json.loads((O/'before.json').read_text())['extra_protected']
def check(xs):return [x['path'] for x in xs if not (R/x['path']).is_file() or sha((R/x['path']).read_bytes())!=x['sha256']]
a=R/'product/artifacts/native-task3/nonfinite-fix/source-hold.tar.gz'
with tarfile.open(a) as t:members={m.name:t.extractfile(m).read() for m in t.getmembers() if m.isfile()}
out={'at':datetime.datetime.now(datetime.timezone.utc).isoformat(),'source_count':len(source),'source_drift':check(source),'archive_members':len(members),'archive_drift':[x['path'] for x in source if sha(members.get(x['path'],b''))!=x['sha256']],'archive_sha256':sha(a.read_bytes()),'protected_count':len(protected),'protected_drift':check(protected),'extra_protected':extra,'extra_protected_drift':check(extra)}
(O/(phase+'.json')).write_text(json.dumps(out,indent=2)+'\n');print({k:v for k,v in out.items() if k!='extra_protected'})
