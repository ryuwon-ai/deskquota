import hashlib,json,pathlib,sys,tarfile,datetime
R=pathlib.Path(__file__).resolve().parents[2]; O=pathlib.Path(__file__).resolve().parent
load=lambda p:json.loads((R/p).read_text())
def ident(p):
 q=R/p;return {'path':p,'bytes':q.stat().st_size,'sha256':hashlib.sha256(q.read_bytes()).hexdigest()}
if sys.argv[1]=='before':
 oldsrc={x['path'] for x in load('product/artifacts/native-task4/quality-fix/source-manifest-final.json')['files']}
 entries=[x for x in load('evidence/native-task4-spec-review/final-confirmation/after.json')['files'] if x['path'] not in oldsrc]
 entries+=load('evidence/native-task4-accepted.json')['preserve_final_spec']
 entries+=load('evidence/native-task4-quality-fix-parent-hold-check.json')['preserve_quality_fix_and_reviews']
 entries+=load('evidence/native-task5-parent-hold-check.json')['preserve_native_task5_artifacts']
 entries+=load('product/artifacts/native-task5/source-manifest-final.json')['files']
 for p in ['evidence/native-task5-parent-hold-check.json','evidence/native-task4-accepted.json','product/target/native-task5/debug/llmgw','product/target/native-task5/release/llmgw','product/target/native-task5/debug/deps/libllmgw-eba4665eb2ddaf45.rlib']:entries.append(ident(p))
 unique={x['path']:x for x in entries}; actual=[ident(p) for p in sorted(unique)];drift=[x['path'] for x in actual if x['sha256']!=unique[x['path']]['sha256'] or x['bytes']!=unique[x['path']]['bytes']]
else:
 before=load('evidence/native-task5-spec-review/before.json')['files'];actual=[ident(x['path']) for x in before];drift=[x['path'] for x,b in zip(actual,before) if x!=b]
m=load('product/artifacts/native-task5/source-manifest-final.json');archive=R/'product/artifacts/native-task5/source-hold-final.tar.gz'
with tarfile.open(archive) as t:
 members=t.getnames(); amap={n:hashlib.sha256(t.extractfile(n).read()).hexdigest() for n in members if t.getmember(n).isfile()}
 archive_drift=[x['path'] for x in m['files'] if amap.get(x['path'])!=x['sha256']]
record={'at':datetime.datetime.now(datetime.timezone.utc).isoformat(),'files':actual,'count':len(actual),'drift':drift,'archive_members':len(members),'archive_drift':archive_drift,'archive':ident(str(archive.relative_to(R))),'unavailable_diagnostic_logs_not_recovered':True}
(O/(sys.argv[1]+'.json')).write_text(json.dumps(record,indent=2)+'\n');print({k:v for k,v in record.items() if k!='files'});assert not drift;assert not archive_drift
