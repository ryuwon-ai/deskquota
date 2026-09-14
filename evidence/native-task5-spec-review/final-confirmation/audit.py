from pathlib import Path
import json,hashlib,tarfile,difflib,subprocess,sys,datetime
O=Path(__file__).resolve().parent; R=O.parents[2]; P=R/'product'
def ident(p):
 b=p.read_bytes(); return {'path':str(p.relative_to(R)),'bytes':len(b),'sha256':hashlib.sha256(b).hexdigest()}
def write(name,value):
 with (O/name).open('x') as f:json.dump(value,f,indent=2)
MF=P/'artifacts/native-task5/quality-fix-v2/source-manifest-final.json'; manifest=json.loads(MF.read_text()); archive=P/'artifacts/native-task5/quality-fix-v2/source-final.tar.gz'
with tarfile.open(archive) as t:
 assert len(t.getmembers())==93
 for v in manifest['files']:
  assert ident(R/v['path'])==v
  b=t.extractfile(v['path']).read();assert len(b)==v['bytes'] and hashlib.sha256(b).hexdigest()==v['sha256']
 assert t.extractfile('source-manifest.json').read()==MF.read_bytes()
diff=[]; changed=[]
with tarfile.open(P/'artifacts/native-task5/document-boundary-fix/source-hold-final.tar.gz') as t:
 for v in manifest['files']:
  a=t.extractfile(v['path']).read();b=(R/v['path']).read_bytes()
  if a!=b:
   changed.append(v['path']);diff.extend(difflib.unified_diff(a.decode().splitlines(True),b.decode().splitlines(True),fromfile='accepted/'+v['path'],tofile='current/'+v['path']))
assert sorted(changed)==sorted(['product/src/autostart/mod.rs','product/scripts/verify_native.py','product/tests/autostart_templates.rs','product/tests/test_verify_native.py'])
with (O/'delta.patch').open('x') as f:f.writelines(diff)
ids=[ident(MF),ident(archive),ident(P/'artifacts/native-task5/quality-fix-v2/FINAL_HOLD.json'),ident(P/'target/native-task5-quality-fix-v2/release/llmgw'),ident(P/'target/native-task5-quality-fix-v2/debug/deps/libllmgw-1750bcafbc9dfb14.rlib')]
assert [v['sha256'] for v in ids]==['8b281a41f48313a2d664d9f94737cbcba3794b10688904ec1e969d675296e304','12f6c81a44d95879383ffb0883b2b7681dcc203c18c20dcca6cf2621cfe5c875','2eb88ddc76d2317615693f501a3d3954db7f9b05a813314b2fa71817471a0541','88c21502025b0bb0b77db2833845d64bb2c562e345e0c48bc5042651a4ee25a3','0c0bfaf27091f5bc10896015f6d99eaacce21efb11f98ed7c5a51950f409fea7']
write('source-identities.json',{'source_count':92,'archive_members':93,'changed_paths':changed,'identities':ids,'archive_content_drift':[]})
cmd=[sys.executable,'-B',str(O/'contract_probe.py')]
env={'PATH':'/usr/bin:/bin','HOME':str(O/'synthetic-home'),'USERPROFILE':str(O/'synthetic-home'),'XDG_CONFIG_HOME':str(O/'synthetic-home/xdg'),'TMPDIR':str(O),'LC_ALL':'C'}
result=subprocess.run(cmd,cwd=O,env=env,stdout=subprocess.PIPE,stderr=subprocess.STDOUT)
with (O/'contract-probe.log').open('xb') as f:f.write(result.stdout)
write('commands.json',[{'command':cmd,'cwd':str(O),'env':env,'exit_code':result.returncode,'scope':'exact Python source loaded via compile/exec; status/env/apply/subprocess mocked; no product executable invoked','fresh_tests':2,'fresh_cases':2}])
print(result.stdout.decode());assert result.returncode==0
before=json.loads((O/'before.json').read_text()); actual=[ident(R/v['path']) for v in before['files']]; drift=[a for a,b in zip(actual,before['files']) if a!=b];assert not drift
write('after.json',{'at':datetime.datetime.now(datetime.timezone.utc).isoformat(),'files':actual,'drift':drift,'source_count':92,'archive_members':93,'identities':ids})
write('cleanup.json',{'product_workers_started':0,'owned_workers_remaining':0,'fixture_directories_created':0,'synthetic_home_exists':(O/'synthetic-home').exists(),'actual_user_registration_changed':False,'actual_login_verified':False,'current_release_executed':False,'held_target_rebuilt':False,'product_edited':False})
print('Preserved available paths:',len(actual),'drift:',len(drift))
