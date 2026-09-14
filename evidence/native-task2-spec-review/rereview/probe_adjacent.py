#!/usr/bin/env python3
"""Bounded adjacent rerun checks on owned configs and one idle worker."""
import pathlib
src=pathlib.Path(__file__).with_name('probe.py')
exec(compile(src.read_text().split('\ntry:\n',1)[0],str(src),'exec'))
ACTIVE=[]

def call(*args, expected=0):
    p=subprocess.run([str(BIN),*map(str,args)],capture_output=True,text=True,timeout=18)
    if p.returncode!=expected:raise AssertionError((args,p.returncode,p.stdout,p.stderr))
    return p
def status(c):return json.loads(call('status','--json','--config',c).stdout)
def finish(c, port=None):
    r=PTY(c);through(r,port=port);r.choose('Apply');return r,r.finish()

try:
    c=fixture('metadata_only',cap=False,endpoints=['models']);before=c.read_bytes();r,code=finish(c)
    RESULTS['metadata_only']={'exit':code,'same_bytes':before==c.read_bytes(),'no_generation_invented':c.read_bytes()==before}
    c=fixture('writer_lock');before=c.read_bytes();lp=c.with_name('.'+c.name+'.setup.lock')
    with open(lp,'wb') as lock:
        os.chmod(lp,0o600);fcntl.flock(lock,fcntl.LOCK_EX|fcntl.LOCK_NB)
        r,code=finish(c,port=4142)
        RESULTS['writer_lock']={'exit':code,'same_bytes':before==c.read_bytes(),'conflict_message':b'another setup apply is in progress' in r.buf}
    c=fixture('permissions_acl');os.chmod(c,0o640)
    subprocess.run(['/bin/chmod','+a','everyone allow read',str(c)],check=True,capture_output=True,timeout=3)
    acl=lambda:subprocess.check_output(['/bin/ls','-le',str(c)],text=True,timeout=3).splitlines()[1:]
    before=c.stat();ab=acl();r,code=finish(c,port=4142);after=c.stat()
    RESULTS['permissions_acl']={'exit':code,'mode_before':oct(before.st_mode&0o777),'mode_after':oct(after.st_mode&0o777),'same_owner_group':(before.st_uid,before.st_gid)==(after.st_uid,after.st_gid),'acl_preserved':ab==acl(),'inode_replaced':before.st_ino!=after.st_ino}
    c=fixture('group_refusal');groups=[g for g in os.getgroups() if g!=c.stat().st_gid]
    if groups:
        os.chown(c,-1,groups[0]);before=c.read_bytes();st=c.stat();r,code=finish(c,port=4142)
        RESULTS['group_refusal']={'exit':code,'same_bytes':before==c.read_bytes(),'same_gid':c.stat().st_gid==st.st_gid,'explicit_refusal':b'ownership could not be preserved' in r.buf}
    else:RESULTS['group_refusal']={'skipped':'no alternate supplementary group available'}
    c=fixture('worker_status_delay');call('on','--config',c);ACTIVE.append(c);identity=status(c)['identity'];r=PTY(c);through(r);r.expect('Apply')
    os.kill(identity['pid'],signal.SIGSTOP)
    try:
        begin=time.monotonic();os.write(r.master,b'\r')
        # Bound the forced handshake delay; the worker is resumed in finally.
        time.sleep(0.15)
        edited=c.read_text().replace('concurrency = 3','concurrency = 4');c.write_text(edited)
        code=r.finish();elapsed=time.monotonic()-begin
    finally:os.kill(identity['pid'],signal.SIGCONT)
    after=status(c)
    RESULTS['snapshot_after_status_delay']={'exit':code,'elapsed_seconds':elapsed,'external_edit_preserved':c.read_text()==edited,'same_worker_identity':after['identity']==identity,'pending_restart':after['pending_restart']}
    call('off','--config',c);assert status(c)['state']=='stopped';ACTIVE.remove(c)
    c=fixture('preexisting_pending_restart');call('on','--config',c);ACTIVE.append(c);before=status(c)['identity']
    c.write_text(c.read_text().replace('concurrency = 3','concurrency = 4'))
    r=PTY(c);through(r);r.choose('Apply',b'\x1b[A\r');code=r.finish();after=status(c)
    RESULTS['preexisting_pending_restart']={'exit':code,'same_identity':before==after['identity'],'pending_restart':after['pending_restart'],'restart_required_error':b'restart_required' in r.buf,'summary_claims_no_restart_required':b'no restart required by setup edits' in r.buf}
    call('off','--config',c);assert status(c)['state']=='stopped';ACTIVE.remove(c)
    c=fixture('live_malformed');call('on','--config',c);ACTIVE.append(c);before=status(c)['identity'];c.write_text('not valid toml')
    on=call('on','--config',c,expected=1);after=status(c)
    RESULTS['live_malformed']={'on_exit':on.returncode,'same_identity':before==after['identity'],'pending_restart':after['pending_restart'],'restart_required_error':'restart_required' in on.stderr}
    call('off','--config',c);assert status(c)['state']=='stopped';ACTIVE.remove(c)
    (OUT/'adjacent-results.json').write_text(json.dumps(RESULTS,indent=2)+'\n')
    for i,r in enumerate(RUNS):(OUT/f'adjacent-{i}.pty.txt').write_bytes(r.buf)
    print(json.dumps(RESULTS,indent=2))
finally:
    for r in RUNS:r.close()
    for c in ACTIVE:
        call('off','--config',c)
        assert status(c)['state']=='stopped'
    shutil.rmtree(SCRATCH)
    (OUT/'adjacent-cleanup.json').write_text(json.dumps({'all_pty_waited':all(r.p.poll() is not None for r in RUNS),'owned_pty_pids':[r.p.pid for r in RUNS],'scratch_removed':not SCRATCH.exists(),'all_workers_authenticated_stopped':True},indent=2)+'\n')
