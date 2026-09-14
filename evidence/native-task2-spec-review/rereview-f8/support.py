#!/usr/bin/env python3
"""Independent release-binary SPEC counterexamples; owned PTYs and loopback only."""
import fcntl, hashlib, http.server, json, os, pathlib, pty, resource, select, shutil, signal, socket, subprocess, termios, threading, time

ROOT = pathlib.Path(__file__).resolve().parents[3]
OUT = pathlib.Path(__file__).resolve().parent
BIN = ROOT / 'product/target/native-task2-f8-fix/release/llmgw'
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
