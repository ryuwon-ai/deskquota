#!/usr/bin/env python3
"""Actual CLI checks of preview consent, desired identity, and operation-lock guards."""
import pathlib
src=pathlib.Path(__file__).with_name('support.py')
exec(compile(src.read_text(),str(src),'exec'))
ACTIVE=[]
RESTORE=[]
def call(*args):
    p=subprocess.run([str(BIN),*map(str,args)],capture_output=True,text=True,timeout=20)
    if p.returncode:raise AssertionError((args,p.returncode,p.stderr))
    return p.stdout
def status(c):return json.loads(call('status','--json','--config',c))
def start(c):
    call('on','--config',c);ACTIVE.append(c);return status(c)
def stop(c):
    call('off','--config',c);assert status(c)['state']=='stopped';ACTIVE.remove(c)
def menu(c,port=None):
    r=PTY(c);through(r,port=port);r.expect('❯ Save only');return r
def start_selection(r):
    os.write(r.master,b'\x1b[A\r');r.expect('✔ Apply · Save and start')
def changed_port(c):
    text=c.read_text()
    with socket.socket() as s:s.bind(('127.0.0.1',0));port=s.getsockname()[1]
    old=text.split('listen = "127.0.0.1:',1)[1].split('"',1)[0]
    c.write_text(text.replace('127.0.0.1:'+old,'127.0.0.1:'+str(port),1))
    return port
def disclosure(r, worker, desired):
    return all(x in r.buf for x in [worker.encode(),desired.encode(),b'cancels queued requests',b'drains active requests for up to 10 seconds',b'verifies readiness for the desired fingerprint'])
def await_pending(c):
    pending=c.with_name('.'+c.name+'.setup-pending.json');end=time.monotonic()+3
    while not pending.exists():
        if time.monotonic()>end:raise AssertionError('setup did not finish publishing pending metadata')
        time.sleep(0.01)
    return pending
try:
    c=fixture('external_predating');before=start(c);changed_port(c);desired=digest(c.read_bytes())
    r=menu(c);shown=disclosure(r,before['identity']['fingerprint'],desired);start_selection(r);code=r.finish();after=status(c)
    RESULTS['external_predating']={'exit':code,'both_fingerprints_and_restart_disclosed':shown,'desired_applied':after['identity']['fingerprint']==desired,'pending_restart':after['pending_restart'],'worker_changed':before['identity']!=after['identity']};stop(c)

    c=fixture('revert_pending_to_worker');before=start(c);original=c.read_bytes();oldport=int(before['identity']['address'].rsplit(':',1)[1]);changed_port(c)
    assert status(c)['pending_restart']
    r=menu(c,port=oldport);shown=b'authenticated running worker matches; no restart required' in r.buf;start_selection(r);code=r.finish();after=status(c)
    RESULTS['desired_matches_worker']={'exit':code,'matching_preview':shown,'original_bytes_restored':c.read_bytes()==original,'same_identity':after['identity']==before['identity'],'pending_restart':after['pending_restart']};stop(c)

    c=fixture('unknown_worker');before=start(c);original=c.read_bytes();token=pathlib.Path(before['state_directory'])/'control-token';held=token.with_name('control-token.owned-held')
    token.rename(held);RESTORE.append((held,token))
    r=menu(c);shown=b'worker fingerprint could not be authenticated' in r.buf;start_selection(r);code=r.finish()
    held.rename(token);RESTORE.remove((held,token));after=status(c)
    RESULTS['unverified_refusal']={'exit':code,'unverified_preview':shown,'same_config_bytes':c.read_bytes()==original,'same_identity':after['identity']==before['identity'],'pending_metadata_absent':not c.with_name('.'+c.name+'.setup-pending.json').exists()};stop(c)

    c=fixture('unpreviewed_worker_mismatch');original=c.read_bytes();r=menu(c);shown=b'lifecycle state is stopped' in r.buf
    changed_port(c);before=start(c);c.write_bytes(original);start_selection(r);code=r.finish();after=status(c)
    RESULTS['unpreviewed_mismatch']={'exit':code,'initial_preview_stopped':shown,'refusal_explained':b'restart impact was not previewed' in r.buf,'same_config_bytes':c.read_bytes()==original,'same_identity':after['identity']==before['identity'],'pending_metadata_absent':not c.with_name('.'+c.name+'.setup-pending.json').exists()};stop(c)

    for action in ['restart','on']:
        c=fixture('expected_guard_'+action);before=start(c)
        if action=='on':stop(c)
        else:changed_port(c)
        r=menu(c);state=status(c);lockpath=pathlib.Path(state['state_directory'])/'operation.lock'
        with open(lockpath,'r+b') as lock:
            fcntl.flock(lock,fcntl.LOCK_EX|fcntl.LOCK_NB)
            start_selection(r);await_pending(c)
            external=c.read_text().replace('concurrency = 3','concurrency = 4');c.write_text(external)
            fcntl.flock(lock,fcntl.LOCK_UN)
        code=r.finish();after=status(c)
        RESULTS['expected_guard_'+action]={'exit':code,'expected_hash_error':('configuration_changed_before_'+('restart' if action=='restart' else 'start')).encode() in r.buf,'external_bytes_preserved':c.read_text()==external,'state':after['state'],'same_identity':after.get('identity')==before.get('identity') if action=='restart' else None}
        if action=='restart':stop(c)

    c=fixture('restart_preview_becomes_matching');before=start(c);changed_port(c);r=menu(c)
    call('restart','--config',c);matching=status(c);start_selection(r);code=r.finish();after=status(c)
    RESULTS['restart_preview_becomes_matching']={'exit':code,'same_matching_identity':after['identity']==matching['identity'],'pending_restart':after['pending_restart']};stop(c)
    for i,r in enumerate(RUNS):(OUT/f'boundary-{i}.pty.txt').write_bytes(r.buf)
    (OUT/'boundary-results.json').write_text(json.dumps(RESULTS,indent=2)+'\n');print(json.dumps(RESULTS,indent=2))
finally:
    for held,token in RESTORE:held.rename(token)
    for r in RUNS:r.close()
    for c in list(ACTIVE):stop(c)
    shutil.rmtree(SCRATCH)
    (OUT/'boundary-cleanup.json').write_text(json.dumps({'all_pty_waited':all(r.p.poll() is not None for r in RUNS),'owned_pty_pids':[r.p.pid for r in RUNS],'scratch_removed':not SCRATCH.exists(),'all_workers_authenticated_stopped':True,'restored_control_fixtures':not RESTORE},indent=2)+'\n')
