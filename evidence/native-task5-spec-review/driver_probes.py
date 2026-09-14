import pathlib,types,sys,unittest.mock as mock,subprocess,json,tempfile,os
O=pathlib.Path(__file__).resolve().parent; P=O.parents[1]/'product'
m=types.ModuleType('review_verify_native');m.__file__=str(P/'scripts/verify_native.py');exec(compile(pathlib.Path(m.__file__).read_text(),m.__file__,'exec'),m.__dict__)
records=[]
# No subprocess runs in this file: every relevant dispatch is mocked.
with mock.patch.object(m.subprocess,'run',return_value=subprocess.CompletedProcess([],0,'','')) as runner, mock.patch.dict(os.environ,{'CARGO_TARGET_DIR':str(O/'external-target')},clear=True):
 m.cargo_template_tests(); kw=runner.call_args.kwargs
 records.append({'name':'cargo_target_override_ignored','observed':kw['env']['CARGO_TARGET_DIR'],'requested':str(O/'external-target'),'violation_reproduced':kw['env']['CARGO_TARGET_DIR']==str(P/'target/native-task5')})
class StopBeforeMutation(Exception): pass
for host in ['linux','win32']:
 seen=[]
 def intercepted_apply(*args):
  seen.append({'action':args[-1],'home':str(args[-2])});raise StopBeforeMutation()
 with mock.patch.object(m.sys,'platform',host),mock.patch.object(m,'cargo_template_tests',return_value={'passed':True}),mock.patch.object(m,'status',return_value={'state':'stopped'}),mock.patch.object(m,'apply_autostart',side_effect=intercepted_apply),mock.patch.object(m.tempfile,'tempdir',str(O)),mock.patch.object(m,'free_port',return_value=49155):
  try:m.templates_only(P/'target/native-task5/release/llmgw')
  except StopBeforeMutation:pass
 records.append({'name':'templates_only_dispatches_autostart_apply_'+host,'dispatch':seen,'violation_reproduced':len(seen)==1,'no_real_subprocess_or_registration':True})
with mock.patch.object(m.subprocess,'run',return_value=subprocess.CompletedProcess([],1,'','configured upstream environment credential is missing')) as runner,mock.patch.dict(os.environ,{'REVIEW_SYNTHETIC_AUTH':'fixture-only-not-recorded','SystemRoot':'C:\\Windows'},clear=True):
 m.cli(P/'target/native-task5/release/llmgw',O/'hypothetical.toml',O/'driver-home','on')
 env=runner.call_args.kwargs['env'];records.append({'name':'manual_cli_clears_terminal_auth_and_windows_environment','auth_was_in_parent':True,'auth_in_child':'REVIEW_SYNTHETIC_AUTH' in env,'SystemRoot_in_child':'SystemRoot' in env,'violation_reproduced':'REVIEW_SYNTHETIC_AUTH' not in env})
args=types.SimpleNamespace(stage='manual-cycle',execute=True,confirm_temporary_os_account=True)
stopped={'state':'stopped','autostart':{'registration':'disabled','login_auth_availability':'configured_but_unavailable'}}
with mock.patch.object(m,'status',return_value=stopped),mock.patch.object(m,'cli',return_value=subprocess.CompletedProcess([],1,'','missing synthetic auth')),mock.patch.object(m.pathlib.Path,'home',return_value=O/'driver-home'):
 rec,code=m.manual_stage(args,P/'target/native-task5/release/llmgw',O/'hypothetical.toml')
 records.append({'name':'failed_manual_cycle_returns_success','on_exit':rec['on_exit'],'running':rec['running'],'driver_exit':code,'violation_reproduced':code==0 and rec['on_exit']!=0})
(O/'driver-probes.json').write_text(json.dumps({'actual_user_registration_changed':False,'actual_login_verified':False,'mocked_subprocess_probes':records},indent=2)+'\n');print(json.dumps(records,indent=2));assert all(r['violation_reproduced'] for r in records)
