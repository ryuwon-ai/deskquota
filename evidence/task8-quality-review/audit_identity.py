import pathlib,json,hashlib,tarfile,difflib,os,sys
root=pathlib.Path(__file__).resolve().parents[2]; out=root/'evidence/task8-quality-review'
hash=lambda p:hashlib.sha256(p.read_bytes()).hexdigest()
man=json.loads((root/'evidence/product-task8-final-review-source-manifest.json').read_text());hold=json.loads((root/'product/artifacts/task8-development/final-review-source-hold.json').read_text())
files={x['path']:hash(root/x['path']) for x in man['files']};assert all(files[x['path']]==x['sha256'] for x in man['files'])
bins={k:hash(pathlib.Path(v['path'])) for k,v in hold['identity'].items() if k!='sources'};assert all(bins[k]==hold['identity'][k]['sha256'] for k in bins)
def archive(name):
 p=root/'evidence'/name
 with tarfile.open(p) as t:return {m.name:t.extractfile(m).read() for m in t.getmembers() if m.isfile() and m.name.startswith("product/")}
base=archive('product-task7-quality-held-source.tar.gz');final=archive('product-task8-final-review-source.tar.gz');measured=archive('product-task8-measured-source.tar.gz')
assert all(hashlib.sha256(v).hexdigest()==files[k] for k,v in final.items())
changed=[k for k in sorted(set(base)|set(final)) if base.get(k)!=final.get(k)]
post=[k for k in sorted(set(measured)|set(final)) if measured.get(k)!=final.get(k)]
if sys.argv[1]=='before':
 (out/'task8-delta.diff').write_text(''.join(''.join(difflib.unified_diff(base.get(k,b'').decode().splitlines(True),final.get(k,b'').decode().splitlines(True),fromfile='task7/'+k,tofile='task8/'+k)) for k in changed))
 (out/'post-measurement.diff').write_text(''.join(''.join(difflib.unified_diff(measured.get(k,b'').decode().splitlines(True),final.get(k,b'').decode().splitlines(True),fromfile='measured/'+k,tofile='final/'+k)) for k in post))
a={'files':files,'binaries':bins,'pilot_sha256':hash(root/'product/artifacts/pilot.json'),'archives':{n:hash(root/'evidence'/n) for n in ['product-task8-final-review-source.tar.gz','product-task8-measured-source.tar.gz']},'task8_changed':changed,'post_measurement_changed':post,'git_absent':not (root/'product/.git').exists()}
(out/(sys.argv[1]+'-identity.json')).write_text(json.dumps(a,indent=2)+'\n');print(json.dumps({k:v for k,v in a.items() if k!='files'},indent=2))
