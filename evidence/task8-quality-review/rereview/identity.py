import pathlib,json,hashlib,sys,tarfile,difflib,os
root=pathlib.Path(__file__).resolve().parents[3];out=pathlib.Path(__file__).resolve().parent; sha=lambda p:hashlib.sha256(p.read_bytes()).hexdigest()
holdpath=root/'product/artifacts/task8-quality-fix/final-source-hold.json';hold=json.loads(holdpath.read_text());assert sha(holdpath)=='f9e9a2d68e0fa4bdd5266bd932a44d69b74ccc414b03e4c2cb47d2ab511cb1e7'
expected=dict(hold['identity']['sources']);expected.update(hold['extra_source_files']);sources={p:sha(root/'product'/p) for p in expected};assert sources==expected
binaries={k:sha(pathlib.Path(hold['identity'][k]['path'])) for k in ['production','benchmark']};assert all(v==hold['identity'][k]['sha256'] for k,v in binaries.items())
pilotpath=root/'product/artifacts/pilot.json';pilot=json.loads(pilotpath.read_text());raw={}
for entry in pilot['runs']:
 p=pathlib.Path(entry['path']);p=p if p.is_absolute() else root/'product'/p;raw[str(p)]=sha(p);assert raw[str(p)]==entry['sha256']
archives={n:sha(root/'evidence'/n) for n in ['product-task8-measured-source.tar.gz','product-task8-final-review-source.tar.gz']}
with tarfile.open(root/'evidence/product-task8-final-review-source.tar.gz') as t:
 changed=[];diff=[]
 for name in sources:
  prior=t.extractfile('product/'+name).read();new=(root/'product'/name).read_bytes()
  if prior!=new:changed.append(name);diff+=difflib.unified_diff(prior.decode().splitlines(True),new.decode().splitlines(True),fromfile='review/'+name,tofile='fixed/'+name)
assert sorted(changed)==['docs/benchmark-method.md','scripts/benchmark.py']
if sys.argv[1]=='before':(out/'fixed-delta.diff').write_text(''.join(diff))
prior=json.loads((root/'evidence/task8-quality-review/before-identity.json').read_text());assert binaries==prior['binaries'] and sha(pilotpath)==prior['pilot_sha256'] and archives==prior['archives']
info={'hold_sha256':sha(holdpath),'sources':sources,'binaries':binaries,'pilot':sha(pilotpath),'raw':raw,'archives':archives,'changed':sorted(changed)};(out/(sys.argv[1]+'-identity.json')).write_text(json.dumps(info,indent=2)+'\n');print('PASS',sys.argv[1],len(sources),'sources',len(raw),'raw files','two binaries and two archives unchanged')
