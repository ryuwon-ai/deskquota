import importlib.util,sys,os,tempfile,pathlib,subprocess,json,hashlib,shutil
sys.dont_write_bytecode=True
ROOT=pathlib.Path(__file__).resolve().parents[3]
OUT=pathlib.Path(__file__).resolve().parent
STOP = '--stop' in sys.argv
CASE = 'stop' if STOP else 'ready-control'
BINARY=ROOT/'product/target/native-task4-quality-fix/release/llmgw'
spec=importlib.util.spec_from_file_location('held_pty',ROOT/'product/scripts/probe_native_setup.py');m=importlib.util.module_from_spec(spec);spec.loader.exec_module(m)
scratch=pathlib.Path(tempfile.mkdtemp(prefix='llmgw-quality-ready-'));runs=[];config=scratch/'gateway.toml';home=scratch/'home';home.mkdir(mode=0o700);native=home/'.pi/agent';native.mkdir(parents=True,mode=0o700);bindir=scratch/'bin';bindir.mkdir();pi=bindir/'pi';pi.write_text('#!/bin/sh\n[ "$1" = "--version" ] || exit 99\nprintf "0.84.2\\n"\n');pi.chmod(0o700)
env={'PATH':str(bindir)+':/usr/bin:/bin','HOME':str(home),'USERPROFILE':str(home),'PI_CODING_AGENT_DIR':str(native),'XDG_CONFIG_HOME':str(home/'config'),'XDG_CACHE_HOME':str(home/'cache'),'XDG_DATA_HOME':str(home/'data'),'TMPDIR':str(scratch),'LANG':'en_US.UTF-8','LC_ALL':'en_US.UTF-8','TERM':'xterm-256color','NO_COLOR':'1'}
result={'binary_sha256':hashlib.sha256(BINARY.read_bytes()).hexdigest(),'client_driver':'synthetic version-only executable; no actual installed client','environment_keys':sorted(env),'gateway_cli_executed':True,'upstream_inference_requested':False}
def cli(*args):return subprocess.run([str(BINARY),*args,'--config',str(config)],env=env,cwd=scratch,capture_output=True,text=True,timeout=15)
class ReviewPty(m.PtyRun):
 def finish(self,expected):
  expected=1 if STOP else 0
  result["setup_exit"]=expected
  return super().finish(expected)
 def send(self,value):
  if value==b'y' and b'Apply the exact pi client preview with hash' in self.buffer and not result.get('confirmation_observed'):
   status=json.loads(cli('status','--json').stdout);assert status['state']=='running'
   result['confirmation_observed']=True;result['reviewed_runtime_fingerprint']=status['identity']['fingerprint'];result['config_sha256_before_confirmation']=hashlib.sha256(config.read_bytes()).hexdigest();result['state_before_confirmation']=status['state'];result['client_existed_before_confirmation']=(native/'models.json').exists()
   if STOP:
    off=cli('off');assert off.returncode==0
    stopped=json.loads(cli('status','--json').stdout);assert stopped['state']=='stopped'
    result['stopped_at_confirmation']=True;result['state_after_off_before_yes']=stopped['state']
  super().send(value)
m.PtyRun=ReviewPty
try:
 log=m.complete(BINARY,config,m.free_port(),True,runs,client_config_dir=native,client_environment=env)
 status=json.loads(cli('status','--json').stdout)
 result.update(worker_state_after_apply=status['state'],client_config_written=(native/'models.json').exists(),client_journal_state=status['clients']['pi']['state'],reported_client_applied='client patch: applied' in log)
 result['config_sha256_after_apply']=hashlib.sha256(config.read_bytes()).hexdigest()
 result['runtime_fingerprint_after_apply']=status.get('identity',{}).get('fingerprint')
 result['bug_reproduced']=result.get('stopped_at_confirmation',False) and result['worker_state_after_apply']=='stopped' and result['client_config_written'] and result['client_journal_state']=='applied'
 if STOP:
  assert not result['bug_reproduced'] and result['worker_state_after_apply']=='stopped' and not result['client_config_written'] and result['client_journal_state']=='not_connected' and not result['reported_client_applied']
  assert 'runtime impact changed' in log
  result['refused_changed_runtime_impact']=True
 else: assert result['worker_state_after_apply']=='running' and result['client_config_written'] and result['runtime_fingerprint_after_apply']==result['reviewed_runtime_fingerprint']
 assert result['config_sha256_before_confirmation']==result['config_sha256_after_apply']
 result['correctness_passed']=True
except Exception as e:
 result['driver_error_type']=type(e).__name__;result['driver_error_message']=str(e)[:300];raise
finally:
 for run in runs:run.close()
 if config.exists():
  off=cli('off');status=cli('status','--json');assert off.returncode==0 and json.loads(status.stdout)['state']=='stopped'
 result['cleanup_confirmed_stopped']=True
 shutil.rmtree(scratch);result['owned_temporary_paths_remaining']=[]
 (OUT/('readiness-'+CASE+'-result.json')).write_text(json.dumps(result,indent=2)+'\n')
 print(json.dumps(result,indent=2))
