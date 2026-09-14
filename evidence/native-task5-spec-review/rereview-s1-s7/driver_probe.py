import pathlib,types,sys,unittest.mock as mock,subprocess,json,tempfile,os
O=pathlib.Path(__file__).resolve().parent;P=O.parents[2]/'product';B=P/'target/native-task5-spec-fix-v2/release/llmgw';H=O/'driver-owned-home';H.mkdir()
m=types.ModuleType('review_verify_native');m.__file__=str(P/'scripts/verify_native.py');exec(compile(pathlib.Path(m.__file__).read_text(),m.__file__,'exec'),m.__dict__)
records=[]
def check(name,value):records.append({'name':name,'passed':bool(value)});assert value,name
for host in ['linux','win32']:
 with mock.patch.object(m.sys,'platform',host),mock.patch.object(m,'cargo_template_tests',return_value={'passed':True}),mock.patch.object(m,'cli',side_effect=AssertionError('CLI forbidden in nonMac templates-only')),mock.patch.object(m,'apply_autostart',side_effect=AssertionError('registration forbidden')),mock.patch.object(m,'status',side_effect=AssertionError('manager query forbidden')):
  record=m.templates_only(B,O/'never-built-target');check('nonmac_templates_render_only_'+host,record['passed'] and record['host_file_fixture_executed'] is False and record['actual_user_registration_changed'] is False)
with mock.patch.object(m.subprocess,'run',return_value=subprocess.CompletedProcess([],0,'','')) as runner:
 m.cargo_template_tests(O/'unique-external-target');check('explicit_cargo_target_honored',runner.call_args.kwargs['env']['CARGO_TARGET_DIR']==str(O/'unique-external-target'))
 try:m.cargo_template_tests(m.HELD_TARGET)
 except ValueError:check('original_held_target_refused',True)
 else:check('original_held_target_refused',False)
source={'REVIEW_SYNTHETIC_AUTH':'fixture-not-recorded','SystemRoot':r'C:\Windows'}
terminal=m.terminal_child_env(H,source);empty=m.safe_child_env(H,source);check('terminal_env_preserved_empty_login_separate','REVIEW_SYNTHETIC_AUTH' in terminal and 'SystemRoot' in terminal and 'REVIEW_SYNTHETIC_AUTH' not in empty)
args=types.SimpleNamespace(stage='manual-cycle',execute=True,confirm_temporary_os_account=True)
stopped={'state':'stopped','autostart':{'registration':'disabled','login_auth_availability':'unknown'}}
with mock.patch.object(m,'terminal_child_env',return_value=terminal),mock.patch.object(m.pathlib.Path,'home',return_value=H),mock.patch.object(m,'status',return_value=stopped),mock.patch.object(m,'cli',return_value=subprocess.CompletedProcess([],1,'','synthetic failure')) as cli:
 rec,code=m.manual_stage(args,B,O/'hypothetical.toml');check('manual_failure_nonzero',code==1 and rec['result']=='manual_cycle_failed');check('manual_stage_uses_terminal_env',all(c.kwargs['environment'] is terminal for c in cli.call_args_list))
check('login_needs_human_attestation',not m.login_on_verified({'state':'running','autostart':{'registration':'registered'}},False));summary=m.login_observation_summary({'state':'running','identity':{'nonce':'synthetic-identity'},'autostart':{'registration':'registered'}});check('login_summary_omits_identity','identity' not in summary and 'synthetic-identity' not in json.dumps(summary))
output=O/'existing-output-fixture.json';output.write_text('original fixture')
args=types.SimpleNamespace(binary=B,templates_only=True,cargo_target_dir=O/'unused-target',output=output)
with mock.patch.object(m,'arguments',return_value=args),mock.patch.object(m,'templates_only',return_value={'passed':True}):
 try:m.main()
 except FileExistsError:check('existing_output_never_overwritten',output.read_text()=='original fixture')
 else:check('existing_output_never_overwritten',False)
(O/'driver-probe.json').write_text(json.dumps({'checks':records,'passed':sum(x['passed'] for x in records),'subprocesses_mocked':True,'actual_user_registration_changed':False,'actual_login_verified':False},indent=2)+'\n');print(json.dumps(records,indent=2))
