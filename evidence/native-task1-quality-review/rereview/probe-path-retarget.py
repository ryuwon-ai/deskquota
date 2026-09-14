from pathlib import Path
import subprocess as sp,tempfile,json,os,time,shutil,hashlib,fcntl,socket
ROOT=Path(__file__).resolve().parents[3];BIN=ROOT/'product/target/native/release/llmgw';OUT=Path(__file__).with_name('path-retarget-results.json')
upstream=socket.socket();upstream.bind(('127.0.0.1',0));upstream.listen();upstream.setblocking(False)
original=f'''listen = "127.0.0.1:0"
[upstream]
api_base = "http://127.0.0.1:{upstream.getsockname()[1]}/v1"
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
tmp=Path(tempfile.mkdtemp(prefix='llmgw-quality-retarget-'));configs=[tmp/'first.toml',tmp/'second.toml']
for c in configs:c.write_text(original)
results={'binary_sha256':hashlib.sha256(BIN.read_bytes()).hexdigest()};owned=[];child=None
def call(c,*args):
 p=sp.run([str(BIN),'--config',str(c),*args],capture_output=True,text=True,timeout=20)
 return {'exit':p.returncode,'stdout':p.stdout,'stderr':p.stderr.strip()}
def status(c):
 r=call(c,'status','--json');assert r['exit']==0,r;return json.loads(r['stdout'])
def alive(pid):
 try:os.kill(pid,0);return True
 except ProcessLookupError:return False
try:
 before=[]
 for c in configs:
  assert call(c,'on')['exit']==0
  s=status(c);before.append(s);owned.append(s['identity']['pid'])
 state=Path(before[0]['state_directory'])
 with (state/'operation.lock').open('r+b') as lock:
  fcntl.flock(lock,fcntl.LOCK_EX)
  child=sp.Popen([str(BIN),'--config',str(configs[0]),'restart'],stdout=sp.PIPE,stderr=sp.PIPE,text=True)
  time.sleep(.5);assert child.poll() is None
  configs[0].unlink();configs[0].symlink_to(configs[1]);fcntl.flock(lock,fcntl.LOCK_UN)
 stdout,stderr=child.communicate(timeout=20)
 configs[0].unlink();configs[0].write_text(original)
 after=[status(c) for c in configs]
 results.update({'restart_exit':child.returncode,'stderr':stderr.strip(),'both_full_identities_preserved':all(a['identity']==b['identity'] for a,b in zip(after,before)),'both_running':all(a['state']=='running' for a in after)})
 try:
  connection,_=upstream.accept();connection.close();results['upstream_connections']=1
 except BlockingIOError:results['upstream_connections']=0
 assert child.returncode==1 and 'configuration_path_changed' in stderr and results['both_full_identities_preserved'] and results['both_running'] and results['upstream_connections']==0,results
 results['passed']=True
finally:
 if configs[0].is_symlink():configs[0].unlink()
 configs[0].write_text(original)
 results['cleanup_off_exits']=[call(c,'off')['exit'] for c in configs]
 if child is not None and child.poll() is None:child.terminate();child.wait(timeout=3)
 upstream.close();time.sleep(.2)
 results['remaining_owned_workers']=[p for p in owned if alive(p)]
 shutil.rmtree(tmp);results['temp_removed']=not tmp.exists()
 OUT.write_text(json.dumps(results,indent=2)+'\n')
print(json.dumps(results,indent=2))
