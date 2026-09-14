import pathlib,types,subprocess,json,os,hashlib,plistlib,shutil,datetime
O=pathlib.Path(__file__).resolve().parent; P=O.parents[1]/'product';B=P/'target/native-task5/release/llmgw'
m=types.ModuleType('review_verify_native');m.__file__=str(P/'scripts/verify_native.py');exec(compile(pathlib.Path(m.__file__).read_text(),m.__file__,'exec'),m.__dict__)
H=O/'runtime-owned-home';H.mkdir(); C=H/'설정 & $100% \\"<>'/'gateway.toml';C.parent.mkdir();C.write_text(m.config_text(m.free_port(),'mode = "none"'))
A=H/'auth.toml';A.write_text(m.config_text(m.free_port(),'mode = "env"\nheader = "authorization"\nname = "REVIEW_SYNTHETIC_AUTH"'))
commands=[];checks=[];cleanup={};started=False

def call(config,*args,auth=False):
 env=m.safe_child_env(H);env.update({'XDG_CONFIG_HOME':str(H/'.config'),'XDG_DATA_HOME':str(H/'.local/share'),'XDG_STATE_HOME':str(H/'.local/state'),'XDG_RUNTIME_DIR':str(H/'runtime'),'LOCALAPPDATA':str(H/'AppData/Local')})
 if auth:env['REVIEW_SYNTHETIC_AUTH']='synthetic-only-private'
 cmd=[str(B),'--config',str(config),*args];r=subprocess.run(cmd,cwd=H,env=env,stdin=subprocess.DEVNULL,capture_output=True,text=True,timeout=25)
 commands.append({'argv':cmd,'exit_code':r.returncode,'auth_fixture_present':auth})
 return r

def stat(config):
 r=call(config,'status','--json');assert r.returncode==0;return json.loads(r.stdout)

def apply(config,action):
 r=call(config,'autostart',action);assert r.returncode==2;h=m.preview_hash(r.stdout);r=call(config,'autostart',action,'--apply-hash',h);assert r.returncode==0;return r

def check(name,value):checks.append({'name':name,'passed':bool(value)});assert value,name
try:
 s0=stat(C);check('initial_stopped',s0['state']=='stopped')
 apply(C,'on');s1=stat(C);T=pathlib.Path(s1['autostart']['target']);check('registration_without_runtime',s1['state']=='stopped' and s1['autostart']['registration']=='registered')
 # Read-only host process inventory retained only as a count; never save unrelated process arguments.
 ps=subprocess.run(['/bin/ps','-axo','command='],capture_output=True,text=True,check=True)
 owned=[l for l in ps.stdout.splitlines() if str(C) in l and (' worker ' in l or l.rstrip().endswith(' run'))];check('no_owned_worker_after_registration',len(owned)==0)
 parsed=plistlib.loads(T.read_bytes());check('plist_literal_argv',parsed['ProgramArguments']==[str(B),'--config',str(C),'run']);check('plist_no_restart',parsed['RunAtLoad'] is True and parsed['KeepAlive'] is False)
 lint=subprocess.run(['/usr/bin/plutil','-lint',str(T)],capture_output=True,text=True);commands.append({'argv':['/usr/bin/plutil','-lint',str(T)],'exit_code':lint.returncode});check('plutil_accepts',lint.returncode==0)
 bad=call(C,'autostart','off','--apply-hash','unreviewed-hash');check('incorrect_preview_hash_no_mutation',bad.returncode==2 and T.is_file())
 r=call(C,'on');check('manual_on',r.returncode==0);started=True;s2=stat(C);nonce=s2['identity']['nonce']
 apply(C,'off');s3=stat(C);check('same_authenticated_nonce_after_unregistration',s3['state']=='running' and s3['identity']['nonce']==nonce and not T.exists())
 r=call(C,'off');check('authenticated_manual_off',r.returncode==0);started=False;check('manual_cleanup_stopped',stat(C)['state']=='stopped')
 apply(A,'on');ast=stat(A);AT=pathlib.Path(ast['autostart']['target']);definition=AT.read_text();check('registration_excludes_auth_value_and_name','synthetic-only-private' not in definition and 'REVIEW_SYNTHETIC_AUTH' not in definition)
 ar=call(A,'run');check('empty_login_fixture_missing_auth',ar.returncode!=0 and m.missing_login_auth(ar.stderr));check('empty_env_status_unavailable',ast['autostart']['login_auth_availability']=='configured_but_unavailable')
 shell=call(A,'doctor','--json',auth=True);sdoc=json.loads(shell.stdout);check('synthetic_terminal_auth_present',shell.returncode==0 and 'available; value hidden' in sdoc['auth_availability'])
 # This is observation of misleading unknown classification, separate from positive acceptance checks.
 unobserved={'login_environment_observed':False,'terminal_auth_present':True,'reported_login_availability':sdoc['autostart']['login_auth_availability']}
 apply(A,'off')
finally:
 for config in [C,A]:
  try:
   r=call(config,'off');state=stat(config)['state'];cleanup[config.name]={'authenticated_off_exit':r.returncode,'state':state}
  except Exception as e:cleanup[config.name]={'error_type':type(e).__name__}
 record={'at':datetime.datetime.now(datetime.timezone.utc).isoformat(),'binary':str(B),'binary_sha256':hashlib.sha256(B.read_bytes()).hexdigest(),'actual_user_registration_changed':False,'actual_login_verified':False,'fresh_checks':checks,'checks_passed':sum(x['passed'] for x in checks),'commands':commands,'cleanup':cleanup,'worker_nonce_before_and_after_autostart_off':[nonce,nonce] if 'nonce' in locals() else None,'unobserved_login_environment':locals().get('unobserved')}
 (O/'runtime-probe.json').write_text(json.dumps(record,indent=2)+'\n')
 if all(v.get('state')=='stopped' for v in cleanup.values()):shutil.rmtree(H);record['owned_fixture_removed']=not H.exists();(O/'runtime-probe.json').write_text(json.dumps(record,indent=2)+'\n')
 print(json.dumps({k:v for k,v in record.items() if k not in ['commands','worker_nonce_before_and_after_autostart_off']},indent=2))
