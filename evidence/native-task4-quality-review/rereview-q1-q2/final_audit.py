import pathlib,json,hashlib,tarfile,re
O=pathlib.Path(__file__).resolve().parent;R=O.parents[2];A=R/'product/artifacts/native-task4/quality-fix'
def row(p):
 b=p.read_bytes();return {'path':str(p.relative_to(R)),'bytes':len(b),'sha256':hashlib.sha256(b).hexdigest()}
expected={
'product/artifacts/native-task4/quality-fix/FINAL_HOLD.json':'363fc08a5c3debf18d7ff7c6e68e6abe280e22bce9e4fd02896fd37f614a0fd7',
'product/artifacts/native-task4/quality-fix/README.md':'ef66b0421c4b47e9b08c1381353623f9950ec3d7c110136276c3fd9230d98a66',
'product/artifacts/native-task4/quality-fix/source-manifest-final.json':'69fe932ae90f46abf65305be8fd0197d4c4a0e6bdc68cceb6ff63b38406ed20b',
'product/artifacts/native-task4/quality-fix/source-hold-final.tar.gz':'85d184adc9ad10e859a6988436a843bbd19ce41a250c743413d2466be5a12d3c',
'product/target/native-task4-quality-fix/release/llmgw':'cc15eb4172148a681faba2d4e2932e94a4853fc303de61d234b7a016c05b6a90',
'product/target/native-task4-quality-fix/debug/llmgw':'547e0776af5552c2bb57f4050bba8b152ed26bb776e1082bdbbcf49dea8f6144',
'product/target/native-task4-quality-fix/debug/deps/libllmgw-eba4665eb2ddaf45.rlib':'8651b48f5c771b351a33e266bf6ea9116eb45267879e6ca8137c4f5ae33bd632',
'product/Cargo.lock':'c82e9357a135eb84f0d1be70d0a5f16d2a6c0f7a520cc140562306418a0d3e1c',
'product/rust-toolchain.toml':'a016e82d3b7986387adf4d7d9d88e1581f87bb2a9cfb91ecec4914411b07634b'}
identities=[row(R/p) for p in expected]
for x in identities:assert x['sha256']==expected[x['path']],x['path']
source=json.loads((A/'source-manifest-final.json').read_text())['files']
with tarfile.open(A/'source-hold-final.tar.gz') as t:
 for x in source:
  p=R/x['path'];assert row(p)==x;assert t.extractfile(x['path']).read()==p.read_bytes()
 archive_entries=len(t.getmembers())
before=json.loads((O/'before.json').read_text())['files'];after=[row(R/x['path']) for x in before];drift=[x['path'] for x,y in zip(before,after) if x!=y];assert not drift,drift
(O/'after.json').write_text(json.dumps({'files':after,'drift':drift},indent=2)+'\n')
held_tests={}
for f in ['cargo-test-locked-final.log','client-profiles-release-final.log']:
 rows=re.findall(r'test result: ok\. (\d+) passed; (\d+) failed;',(A/f).read_text());held_tests[f]={'rows':len(rows),'passed':sum(int(x) for x,y in rows),'failed':sum(int(y) for x,y in rows),'reviewer_rerun':False}
clients={}
for name in ['pi','codex','claude']:
 d=json.loads((A/(name+'-installed-wire-final.json')).read_text());assert d['passed'] is True;assert d['binary_sha256']==expected['product/target/native-task4-quality-fix/release/llmgw'];clients[name]={'passed':d['passed'],'binary_sha256':d['binary_sha256'],'reviewer_rerun':False}
result={'source_files_verified':len(source),'archive_entries':archive_entries,'source_archive_drift':[],'changed_source_paths':10,'added_removed_paths':[],'protected_files':len(before),'protected_drift':drift,'held_log_audit':held_tests,'held_installed_client_audit':clients,'identities':identities}
(O/'source-evidence-audit.json').write_text(json.dumps(result,indent=2)+'\n');print(json.dumps({k:v for k,v in result.items() if k not in ['identities','held_installed_client_audit']},indent=2))
