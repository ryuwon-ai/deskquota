import argparse, base64, hashlib, json, os, subprocess, sys
from pathlib import Path


def run(argv, env, cwd, timeout=35):
    return subprocess.run([str(x) for x in argv],env=env,cwd=cwd,capture_output=True,timeout=timeout)


def main():
    a=argparse.ArgumentParser()
    a.add_argument('--client',choices=['claude','codex'],required=True)
    a.add_argument('--binary',type=Path,required=True)
    a.add_argument('--executable',type=Path,required=True)
    a.add_argument('--product',type=Path,required=True)
    a.add_argument('--work',type=Path,required=True)
    a.add_argument('--managed',action='store_true')
    q=a.parse_args()
    sys.path.insert(0,str(q.product/'scripts'))
    import verify_clients as fixtures
    work=q.work.resolve()
    work.mkdir(parents=True,exist_ok=False)
    # Protect this newly created synthetic tree before creating any client data.
    ps="$p='"+str(work).replace("'","''")+"';$sid=[Security.Principal.WindowsIdentity]::GetCurrent().User;$acl=[Security.AccessControl.DirectorySecurity]::new();$acl.SetOwner($sid);$acl.SetAccessRuleProtection($true,$false);$acl.AddAccessRule([Security.AccessControl.FileSystemAccessRule]::new($sid,'FullControl','ContainerInherit,ObjectInherit','None','Allow'));Set-Acl -LiteralPath $p -AclObject $acl"
    subprocess.run(['powershell.exe','-NoProfile','-NonInteractive','-EncodedCommand',base64.b64encode(ps.encode('utf-16le')).decode()],check=True)
    home=work/'home';cwd=work/'empty-work';native=home/('.claude' if q.client=='claude' else '.codex')
    for d in [cwd,native,work/'tmp',home/'AppData/Local',home/'AppData/Roaming']:d.mkdir(parents=True,exist_ok=True)
    env={k:v for k,v in os.environ.items() if k.upper() in ['SYSTEMROOT','WINDIR','COMSPEC','PATHEXT','PATH']}
    env.update(HOME=str(home),USERPROFILE=str(home),APPDATA=str(home/'AppData/Roaming'),LOCALAPPDATA=str(home/'AppData/Local'),TEMP=str(work/'tmp'),TMP=str(work/'tmp'),NO_COLOR='1',DO_NOT_TRACK='1',OTEL_SDK_DISABLED='true',DISABLE_AUTOUPDATER='1',CLAUDE_CODE_DISABLE_NONESSENTIAL_TRAFFIC='1',DISABLE_TELEMETRY='1',DISABLE_ERROR_REPORTING='1')
    env['CLAUDE_CONFIG_DIR' if q.client=='claude' else 'CODEX_HOME']=str(native)
    if q.client=='claude':
        env['CLAUDE_CODE_GIT_BASH_PATH']=str(Path('C:/Program Files/Git/bin/bash.exe'))
        env['ANTHROPIC_AUTH_TOKEN']='synthetic-only'
    tool=cwd/'synthetic-read.txt';tool.write_text(fixtures.CODEX_TOOL_CONTENT,encoding='utf-8')
    original_match=fixtures.codex_tool_result_matches
    def inspect_tool(payload):
        matched=original_match(payload)
        if not matched:
            for item in payload.get('input',[]):
                if isinstance(item,dict) and item.get('type')=='function_call_output':
                    print('synthetic tool diagnostic:',str(item.get('output'))[:2200],file=sys.stderr)
        return matched
    fixtures.codex_tool_result_matches=inspect_tool
    fixture=(fixtures.ClaudeFixture if q.client=='claude' else fixtures.CodexFixture)(tool)
    endpoint='messages' if q.client=='claude' else 'responses'
    port=fixtures.probe_pi.reserve_loopback_port()
    config=work/'gateway.toml'
    config.write_text(f'''listen = "127.0.0.1:{port}"
startup_hold_secs = 0
concurrency = 3
accounting = "actual"
[upstream]
api_base = "http://127.0.0.1:{fixture.port}/team/v1"
[upstream.auth]
mode = "none"
[quota.rpm]
kind = "known"
value = 18
[quota.tpm]
kind = "known"
value = 450000
[[models]]
id = "example-model"
max_output_tokens = 4096
[[roots]]
id = "client"
endpoints = ["{endpoint}"]
models = ["example-model"]
''',encoding='utf-8')
    cli=[q.binary,'--config',config]
    result={'client':q.client,'managed':q.managed,'passed':False,'real_provider_called':False,'localized_path':any(ord(c)>127 for c in str(work)),'binary_sha256':hashlib.sha256(q.binary.read_bytes()).hexdigest()}
    if q.client=='claude':
        b=run([env['CLAUDE_CODE_GIT_BASH_PATH'],'--version'],env,cwd)
        result['git_bash_exit']=b.returncode
        if b.returncode: print(b.stderr.decode(errors='replace'),file=sys.stderr)
    result['version']=run([q.executable,'--version'],env,cwd).stdout.decode().strip()
    try:
        start=run(cli+['on'],env,cwd)
        assert start.returncode==0,start.stderr.decode(errors='replace')
        status=json.loads(run(cli+['status','--json'],env,cwd).stdout)
        origin='http://'+status['identity']['address']
        client_file=native/('settings.json' if q.client=='claude' else 'llmgw.config.toml')
        if q.managed:
            conn=cli+['connect',q.client,'--client-executable',q.executable,'--client-home',home,'--root','client','--model','example-model']
            preview=run(conn,env,cwd)
            result['connect_exit']=preview.returncode
            if preview.returncode:
                result['connect_error']=preview.stderr.decode(errors='replace')[:1500]
                return result
            reviewed=next(l.removeprefix('preview hash: ') for l in preview.stdout.decode().splitlines() if l.startswith('preview hash: '))
            applied=run(conn+['--apply-hash',reviewed],env,cwd)
            assert applied.returncode==0,applied.stderr.decode(errors='replace')
        elif q.client=='claude':
            client_file.write_text(json.dumps({'env':{'ANTHROPIC_BASE_URL':origin+'/r/client','ANTHROPIC_MODEL':'example-model','ANTHROPIC_API_KEY':'llmgw-local-only'}}),encoding='utf-8')
        else:raise AssertionError('Codex uses managed profile')
        if q.client=='claude':
            command=[q.executable,'-p','--output-format','stream-json','--verbose','--no-session-persistence','--no-chrome','--disable-slash-commands','--strict-mcp-config','--mcp-config','{"mcpServers":{}}','--permission-mode','dontAsk','--tools','Read','--allowedTools','Read','--setting-sources','user','--model','example-model',f'Read only the exact synthetic file {tool} and report the fixture result.']
            marker=fixtures.CLAUDE_TOOL_MARKER
        else:
            command=[q.executable,'--profile','llmgw','exec','--skip-git-repo-check','--ephemeral','--ignore-rules','--disable','plugins','--disable','remote_plugin','--disable','apps','--sandbox','read-only','--json','-C',cwd,f'Read only the exact synthetic file {tool} and report the fixture result.']
            marker=fixtures.CODEX_TOOL_MARKER
        inference=run(command,env,cwd)
        result.update(exit_code=inference.returncode,marker_seen=marker.encode() in inference.stdout,observations=fixture.observations)
        if inference.returncode or len(fixture.observations)!=2 or not result['marker_seen']:
            result['diagnostic_stderr']=inference.stderr.decode(errors='replace')[-4000:]
            result['diagnostic_stdout']=inference.stdout.decode(errors='replace')[-4000:]
        result['admission']=json.loads(run(cli+['status','--json'],env,cwd).stdout)['runtime']['admission']
        key='tool_result_matches' if q.client=='claude' else 'tool_output_matches'
        result['passed']=inference.returncode==0 and result['marker_seen'] and len(fixture.observations)==2 and fixture.observations[1][key]
        if q.managed:
            disconnected=run(cli+['disconnect',q.client],env,cwd)
            result['disconnect_exit']=disconnected.returncode
            result['passed']=result['passed'] and disconnected.returncode==0
        return result
    finally:
        stopped=run(cli+['off'],env,cwd)
        result['cleanup_exit']=stopped.returncode
        fixture.close()
        for name in ('diagnostic_stdout', 'diagnostic_stderr'):
            if name in result:
                print(name, result.pop(name), file=sys.stderr)
        (work/'result.json').write_text(json.dumps(result,indent=2)+'\n',encoding='utf-8')

if __name__=='__main__':
    result=main();print(json.dumps(result,indent=2));sys.exit(0 if result['passed'] else 1)
