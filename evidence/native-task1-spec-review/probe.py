from pathlib import Path
import subprocess as sp, tempfile, json, socket, time, os, signal, urllib.request, concurrent.futures, shutil
ROOT=Path(__file__).resolve().parents[2]; BIN=ROOT/'product/target/native/release/llmgw'; OUT=Path(__file__).with_name('probe-results.json')
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
results={}; owned=[]; tmp=Path(tempfile.mkdtemp(prefix='llmgw-spec-review-')); config=tmp/'한글 config.toml'; config.write_text(original)
helper=sp.Popen(['/bin/sleep','90']); owned.append(helper.pid)
def call(cmd,**kwargs):
 t=time.monotonic(); p=sp.run([str(BIN),'--config',str(config),*cmd],capture_output=True,text=True,timeout=20,**kwargs); return {'code':p.returncode,'stdout':p.stdout,'stderr':p.stderr,'elapsed':round(time.monotonic()-t,3)}
def status():
 r=call(['status','--json']); assert r['code']==0,r; return json.loads(r['stdout'])
def request(address,path,token,method='GET',nonce=None):
 headers={'x-llmgw-control-token':token} if path.startswith('/_') else {'x-llmgw-token':token}
 if nonce: headers['x-llmgw-instance-nonce']=nonce
 req=urllib.request.Request('http://'+address+path,headers=headers,method=method)
 try:
  with urllib.request.build_opener(urllib.request.ProxyHandler({})).open(req,timeout=2) as r: return r.status,r.read()
 except urllib.error.HTTPError as e:return e.code,e.read()
def port_closed(addr):
 host,port=addr.split(':'); s=socket.socket(); s.settimeout(.3)
 try:return s.connect_ex((host,int(port)))!=0
 finally:s.close()
def alive(pid):
 try:os.kill(pid,0); return True
 except ProcessLookupError:return False
address=None; worker=None
try:
 with concurrent.futures.ThreadPoolExecutor(2) as ex: starts=list(ex.map(lambda _:call(['on']),range(2)))
 assert all(x['code']==0 for x in starts), starts
 s=status(); worker=s['identity']['pid']; owned.append(worker); address=s['identity']['address']; state=Path(s['state_directory']); nonce=s['identity']['nonce']; results['concurrent_on']={'exit_codes':[x['code'] for x in starts],'state':s['state'],'worker_pid':worker}
 config.write_text('invalid = [')
 results['invalid_disk_on']=call(['on']); results['invalid_disk_on']['pending_restart']=status()['pending_restart']; results['invalid_restart']=call(['restart']); results['invalid_restart']['old_worker_unchanged']=status()['identity']['nonce']==nonce
 config.write_text(original+'\n# changed\n'); results['valid_disk_on']=call(['on']); assert 'restart_required' in results['valid_disk_on']['stderr']
 config.write_text(original)
 record=state/'runtime.json'; saved=record.read_bytes(); fields={'path_hash':'0'*64,'fingerprint':'0'*64,'nonce':'0'*64,'address':'127.0.0.1:1','pid':helper.pid,'started_unix_ms':0}; checks=[]
 for key,value in fields.items():
  data=json.loads(saved); data['identity'][key]=value; record.write_text(json.dumps(data)); failed=call(['off']); record.write_bytes(saved); checks.append({'field':key,'code':failed['code'],'error':failed['stderr'].strip(),'worker_survived':status()['identity']['nonce']==nonce,'unrelated_survived':helper.poll() is None})
 results['identity_mismatch']=checks; assert all(c['code']==1 and c['worker_survived'] and c['unrelated_survived'] for c in checks)
 token=(state/'control-token').read_text(); results['conditional_nonce']={'status':request(address,'/_llmgw/stop',token,'POST','previous')[0]}; assert results['conditional_nonce']['status']==409
 results['off']=call(['off']); assert results['off']['code']==0; results['off']['port_closed']=port_closed(address); results['off']['state']=status()['state']; results['off']['helper_survived']=helper.poll() is None
 # Actual stale PID fixture points at our unrelated owned helper; no lock held.
 record.write_text(json.dumps({'pid':helper.pid,'state':'running'})); results['stale_pid']={'off':call(['off'])['code'],'on':call(['on'])['code'],'helper_survived':helper.poll() is None}; s=status(); worker=s['identity']['pid']; owned.append(worker); address=s['identity']['address']
 assert call(['off'])['code']==0
 # Inherited ACL from fixture parent must reject new state before token bytes.
 for p in state.iterdir(): p.unlink()
 state.rmdir()
 chmod=sp.run(['/bin/chmod','+a','everyone allow read,write,file_inherit,directory_inherit',str(tmp)],capture_output=True,text=True); assert chmod.returncode==0
 result=call(['on']); results['inherited_acl']={'exit':result['code'],'error':result['stderr'].strip(),'data_token_exists':(state/'data-token').exists(),'control_token_exists':(state/'control-token').exists()}; assert result['code']==1 and not (state/'data-token').exists()
 sp.run(['/bin/chmod','-N',str(tmp)],check=True)
finally:
 config.write_text(original)
 # Only our temp state; inherited ACL case has no worker.
 results['cleanup_off']=call(['off'])['code']
 if helper.poll() is None: helper.terminate()
 helper.wait(timeout=3)
 time.sleep(.15)
 results['cleanup']={'owned_pids':owned,'remaining_pids':[p for p in owned if alive(p)],'last_worker_port_closed':None if address is None else port_closed(address)}
 shutil.rmtree(tmp); results['cleanup']['temp_removed']=not tmp.exists()
 OUT.write_text(json.dumps(results,indent=2)+'\n')
print(json.dumps(results,indent=2))
