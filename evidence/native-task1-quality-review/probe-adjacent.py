from pathlib import Path
import subprocess as sp,tempfile,json,os,time,shutil,hashlib
ROOT=Path(__file__).resolve().parents[2]; BIN=ROOT/'product/target/native/release/llmgw'; OUT=Path(__file__).with_name('adjacent-results.json')
original='''listen = "127.0.0.1:0"
[upstream]
api_base = "http://127.0.0.1:9/v1"
[upstream.auth]
mode = "none"
[quota.rpm]
kind = "unlimited"
[quota.tpm]
kind = "unknown"
[[models]]
id = "fixture"
max_output_tokens = 32
[[roots]]
id = "fixture"
endpoints = ["models"]
models = ["fixture"]
'''
tmp=Path(tempfile.mkdtemp(prefix='llmgw-quality-adjacent-')); config=tmp/'gateway.toml';config.write_text(original)
results={'binary_sha256':hashlib.sha256(BIN.read_bytes()).hexdigest()};pid=None;record=None;saved=None
helper=sp.Popen(['/bin/sleep','45'])
def call(*args):
 t=time.monotonic();p=sp.run([str(BIN),'--config',str(config),*args],capture_output=True,text=True,timeout=20)
 return {'exit':p.returncode,'stdout':p.stdout,'stderr':p.stderr.strip(),'elapsed_s':round(time.monotonic()-t,3)}
def status():
 r=call('status','--json');assert r['exit']==0,r;return json.loads(r['stdout'])
def alive(pid):
 try:os.kill(pid,0);return True
 except ProcessLookupError:return False
try:
 assert call('on')['exit']==0
 old=status();pid=old['identity']['pid'];state=Path(old['state_directory']);record=state/'runtime.json';saved=record.read_bytes()
 config.write_text('invalid = [');r=call('restart');after=status()
 results['invalid_before_restart']={'exit':r['exit'],'stderr':r['stderr'],'same_identity':after['identity']==old['identity']}
 assert r['exit']==1 and after['identity']==old['identity']
 config.write_text(original)
 record.write_bytes(b' '*8193)
 r=call('status','--json');s=call('off')
 record.write_bytes(saved)
 results['oversized_runtime_record']={'status_exit':r['exit'],'off_exit':s['exit'],'elapsed_s':[r['elapsed_s'],s['elapsed_s']],'same_identity_after_restore':status()['identity']==old['identity']}
 assert r['exit']==1 and s['exit']==1 and results['oversized_runtime_record']['same_identity_after_restore']
 assert call('off')['exit']==0
 (state/'data-token').unlink();os.mkfifo(state/'data-token',0o600)
 r=call('on')
 results['nonregular_token']={'exit':r['exit'],'stderr':r['stderr'],'elapsed_s':r['elapsed_s'],'state':status()['state']}
 assert r['exit']==1 and r['elapsed_s']<2 and results['nonregular_token']['state']=='stopped'
 results['unrelated_helper_survived']=helper.poll() is None
finally:
 config.write_text(original)
 if record is not None and saved is not None:record.write_bytes(saved)
 results['cleanup_off']=call('off')['exit']
 if helper.poll() is None:helper.terminate()
 helper.wait(timeout=3);time.sleep(.2)
 results['cleanup']={'remaining_worker':pid if pid is not None and alive(pid) else None,'helper_reaped':helper.poll() is not None}
 shutil.rmtree(tmp);results['cleanup']['temp_removed']=not tmp.exists()
 OUT.write_text(json.dumps(results,indent=2)+'\n')
print(json.dumps(results,indent=2))
