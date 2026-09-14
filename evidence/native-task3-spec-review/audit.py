import pathlib,json,hashlib,tarfile,datetime
R=pathlib.Path(__file__).resolve().parents[2]
O=pathlib.Path(__file__).resolve().parent
sha=lambda b:hashlib.sha256(b).hexdigest()
manifest=R/'product/artifacts/native-task3/lock-fix/source-manifest.json'
source=json.loads(manifest.read_text())['files']
protected=json.loads((R/'evidence/native-task2-quality-review/rereview/after.json').read_text())['protected']
extra=[]
for base in ['product/artifacts/native-task3','product/target/native-task3-lock-fix/debug/llmgw','product/target/native-task3-lock-fix/release/llmgw','product/target/native-task3/debug/llmgw','product/target/native-task3/release/llmgw']:
 p=R/base
 for f in ([p] if p.is_file() else sorted(p.rglob('*'))):
  if f.is_file():extra.append({'path':str(f.relative_to(R)),'bytes':f.stat().st_size,'sha256':sha(f.read_bytes())})
def check(records):
 return [x['path'] for x in records if not (R/x['path']).is_file() or sha((R/x['path']).read_bytes())!=x['sha256']]
archive=R/'product/artifacts/native-task3/lock-fix/source-hold.tar.gz'
with tarfile.open(archive) as t:
 members={m.name:t.extractfile(m).read() for m in t.getmembers() if m.isfile()}
 archive_drift=[x['path'] for x in source if sha(members.get(x['path'],b''))!=x['sha256']]
phase=__import__('sys').argv[1]
if phase=='after': extra=json.loads((O/'before.json').read_text())['extra_protected']
out={'at':datetime.datetime.now(datetime.timezone.utc).isoformat(),'source_count':len(source),'source_drift':check(source),'manifest_sha256':sha(manifest.read_bytes()),'archive_sha256':sha(archive.read_bytes()),'archive_members':len(members),'archive_drift':archive_drift,'protected_count':len(protected),'protected_drift':check(protected),'extra_protected':extra,'extra_protected_drift':check(extra)}
(O/(phase+'.json')).write_text(json.dumps(out,indent=2)+'\n')
print({k:v for k,v in out.items() if k!='extra_protected'})
