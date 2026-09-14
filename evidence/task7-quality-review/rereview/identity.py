from pathlib import Path
import json,hashlib,sys
r=Path(__file__).resolve().parents[3]; p=Path(__file__).resolve().parent
m=json.loads((r/'evidence/product-task7-quality-fixed-source-manifest.json').read_text());checks=[]
for f in m['files']:
 h=hashlib.sha256((r/f['path']).read_bytes()).hexdigest();checks.append({'path':f['path'],'sha256':h,'matches':h==f['sha256']})
for profile in ['debug','release']:
 m=json.loads((r/f"evidence/product-task7-quality-fixed{'-release' if profile=='release' else ''}-source-manifest.json").read_text());f=f'product/target/{profile}/llmgw';h=hashlib.sha256((r/f).read_bytes()).hexdigest();checks.append({'path':f,'sha256':h,'matches':h==m['binary_sha256']})
(p/sys.argv[1]).write_text(json.dumps(checks,indent=2)+'\n');assert all(c['matches'] for c in checks)
print('identity verified',len(checks))
