#!/usr/bin/env python3
"""An unchanged setup save-and-start must disclose actual running-worker impact."""
import pathlib
src=pathlib.Path(__file__).with_name('green_probe.py')
# Reuse only this review's fixture/PTy definitions, not its test body.
exec(compile(src.read_text().split('\ntry:\n',1)[0], str(src), 'exec'))

def call(*args):
    p=subprocess.run([str(BIN),*map(str,args)],capture_output=True,text=True,timeout=18)
    if p.returncode:raise AssertionError((args,p.returncode,p.stdout,p.stderr))
    return p.stdout

def status(c):return json.loads(call('status','--json','--config',c))

c=fixture('running_noop');clean=False
try:
    call('on','--config',c);before=status(c)
    r=PTY(c);through(r);r.choose('Apply',b'\x1b[A\r');code=r.finish();after=status(c)
    result={'setup_exit':code,'before_identity':before.get('identity'),'after_identity':after.get('identity'),'same_fingerprint':before['identity']['fingerprint']==after['identity']['fingerprint'],'identity_changed':before['identity']!=after['identity'],'summary_says_no_restart_required':b'no restart required by setup edits' in r.buf,'summary_mentions_cancel_or_drain':b'cancels queued' in r.buf or b'drains active' in r.buf}
    (OUT/'green-running-noop-results.json').write_text(json.dumps(result,indent=2)+'\n')
    (OUT/'green-running-noop.pty.txt').write_bytes(r.buf)
    print(json.dumps(result,indent=2))
finally:
    for r in RUNS:r.close()
    call('off','--config',c);st=status(c)
    if st['state']!='stopped':raise AssertionError('owned worker not stopped; preserving scratch')
    shutil.rmtree(SCRATCH)
    (OUT/'green-running-cleanup.json').write_text(json.dumps({'state':st['state'],'all_pty_waited':all(r.p.poll() is not None for r in RUNS),'scratch_removed':not SCRATCH.exists(),'owned_pty_pids':[r.p.pid for r in RUNS]},indent=2)+'\n')
