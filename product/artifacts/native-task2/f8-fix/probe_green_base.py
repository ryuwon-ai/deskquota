#!/usr/bin/env python3
"""Independent release-binary SPEC counterexamples; owned PTYs and loopback only."""
import fcntl, hashlib, http.server, json, os, pathlib, pty, resource, select, shutil, signal, socket, subprocess, termios, threading, time

ROOT = pathlib.Path(__file__).resolve().parents[3]
OUT = pathlib.Path(__file__).resolve().parent
BIN = ROOT / 'target/native-task2-f8-fix/release/llmgw'
SCRATCH = OUT / 'scratch'
SCRATCH.mkdir(exist_ok=False)
RUNS = []
RESULTS = {}

def digest(b): return hashlib.sha256(b).hexdigest()

def fixture(name, cap=True, auth=False, endpoints=None):
    d = SCRATCH / name
    d.mkdir()
    c = d / 'gateway 설정.toml'
    with socket.socket() as s:
        s.bind(('127.0.0.1', 0)); port = s.getsockname()[1]
    c.write_text(f'''# preserved user heading
listen = "127.0.0.1:{port}" # port comment
concurrency = 3
accounting = "actual"
cancel_policy = "close"
retry_transient_429 = true
[upstream]
api_base = "http://127.0.0.1:9/company/v1"
auth = { '{ mode = "env", header = "x-api-key", name = "SPEC_EXISTING_AUTH" }' if auth else '{ mode = "none" }' }
[quota]
rpm = {{ kind = "unlimited" }}
tpm = {{ kind = "unlimited" }}
[[models]]
id = "model-a"
{ 'max_output_tokens = 64' if cap else '' }
[[roots]]
id = "pi"
endpoints = {json.dumps(endpoints or ['responses'])}
models = ["model-a"]
''')
    return c

class PTY:
    def __init__(self, config, size_limit=None):
        self.master, slave = pty.openpty(); self.buf = b''; self.pos = 0
        def attach():
            os.setsid(); fcntl.ioctl(slave, termios.TIOCSCTTY, 0)
            if size_limit is not None: resource.setrlimit(resource.RLIMIT_FSIZE, (size_limit, size_limit))
        self.p = subprocess.Popen([str(BIN), 'setup', '--config', str(config)], stdin=slave, stdout=slave, stderr=slave, preexec_fn=attach, close_fds=True)
        os.close(slave); RUNS.append(self)
    def expect(self, marker):
        end=time.monotonic()+5; m=marker.encode()
        while True:
            idx=self.buf.find(m,self.pos)
            if idx>=0: self.pos=idx+len(m); return
            if time.monotonic()>=end: raise AssertionError('timeout '+marker+' '+repr(self.buf[-1500:]))
            if select.select([self.master],[],[],0.1)[0]:
                try: chunk=os.read(self.master,65536)
                except OSError: chunk=b''
                if not chunk: raise AssertionError('closed '+marker+' '+repr(self.buf[-1500:]))
                self.buf+=chunk
    def choose(self, marker, value=b'\r'):
        self.expect(marker);os.write(self.master,value)
    def finish(self):
        end=time.monotonic()+8
        while self.p.poll() is None and time.monotonic()<end:
            if select.select([self.master],[],[],0.05)[0]:
                try:self.buf+=os.read(self.master,65536)
                except OSError:pass
        code=self.p.wait(timeout=1)
        # Drain any final bytes after exit.
        while select.select([self.master],[],[],0)[0]:
            try: b=os.read(self.master,65536)
            except OSError: break
            if not b:break
            self.buf+=b
        return code
    def close(self):
        if self.p.poll() is None:
            os.killpg(self.p.pid,signal.SIGTERM)
            try:self.p.wait(timeout=2)
            except subprocess.TimeoutExpired:os.killpg(self.p.pid,signal.SIGKILL);self.p.wait(timeout=2)
        os.close(self.master)

def through(r, auth=False, port=None):
    for m in ['Setup: Environment','Environment preset','Setup: Connection','Upstream API base','Actually supported endpoint','Upstream auth']:r.choose(m)
    if auth:
        r.choose('Header name');r.choose('Environment variable')
    r.choose('Setup: Models');r.choose('Keep all existing models')
    for m in ['Setup: Quota','RPM','TPM','same quota shared','separate input/output','Concurrency','Setup: Run']:r.choose(m)
    r.choose('Local loopback port',b'\r' if port is None else f'{port}\r'.encode())
    r.choose('Request start at next login');r.choose('Setup: Tools');r.choose('Client intents');r.choose('Setup: Apply')

def save_case(name, **kwargs):
    c=fixture(name,**kwargs); before=c.read_bytes();r=PTY(c);through(r,auth=kwargs.get('auth',False));r.choose('Apply');code=r.finish()
    return c,r,{'exit':code,'same_bytes':before==c.read_bytes(),'before_sha':digest(before),'after_sha':digest(c.read_bytes())}

try:
    c,r,result=save_case('noop'); RESULTS['noop']=result
    c,r,result=save_case('auth_defaults',auth=True)
    result['old_reference_preserved']='SPEC_EXISTING_AUTH' in c.read_text()
    result['old_header_preserved']='x-api-key' in c.read_text()
    result['new_default_reference']='LLMGW_UPSTREAM_AUTH' in c.read_text()
    RESULTS['auth_defaults']=result
    c=fixture('pending_defaults'); pending=c.with_name('.'+c.name+'.setup-pending.json')
    before={'version':1,'shared_with_other_pcs':True,'separate_input_output':True,'login_requested':True,'clients':['pi','codex']}
    pending.write_text(json.dumps(before));r=PTY(c);through(r);r.choose('Apply');code=r.finish()
    RESULTS['pending_defaults']={'exit':code,'before':before,'after':json.loads(pending.read_text())}
    c,r,result=save_case('endpoint_defaults',endpoints=['responses','models','messages/count_tokens'])
    result['models_endpoint_preserved']='"models"' in c.read_text()
    result['count_tokens_preserved']='messages/count_tokens' in c.read_text()
    RESULTS['endpoint_defaults']=result
    c=fixture('request_bounded',cap=False);before=c.read_bytes()
    doctor=subprocess.run([str(BIN),'doctor','--json','--config',str(c)],capture_output=True,text=True,timeout=5)
    r=PTY(c);through(r);r.choose('Apply');code=r.finish()
    RESULTS['request_bounded']={'doctor_exit':doctor.returncode,'setup_exit':code,'same_bytes':before==c.read_bytes(),'error':'each model requires a nonzero output bound' in r.buf.decode()}
    c=fixture('concurrent_edit');r=PTY(c);through(r);r.expect('Apply')
    external=c.read_text().replace('concurrency = 3','concurrency = 4');c.write_text(external)
    os.write(r.master,b'\r');code=r.finish()
    RESULTS['concurrent_edit']={'exit':code,'external_edit_preserved':'concurrency = 4' in c.read_text(),'stale_value_restored':'concurrency = 3' in c.read_text()}
    c=fixture('partial_write');before=c.read_bytes();r=PTY(c,100);through(r,port=4142);r.choose('Apply');code=r.finish()
    RESULTS['partial_write']={'exit':code,'before_bytes':len(before),'after_bytes':c.stat().st_size,'same_bytes':before==c.read_bytes(),'signal_name':'SIGXFSZ' if code==-signal.SIGXFSZ else None}
    c=fixture('invalid_cli');c.write_text('not valid toml')
    RESULTS['invalid_cli']={}
    for command in ['doctor','on']:
        p=subprocess.run([str(BIN),command,'--config',str(c)],capture_output=True,text=True,timeout=5)
        RESULTS['invalid_cli'][command]={'exit':p.returncode,'stderr':p.stderr.strip()}
    c=fixture('missing_inference_args')
    p=subprocess.run([str(BIN),'doctor','--inference','--config',str(c)],capture_output=True,text=True,timeout=5)
    RESULTS['missing_inference_args']={'exit':p.returncode,'stderr':p.stderr.strip()}
    for name in ['noop','auth_defaults','pending_defaults','endpoint_defaults','request_bounded','concurrent_edit','partial_write']:
        # Transcript contains only synthetic loopback configuration and no env values.
        index=['noop','auth_defaults','pending_defaults','endpoint_defaults','request_bounded','concurrent_edit','partial_write'].index(name)
        (OUT/(name+'.pty.txt')).write_bytes(RUNS[index].buf)
    (OUT/'probe-results.json').write_text(json.dumps(RESULTS,indent=2)+'\n')
    print(json.dumps(RESULTS,indent=2))
finally:
    for r in RUNS:r.close()
    cleanup={'owned_pids':[r.p.pid for r in RUNS],'all_waited':all(r.p.poll() is not None for r in RUNS),'scratch':str(SCRATCH),'workers_started':0}
    shutil.rmtree(SCRATCH);cleanup['scratch_removed']=not SCRATCH.exists()
    (OUT/'probe-cleanup.json').write_text(json.dumps(cleanup,indent=2)+'\n')
