import pathlib,json,hashlib,tarfile,re
R=pathlib.Path(__file__).resolve().parents[2]; O=pathlib.Path(__file__).resolve().parent
sha=lambda p:hashlib.sha256(p.read_bytes()).hexdigest()
checks={
'product/artifacts/native-task4/spec-fix/source-manifest-final2.json':'ffe2eaae543808401e51105cd948f071d7f73170e3c4b86c9253b7221c9a498f',
'product/artifacts/native-task4/spec-fix/source-hold-final2.tar.gz':'a8282024113d4da65b07642df98f1a5b093fcc941689e40c158f13f1763a4e33',
'product/target/native-task4-spec-fix/release/llmgw':'4414957b603bc04d77cb55275d5ca335477645159c2509735ce84b3cd8013801',
'product/target/native-task4-spec-fix/debug/llmgw':'d61bc5de5fc89c8402753d6471aa75e99bfa1582621923e1063aa6987f15bd9f',
'product/target/native-task4-spec-fix/debug/deps/libllmgw-eba4665eb2ddaf45.rlib':'9e658ae3ac6e2dea276bd5d81371389bf1d424f1e2d4fbfbacf65174f215bca0',
'product/Cargo.lock':'c82e9357a135eb84f0d1be70d0a5f16d2a6c0f7a520cc140562306418a0d3e1c',
'product/rust-toolchain.toml':'a016e82d3b7986387adf4d7d9d88e1581f87bb2a9cfb91ecec4914411b07634b',
'product/artifacts/native-task4/spec-fix/README-v2.md':'0e1bcd8995dbb15a3b5fd71ff653d697891b3460b022c2c608ca66a926217a80',
'product/artifacts/native-task4/spec-fix/final-hold-v2.json':'703e0b6b4904890f6f3e3dbe873fce6ee31f9f108442718f397724d55ea67583',
'evidence/native-task4-spec-review/rereview-spec1/README.md':'ff61b3e5619edccb56c812bcd39fe4122d23a806f88ef8e233454190519161ea',
'evidence/native-task4-spec-review/rereview-spec1/FINAL_HOLD.json':'65132189e7c31c6d0853cfe5583f680accc159a5c617b68794f27293371f84eb',
}
identities=[]
for p,want in checks.items():
 b=(R/p).read_bytes();actual=hashlib.sha256(b).hexdigest();assert actual==want,(p,actual);identities.append({'path':p,'bytes':len(b),'sha256':actual})
source=json.loads((R/'product/artifacts/native-task4/spec-fix/source-manifest-final2.json').read_text())['files']
with tarfile.open(R/'product/artifacts/native-task4/spec-fix/source-hold-final2.tar.gz') as t:
 for x in source:
  b=(R/x['path']).read_bytes();assert len(b)==x['bytes'] and hashlib.sha256(b).hexdigest()==x['sha256'];assert t.extractfile(x['path']).read()==b
 archive_entries=len(t.getmembers())
base=json.loads((R/'product/artifacts/native-task3/nonfinite-fix/source-manifest.json').read_text())['files'];base_by={x['path']:x for x in base};source_by={x['path']:x for x in source}
unchanged=[p for p in base_by if p in source_by and base_by[p]['sha256']==source_by[p]['sha256']]
log=(R/'product/artifacts/native-task4/spec-fix/cargo-test-full-final.log').read_text();rows=re.findall(r'test result: ok\. (\d+) passed; (\d+) failed;',log)
before=json.loads((O/'before.json').read_text())['files'];after=[];drift=[]
for x in before:
 p=R/x['path'];b=p.read_bytes();row={'path':x['path'],'bytes':len(b),'sha256':hashlib.sha256(b).hexdigest()};after.append(row)
 if row!=x:drift.append(x['path'])
assert not drift,drift
(O/'after.json').write_text(json.dumps({'files':after,'drift':drift},indent=2)+'\n')
result={'source_files':len(source),'archive_entries':archive_entries,'source_and_archive_drift':[],'protected_files':len(before),'protected_drift':drift,'unchanged_base_paths':unchanged,'added_paths':sorted(source_by.keys()-base_by.keys()),'removed_paths':sorted(base_by.keys()-source_by.keys()),'held_test_log_audit':{'fresh_execution':False,'rows':len(rows),'passed':sum(int(x) for x,y in rows),'failed':sum(int(y) for x,y in rows)},'identities':identities,'product_git_absent':not (R/'product/.git').exists(),'research_git_absent':not (R/'.git').exists()}
(O/'source-evidence-audit.json').write_text(json.dumps(result,indent=2)+'\n');print(json.dumps({k:v for k,v in result.items() if k not in ['unchanged_base_paths','identities']},indent=2))
