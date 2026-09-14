#!/usr/bin/env python3
"""Wait for native Apply acknowledgement before editing during a paused-worker handshake."""
import pathlib
src=pathlib.Path(__file__).with_name('probe.py')
exec(compile(src.read_text().split('\ntry:\n',1)[0],str(src),'exec'))
def call(*args):
    p=subprocess.run([str(BIN),*map(str,args)],capture_output=True,text=True,timeout=18)
    if p.returncode:raise AssertionError((args,p.returncode,p.stderr))
    return p.stdout
def status(c):return json.loads(call('status','--json','--config',c))
c=fixture('synchronized_status_delay');identity=None;paused=False
try:
    call('on','--config',c);identity=status(c)['identity']
    r=PTY(c);through(r);r.expect('❯ Save only')
    os.kill(identity['pid'],signal.SIGSTOP);paused=True
    begin=time.monotonic();os.write(r.master,b'\r');r.expect('✔ Apply · Save only')
    # The menu has acknowledged application; the owned worker cannot answer
    # the bounded status handshake until resumed after the observed result.
    time.sleep(0.1);edit_at=time.monotonic()-begin
    edited=c.read_text().replace('concurrency = 3','concurrency = 4');c.write_text(edited)
    code=r.finish();elapsed=time.monotonic()-begin
    os.kill(identity['pid'],signal.SIGCONT);paused=False
    result={'exit':code,'edit_after_apply_ack_seconds':edit_at,'total_seconds':elapsed,'external_edit_preserved':c.read_text()==edited,'same_worker_identity':status(c)['identity']==identity,'forced_handshake_timeout_observed':elapsed>=0.45}
    (OUT/'status-delay-results.json').write_text(json.dumps(result,indent=2)+'\n');(OUT/'status-delay.pty.txt').write_bytes(r.buf);print(json.dumps(result,indent=2))
finally:
    if paused:os.kill(identity['pid'],signal.SIGCONT)
    for r in RUNS:r.close()
    if identity:
        call('off','--config',c);assert status(c)['state']=='stopped'
    shutil.rmtree(SCRATCH)
    (OUT/'status-delay-cleanup.json').write_text(json.dumps({'all_pty_waited':all(r.p.poll() is not None for r in RUNS),'owned_pty_pids':[r.p.pid for r in RUNS],'scratch_removed':not SCRATCH.exists(),'worker_authenticated_stopped':True},indent=2)+'\n')
