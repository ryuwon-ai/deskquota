from pathlib import Path
import json, hashlib, sys
r=Path(__file__).resolve().parents[2]
def digest(p):return hashlib.sha256(p.read_bytes()).hexdigest()
records=[]
for name in ['evidence/product-task6-pre-review-source-manifest.json','evidence/product-task6-pre-review-release-source-manifest.json','product/artifacts/task6/source-manifest.json']:
 d=json.loads((r/name).read_text()); checks=[{'path':f['path'],'sha256':digest(r/f['path']),'matches':digest(r/f['path'])==f['sha256']} for f in d['files']]
 records.append({'manifest':name,'files':checks});assert all(f['matches'] for f in checks)
binaries={p:digest(r/p) for p in ['product/target/debug/llmgw','product/target/release/llmgw']}
assert list(binaries.values())==['bc451f4bdfdac9c1f5c0e1de6200b17092ced8d2ab7871f0cf4e787524a89b56','65ca882cf0f299ccc85ba1651500193956dce441951eacdb45e17a231c4c896c']
baseline=json.loads((r/'evidence/product-task5-final-source-manifest.json').read_text())
changed=[f['path'] for f in baseline['files'] if digest(r/f['path'])!=f['sha256']]
out={'passed':True,'checks':records,'binaries':binaries,'changed_from_task5':changed,'added_from_task5':sorted(set(f['path'] for f in d['files'])-set(f['path'] for f in baseline['files']))}
(r/'evidence/task6-quality-review'/sys.argv[1]).write_text(json.dumps(out,indent=2)+'\n');print('All 38 sources match each of 3 manifests; both held binaries match; changed:',changed)
