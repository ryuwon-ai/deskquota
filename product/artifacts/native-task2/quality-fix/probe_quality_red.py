"""Fresh bounded product-quality probes. Only owned config/worker/PTY state."""
from support_red import *

def call(*args, check=True):
 p=subprocess.run([str(BIN),*map(str,args)],capture_output=True,text=True,timeout=18)
 if check and p.returncode:raise AssertionError((args,p.returncode,p.stdout,p.stderr))
 return p

def status(c):return json.loads(call('status','--json','--config',c).stdout)
def tail(r):
 r.choose('Setup: Models');r.choose('Keep all existing models')
 for m in ['Setup: Quota','RPM','TPM','same quota shared','separate input/output','Concurrency','Setup: Run','Local loopback port','Request start at next login','Setup: Tools','Client intents','Setup: Apply']:r.choose(m)
configs=[];results={};clean=[]
try:
 c=fixture('endpoint-comment-preservation');configs.append(c);before=c.read_bytes();r=PTY(c)
 for m in ['Setup: Environment','Environment preset','Setup: Connection','Upstream API base']:r.choose(m)
 r.choose('Actually supported endpoint',b' \r');r.choose('Upstream auth');tail(r);r.choose('Apply');code=r.finish();after=c.read_bytes()
 results['endpoint_edit']={'exit':code,'unrelated_heading_preserved':b'# preserved user heading' in after,'unrelated_port_comment_preserved':b'# port comment' in after,'responses_kept':b'responses' in after,'completions_added':b'chat/completions' in after,'before_sha':digest(before),'after_sha':digest(after),'state':status(c)['state']}
 (OUT/'endpoint-before.toml').write_bytes(before);(OUT/'endpoint-after.toml').write_bytes(after);(OUT/'endpoint.pty.txt').write_bytes(r.buf)
 c=fixture('restart-invalid-ca');configs.append(c);call('on','--config',c);before_status=status(c);r=PTY(c)
 r.choose('Setup: Environment');r.choose('Environment preset',b'\x1b[A\r')
 for m in ['Setup: Connection','Upstream API base','Actually supported endpoint','Upstream auth','Use an explicit upstream proxy']:r.choose(m)
 r.choose('Merge an explicit PEM CA bundle',b'y');r.choose('PEM CA bundle path',str(c.parent/'does-not-exist.pem').encode()+b'\r');tail(r);r.choose('Apply',b'\x1b[A\r');code=r.finish();after_status=status(c)
 results['missing_ca_restart']={'exit':code,'before_state':before_status['state'],'after_state':after_status['state'],'last_worker_state':after_status.get('last_worker_state'),'saved_ca_present':'ca_bundle' in c.read_text(),'restart_disclosed':b'explicitly restarts' in r.buf,'read_ca_error_present':b'failed to read configured CA' in r.buf,'worker_start_failed':b'worker_start_failed' in r.buf}
 (OUT/'missing-ca.pty.txt').write_bytes(r.buf)
 print(json.dumps(results,indent=2))
finally:
 for r in RUNS:r.close()
 for c in configs:
  off=call('off','--config',c,check=False);st=status(c);assert st['state']=='stopped'
  clean.append({'name':c.parent.name,'off_exit':off.returncode,'state':st['state']})
 (OUT/'probe-results.json').write_text(json.dumps(results,indent=2)+'\n')
 shutil.rmtree(SCRATCH)
 (OUT/'cleanup.json').write_text(json.dumps({'workers':clean,'all_pty_waited':all(r.p.poll() is not None for r in RUNS),'scratch_removed':not SCRATCH.exists(),'pty_pids':[r.p.pid for r in RUNS]},indent=2)+'\n')
