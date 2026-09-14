from pathlib import Path
import subprocess as sp, tempfile, json, socket, time, os, fcntl, shutil, hashlib
ROOT=Path(__file__).resolve().parents[3]
BIN=ROOT/'product/target/native/release/llmgw'
OUT=Path(__file__).with_name('restart-race-results.json')
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
endpoints = ["models", "chat/completions"]
models = ["fixture"]
'''
results={'binary_sha256':hashlib.sha256(BIN.read_bytes()).hexdigest(),'cases':[]}; owned=[]
def alive(pid):
 try: os.kill(pid,0); return True
 except ProcessLookupError: return False
for label, edit in [('invalid_edit','invalid = ['),('valid_edit',original+'\n# saved during lifecycle wait\n')]:
 tmp=Path(tempfile.mkdtemp(prefix='llmgw-quality-restart-'));config=tmp/'gateway.toml';config.write_text(original)
 def call(*cmd):
  p=sp.run([str(BIN),'--config',str(config),*cmd],capture_output=True,text=True,timeout=20)
  return {'code':p.returncode,'stdout':p.stdout,'stderr':p.stderr}
 def status():
  r=call('status','--json');assert r['code']==0,r;return json.loads(r['stdout'])
 child=None;case={'case':label}
 try:
  assert call('on')['code']==0
  before=status();pid=before['identity']['pid'];owned.append(pid);state=Path(before['state_directory'])
  with (state/'operation.lock').open('r+b') as lock:
   fcntl.flock(lock,fcntl.LOCK_EX)
   child=sp.Popen([str(BIN),'--config',str(config),'restart'],stdout=sp.PIPE,stderr=sp.PIPE,text=True)
   time.sleep(.5)
   assert child.poll() is None,'restart did not wait for held operation lock'
   case['alive_while_gate_held']=alive(pid)
   config.write_text(edit)
   fcntl.flock(lock,fcntl.LOCK_UN)
  stdout,stderr=child.communicate(timeout=20)
  after=status()
  if after.get('identity'):owned.append(after['identity']['pid'])
  case.update({'restart_exit':child.returncode,'stderr':stderr.strip(),'state_after':after['state'],'old_worker_alive':alive(pid),'restart_had_waited_on_operation_lock':True,'full_old_identity_preserved':after.get('identity')==before['identity'],'latest_fingerprint':after.get('identity',{}).get('fingerprint')==hashlib.sha256(edit.encode()).hexdigest(),'pending_restart':after.get('pending_restart')})
  if label=='invalid_edit':
   assert child.returncode==1 and 'invalid configuration' in stderr and case['full_old_identity_preserved'] and after['state']=='running' and after['pending_restart'] is True,case
  else:
   assert child.returncode==0 and after['state']=='running' and not case['full_old_identity_preserved'] and case['latest_fingerprint'] and after['pending_restart'] is False,case
  case['passed']=True
 finally:
  config.write_text(original);case['cleanup_off']=call('off')['code']
  if child is not None and child.poll() is None: child.terminate();child.wait(timeout=3)
  time.sleep(.2)
  case['remaining_owned_workers']=[p for p in owned if alive(p)]
  shutil.rmtree(tmp);case['temp_removed']=not tmp.exists();results['cases'].append(case)
  OUT.write_text(json.dumps(results,indent=2)+'\n')
print(json.dumps(results,indent=2))
