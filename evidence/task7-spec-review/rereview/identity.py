from pathlib import Path
import json,hashlib,sys
r=Path(__file__).resolve().parents[3]
m=json.loads((r/'evidence/product-task7-spec-fixed-source-manifest.json').read_text())
checks=[{'path':f['path'],'sha256':hashlib.sha256((r/f['path']).read_bytes()).hexdigest(),'expected':f['sha256']} for f in m['files']]
for p,h in [('debug','4fb6c03938774b8ca756490ea6c3f624bc5c2bf9e72cdf54c9fb9fb7421609f0'),('release','9be8dcf3cb880b41d6662a41eb91d25c27df07e7be6f23d3fae62c81b374b36d')]:
 q='product/target/'+p+'/llmgw';checks.append({'path':q,'sha256':hashlib.sha256((r/q).read_bytes()).hexdigest(),'expected':h})
v={'source_count':41,'binary_count':2,'checks':checks,'passed':all(c['sha256']==c['expected'] for c in checks),'git_directories_present':[(str(p), (r/p/'.git').exists()) for p in ['.','product']]}
(r/'evidence/task7-spec-review/rereview'/sys.argv[1]).write_text(json.dumps(v,indent=2)+'\n')
print('identity',v['passed'],len(checks));assert v['passed']
