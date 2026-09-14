import pathlib,tempfile,subprocess,json
out=pathlib.Path(__file__).resolve().parent
cases=['pi_retired_journal_after_preview_refuses_recreated_value','codex_retired_journal_after_preview_refuses_recreated_value','claude_journal_write_failure_before_restore_preserves_and_retries','claude_resource_failure_records_partial_and_retry_preserves_unowned','normal_owned_reconnect_and_disconnect_controls','dedicated_provider_locks_refuse_without_journal_publication_then_retry','cooperating_retained_apply_and_disconnect_have_serializable_outcome'];results=[]
for case in cases:
 with tempfile.TemporaryDirectory(prefix='llmgw-quality-rereview-') as s:
  env={'PATH':'/usr/bin:/bin','HOME':s,'USERPROFILE':s,'TMPDIR':s,'XDG_CONFIG_HOME':s+'/config','XDG_CACHE_HOME':s+'/cache','XDG_DATA_HOME':s+'/data'}
  cmd=[str(out/'quality_probes'),case,'--exact','--nocapture','--test-threads=1']
  p=subprocess.run(cmd,cwd=s,env=env,capture_output=True,text=True,timeout=30);(out/(case+'.log')).write_text(p.stdout+p.stderr);results.append({'case':case,'command':cmd,'exit_code':p.returncode,'environment_keys':sorted(env),'scratch_contents_after':sorted(x.name for x in pathlib.Path(s).iterdir())});print(case,p.returncode,p.stdout,flush=True)
(out/'rust-probes-execution.json').write_text(json.dumps({'cases':results,'held_rlib_only':True,'cargo_executed':False,'unique_fixture_paths':True,'serial_named_processes':True},indent=2)+'\n')
raise SystemExit(0 if all(x['exit_code']==0 and not x['scratch_contents_after'] for x in results) else 1)
