"""Fresh quality re-review: held release, owned configs/PTYS and loopback only."""
from support import *
import tomllib
configs=[];results={};clean=[];requests=[]
class H(http.server.BaseHTTPRequestHandler):
 def do_GET(self):
  requests.append(('GET',self.path));self.send_response(500);self.end_headers()
 def do_POST(self):
  requests.append(('POST',self.path));self.send_response(500);self.end_headers()
 def log_message(self,*args):pass
httpd=http.server.ThreadingHTTPServer(('127.0.0.1',0),H);thread=threading.Thread(target=httpd.serve_forever);thread.start()
def call(*args,check=True):
 p=subprocess.run([str(BIN),*map(str,args)],capture_output=True,text=True,timeout=18)
 if check and p.returncode:raise AssertionError((args,p.returncode,p.stdout,p.stderr))
 return p
def status(c):return json.loads(call('status','--json','--config',c).stdout)
def f(name):
 c=fixture(name);configs.append(c);c.write_text(c.read_text().replace('127.0.0.1:9/',f'127.0.0.1:{httpd.server_port}/'));return c
def start_through_connection(r,endpoint_edit=False,corporate=False):
 r.choose('Setup: Environment');r.choose('Environment preset',b'\x1b[A\r' if corporate else b'\r')
 for m in ['Setup: Connection','Upstream API base']:r.choose(m)
 r.choose('Actually supported endpoint',b' \r' if endpoint_edit else b'\r');r.choose('Upstream auth')
def tail(r,model_edit=False):
 r.choose('Setup: Models');r.choose('Keep all existing models',b'n' if model_edit else b'\r')
 if model_edit:
  r.choose('Try one bounded GET',b'n');r.choose('Manual model ID',b'model-b\r');r.choose('Set a user-chosen reservation fallback');r.choose('Nonzero reservation fallback',b'128\r')
 for m in ['Setup: Quota','RPM','TPM','same quota shared','separate input/output','Concurrency','Setup: Run','Local loopback port','Request start at next login','Setup: Tools','Client intents','Setup: Apply']:r.choose(m)
def done(name,r):
 code=r.finish();(OUT/(name+'.pty.txt')).write_bytes(r.buf);return code
try:
 c=f('endpoint');before=c.read_bytes();r=PTY(c);start_through_connection(r,endpoint_edit=True);tail(r);r.choose('Apply');code=done('endpoint',r);after=c.read_bytes();v=tomllib.loads(after.decode())
 results['Q1_exact_endpoint']={'exit':code,'heading_kept':b'# preserved user heading' in after,'port_comment_kept':b'# port comment' in after,'endpoints':v['roots'][0]['endpoints']}
 assert code==0 and results['Q1_exact_endpoint']['heading_kept'] and results['Q1_exact_endpoint']['port_comment_kept'] and set(v['roots'][0]['endpoints'])=={'responses','chat/completions'}
 (OUT/'endpoint-before.toml').write_bytes(before);(OUT/'endpoint-after.toml').write_bytes(after)
 c=f('second-root');second='''\n# keep secondary route\n[[roots]]\nid = "second"\nendpoints = [\n # inner endpoints comment\n "models",\n]\nmodels = [\n # inner models comment\n "model-a",\n]\n''';c.write_text(c.read_text()+second);r=PTY(c);start_through_connection(r,endpoint_edit=True);tail(r);r.choose('Apply');code=done('second-root',r)
 results['Q1_unaffected_second_root']={'exit':code,'exact_second_root_kept':c.read_text().endswith(second)};assert code==0 and results['Q1_unaffected_second_root']['exact_second_root_kept']
 for kind in ['aot','inline']:
  c=f('model-'+kind)
  if kind=='inline':
   base=c.read_text().split('[[models]]')[0]
   c.write_text('''# inline heading\nmodels = [{ id = "model-a", max_output_tokens = 64 }] # models container\nroots = [{ id = "pi", endpoints = ["responses"], models = ["model-a"] }] # roots container\n'''+base)
  else:c.write_text(c.read_text().replace('id = "model-a"','id = "model-a" # model identity').replace('id = "pi"','id = "pi" # route identity'))
  before=c.read_bytes();r=PTY(c);start_through_connection(r);tail(r,model_edit=True);r.choose('Apply');code=done('model-'+kind,r);after=c.read_bytes();v=tomllib.loads(after.decode());comments=[line.split('#',1)[1] for line in before.decode().splitlines() if '#' in line]
  results['Q1_model_'+kind]={'exit':code,'all_unrelated_comments_kept':all('#'+text in after.decode() for text in comments),'model_id':v['models'][0]['id'],'cap':v['models'][0]['max_output_tokens'],'root_models':v['roots'][0]['models']};assert code==0 and results['Q1_model_'+kind]['all_unrelated_comments_kept'] and v['models'][0]['id']=='model-b' and v['models'][0]['max_output_tokens']==128 and v['roots'][0]['models']==['model-b']
  (OUT/('model-'+kind+'-after.toml')).write_bytes(after)
 for kind in ['missing','malformed','empty']:
  c=f('ca-'+kind);ca=c.parent/'local-ca.pem'
  if kind!='missing':ca.write_bytes(b'not a PEM certificate\n' if kind=='malformed' else b'')
  call('on','--config',c);before=status(c);r=PTY(c);start_through_connection(r,corporate=True);r.choose('Use an explicit upstream proxy');r.choose('Merge an explicit PEM CA bundle',b'y');r.choose('PEM CA bundle path',str(ca).encode()+b'\r');tail(r);r.choose('Apply',b'\x1b[A\r');code=done('ca-'+kind,r);after=status(c)
  results['Q2_'+kind]={'exit':code,'state':after['state'],'same_identity':before['identity']==after['identity'],'pending_restart':after['pending_restart'],'actionable_ca_error':b'configured CA bundle' in r.buf,'generic_worker_start_failed':b'worker_start_failed' in r.buf,'upstream_requests':len(requests)}
  assert code==1 and after['state']=='running' and before['identity']==after['identity'] and after['pending_restart'] and results['Q2_'+kind]['actionable_ca_error'] and not results['Q2_'+kind]['generic_worker_start_failed'] and not requests
 c=f('valid-ca-then-loss');ca=c.parent/'valid-ca.pem';ca.write_bytes((ROOT/'product/tests/fixtures/setup-localhost-cert.pem').read_bytes());c.write_text(c.read_text().replace('[upstream]','[upstream]\nca_bundle = '+json.dumps(str(ca))));call('on','--config',c);before=status(c);ca.unlink();call('on','--config',c);after=status(c)
 results['matching_on_after_ca_loss']={'same_identity':before['identity']==after['identity'],'state':after['state'],'upstream_requests':len(requests)};assert before['identity']==after['identity'] and after['state']=='running' and not requests
 # F8 saved desired config -> restart, then F6 no-op.
 c=f('f8-f6');call('on','--config',c);before=status(c)
 r=PTY(c);start_through_connection(r,endpoint_edit=True);tail(r);r.choose('Apply');assert done('f8-save-only',r)==0;pending=status(c);assert pending['identity']==before['identity'] and pending['pending_restart']
 r=PTY(c);through(r);r.choose('Apply',b'\x1b[A\r');assert done('f8-save-start',r)==0;restarted=status(c);assert not restarted['pending_restart'] and restarted['identity']!=before['identity']
 exact=c.read_bytes();r=PTY(c);through(r);r.choose('Apply',b'\x1b[A\r');assert done('f6-noop',r)==0;noop=status(c);assert noop['identity']==restarted['identity'] and exact==c.read_bytes()
 results['F8_F6']={'save_only_keeps_identity':True,'pending_rerun_applies_new_identity':True,'noop_keeps_identity_and_bytes':True,'upstream_requests':len(requests)}
 assert not requests
 print(json.dumps(results,indent=2))
finally:
 for r in RUNS:r.close()
 for c in configs:
  p=call('off','--config',c,check=False);st=status(c);assert st['state']=='stopped';clean.append({'fixture':c.parent.name,'off_exit':p.returncode,'state':st['state']})
 httpd.shutdown();httpd.server_close();thread.join(timeout=3);assert not thread.is_alive()
 (OUT/'probe-results.json').write_text(json.dumps(results,indent=2)+'\n')
 shutil.rmtree(SCRATCH)
 (OUT/'cleanup.json').write_text(json.dumps({'fixtures':clean,'pty_count':len(RUNS),'all_pty_waited':all(r.p.poll() is not None for r in RUNS),'upstream_requests':requests,'http_fixture_joined':not thread.is_alive(),'scratch_removed':not SCRATCH.exists()},indent=2)+'\n')
