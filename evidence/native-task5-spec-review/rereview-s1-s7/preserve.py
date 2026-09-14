import hashlib,json,pathlib,sys,tarfile,datetime
O=pathlib.Path(__file__).resolve().parent;R=O.parents[2];P=R/'product';A=P/'artifacts/native-task5/spec-fix'
load=lambda p:json.loads(p.read_text())
def ident(p):return {'path':str(p.relative_to(R)),'bytes':p.stat().st_size,'sha256':hashlib.sha256(p.read_bytes()).hexdigest()}
if sys.argv[1]=='before':
 baseline=load(A/'preservation-before.json')['files'];source=load(A/'source-manifest-final-v2.json')['files'];entries=baseline+source
 old=load(O.parent/'FINAL_HOLD.json');entries+=old['files'];entries.append(ident(O.parent/'FINAL_HOLD.json'))
 entries += [ident(p) for p in A.rglob('*') if p.is_file()]
 hold=load(A/'FINAL_HOLD-v2.json')
 for k in ['debug_binary','release_binary','default_debug_rlib']:entries.append({**hold[k],'path':'product/'+hold[k]['path']})
 breach=load(A/'preservation-breach-debug-target.json')
 for x in breach['artifacts']:entries.append({'path':'product/'+x['path'],'bytes':x['after_bytes'],'sha256':x['after_sha256']})
 entries.append({k:v for k,v in breach['provisional_release'].items() if k in ['path','bytes','sha256']});entries[-1]['path']='product/'+entries[-1]['path']
 expected={x['path']:x for x in entries};files=[ident(R/p) for p in sorted(expected)];drift=[x['path'] for x in files if any(x[k]!=expected[x['path']][k] for k in ['bytes','sha256'])]
else:
 expected=load(O/'before.json')['files'];files=[ident(R/x['path']) for x in expected];drift=[x['path'] for x,y in zip(files,expected) if x!=y]
with tarfile.open(A/'source-hold-final-v2.tar.gz') as t:
 members=t.getmembers();ah={x.name:hashlib.sha256(t.extractfile(x).read()).hexdigest() for x in members if x.isfile()};ad=[x['path'] for x in load(A/'source-manifest-final-v2.json')['files'] if ah.get(x['path'])!=x['sha256']]
record={'at':datetime.datetime.now(datetime.timezone.utc).isoformat(),'files':files,'count':len(files),'drift':drift,'archive_members':len(members),'archive_drift':ad,'historical_missing_diagnostics':2,'overwritten_provisional_debug_originals':2,'provisional_original_recovery':'unknown; not restored or reconstructed'}
with (O/(sys.argv[1]+'.json')).open('x') as f:json.dump(record,f,indent=2);f.write('\n')
print({k:v for k,v in record.items() if k!='files'});assert not drift and not ad
