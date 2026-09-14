from pathlib import Path
import ast,json,subprocess as sp,tempfile,socket,time,os,shutil
r=Path(__file__).resolve().parents[3]; out=Path(__file__).with_name('adjacent-results.json'); binary=r/'product/target/native/release/llmgw'
tree=ast.parse((r/'evidence/native-task1-spec-review/probe.py').read_text()); original=next(ast.literal_eval(n.value) for n in tree.body if isinstance(n,ast.Assign) and any(isinstance(t,ast.Name) and t.id=='original' for t in n.targets))
tmp=Path(tempfile.mkdtemp(prefix='llmgw-spec-review-adjacent-')); cfg=tmp/'config.toml'; cfg.write_text(original); results={}; pids=[]; address=None; upstream=socket.socket(); upstream.bind(('127.0.0.1',0)); upstream.listen(); upstream.setblocking(False)
original=original.replace('127.0.0.1:9',f'127.0.0.1:{upstream.getsockname()[1]}'); cfg.write_text(original)
def call(*cmd):
 x=sp.run([str(binary),'--config',str(cfg),*cmd],capture_output=True,text=True,timeout=20); return {'code':x.returncode,'stdout':x.stdout,'stderr':x.stderr}
def status():
 x=call('status','--json'); assert x['code']==0,x; return json.loads(x['stdout'])
def check_tokens():return {'data_exists':(state/'data-token').exists(),'control_exists':(state/'control-token').exists()}
state=Path(json.loads(call('doctor','--json')['stdout'])['state_directory'])
try:
 cfg.write_text('invalid = ['); fresh=[]
 for _ in range(2):
  x=call('on'); x.update(check_tokens()); x['state']=status()['state']; fresh.append(x); assert x['code']==1 and not x['data_exists'] and not x['control_exists'] and x['state']=='stopped'
 results['fresh_malformed']=fresh
 cfg.write_text(original); assert call('on')['code']==0; old=status(); pids.append(old['identity']['pid']); address=old['identity']['address']
 cfg.write_text('invalid = ['); on=call('on'); restart=call('restart'); current=status(); results['running_malformed']={'on':on,'restart':restart,'full_identity_unchanged':current['identity']==old['identity'],'pending_restart':current['pending_restart']}; assert on['code']==1 and 'restart_required' in on['stderr']; assert restart['code']==1 and current['identity']==old['identity']
 assert call('off')['code']==0
 for name in ['data-token','control-token']:(state/name).unlink()
 stopped=call('on'); stopped.update(check_tokens()); stopped['state']=status()['state']; results['previously_started_stopped_malformed']=stopped; assert stopped['code']==1 and not stopped['data_exists'] and not stopped['control_exists'] and stopped['state']=='stopped'
 try:
  connection,_=upstream.accept(); connection.close(); received=True
 except BlockingIOError:received=False
 results['upstream_connections_observed']=int(received); assert not received
finally:
 cfg.write_text(original); results['cleanup_off_code']=call('off')['code']; upstream.close(); time.sleep(.1); live=[]
 for pid in pids:
  try:os.kill(pid,0);live.append(pid)
  except ProcessLookupError:pass
 closed=True
 if address:
  host,port=address.split(':'); sock=socket.socket(); closed=sock.connect_ex((host,int(port)))!=0; sock.close()
 shutil.rmtree(tmp); results['cleanup']={'remaining_owned_pids':live,'worker_port_closed':closed,'temp_removed':not tmp.exists()}; out.write_text(json.dumps(results,indent=2)+'\n')
print(json.dumps(results,indent=2))
