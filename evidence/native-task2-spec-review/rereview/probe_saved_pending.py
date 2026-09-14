#!/usr/bin/env python3
"""Save-only followed by setup SaveAndStart must apply the pending configuration."""
import pathlib
src=pathlib.Path(__file__).with_name('probe.py')
exec(compile(src.read_text().split('\ntry:\n',1)[0],str(src),'exec'))
def call(*args):
    p=subprocess.run([str(BIN),*map(str,args)],capture_output=True,text=True,timeout=18)
    if p.returncode:raise AssertionError((args,p.returncode,p.stderr))
    return p.stdout
def status(c):return json.loads(call('status','--json','--config',c))
c=fixture('saved_pending');started=False
try:
    call('on','--config',c);started=True;original=status(c)
    with socket.socket() as s:s.bind(('127.0.0.1',0));port=s.getsockname()[1]
    first=PTY(c);through(first,port=port);first.choose('Apply');first_code=first.finish();saved=status(c)
    saved_fingerprint=digest(c.read_bytes())
    second=PTY(c);through(second);second.choose('Apply',b'\x1b[A\r');second_code=second.finish();after=status(c)
    result={'save_only_exit':first_code,'save_only_pending_restart':saved['pending_restart'],'save_only_preserved_worker_identity':saved['identity']==original['identity'],'saved_config_fingerprint':saved_fingerprint,'save_and_start_exit':second_code,'worker_identity_after_save_and_start':after['identity'],'pending_restart_after_save_and_start':after['pending_restart'],'saved_config_applied':after['identity']['fingerprint']==saved_fingerprint,'preview_says_no_restart_required':b'no restart required by setup edits' in second.buf,'preview_says_idempotent_on':b'Save and start uses authenticated idempotent on' in second.buf,'restart_required_error':b'error: restart_required' in second.buf}
    (OUT/'saved-pending-results.json').write_text(json.dumps(result,indent=2)+'\n')
    (OUT/'saved-pending-save-only.pty.txt').write_bytes(first.buf);(OUT/'saved-pending-save-start.pty.txt').write_bytes(second.buf)
    print(json.dumps(result,indent=2))
finally:
    for r in RUNS:r.close()
    if started:
        call('off','--config',c);assert status(c)['state']=='stopped'
    shutil.rmtree(SCRATCH)
    (OUT/'saved-pending-cleanup.json').write_text(json.dumps({'all_pty_waited':all(r.p.poll() is not None for r in RUNS),'owned_pty_pids':[r.p.pid for r in RUNS],'scratch_removed':not SCRATCH.exists(),'worker_authenticated_stopped':True},indent=2)+'\n')
