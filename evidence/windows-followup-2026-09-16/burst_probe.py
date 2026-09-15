import concurrent.futures,hashlib,http.client,json,os,subprocess,threading,time
from http.server import BaseHTTPRequestHandler,ThreadingHTTPServer
from pathlib import Path
r=Path(os.environ['LOCALAPPDATA'])/'deskquota-windows-lab-20260915'
b=r/'acceptance-02/installed space/llmgw.exe';w=r/'client-tests-20260916/cache-burst';w.mkdir(exist_ok=False)
attempts=0;lock=threading.Lock()
class Handler(BaseHTTPRequestHandler):
 protocol_version='HTTP/1.1'
 def log_message(self,*args):pass
 def do_POST(self):
  global attempts
  payload=json.loads(self.rfile.read(int(self.headers['Content-Length'])))
  with lock:attempts+=1
  time.sleep(.15)
  body=json.dumps({'id':'synthetic','choices':[{'index':0,'message':{'role':'assistant','content':'ok'},'finish_reason':'stop'}],'usage':{'prompt_tokens':20,'completion_tokens':3}}).encode()
  self.send_response(200);self.send_header('Content-Type','application/json');self.send_header('Content-Length',str(len(body)));self.end_headers();self.wfile.write(body)
u=ThreadingHTTPServer(('127.0.0.1',0),Handler);u.daemon_threads=True
threading.Thread(target=u.serve_forever,daemon=True).start()
results=[]
try:
 for parallel in [False,True,True,True]:
  c=w/f'config-{len(results)}.toml'
  c.write_text(f'''listen = "127.0.0.1:0"
startup_hold_secs = 0
concurrency = 3
[cache]
[upstream]
api_base = "http://127.0.0.1:{u.server_port}/v1"
[upstream.auth]
mode = "none"
[quota.rpm]
kind = "known"
value = 18
[quota.tpm]
kind = "known"
value = 450000
[[models]]
id = "synthetic"
max_output_tokens = 64
[[roots]]
id = "one"
endpoints = ["chat/completions"]
models = ["synthetic"]
''')
  cli=[str(b),'--config',str(c)]
  def run(args):return subprocess.run(cli+args,capture_output=True,timeout=10,check=True)
  run(['on'])
  try:
   state=json.loads(run(['status','--json']).stdout);address=state['identity']['address'];start=attempts
   barrier=threading.Barrier(10) if parallel else None
   body=json.dumps({'model':'synthetic','messages':[{'role':'user','content':'same synthetic request'}],'max_tokens':64,'temperature':0,'stream':False})
   def request(_):
    conn=http.client.HTTPConnection(address,timeout=10)
    if barrier:barrier.wait(timeout=5)
    conn.request('POST','/r/one/v1/chat/completions',body,{'Content-Type':'application/json'})
    response=conn.getresponse();data=response.read();conn.close();return response.status,json.loads(data)['choices'][0]['message']['content']
   if parallel:
    with concurrent.futures.ThreadPoolExecutor(max_workers=10) as pool:responses=list(pool.map(request,range(10)))
   else:responses=[request(i) for i in range(10)]
   assert responses==[(200,'ok')]*10
   state=json.loads(run(['status','--json']).stdout)['runtime']
   results.append({'parallel':parallel,'submitted':10,'completed':10,'failures':0,'upstream_attempts':attempts-start,'cache':state['exact_cache'],'admission':state['admission']})
  finally:run(['off'])
finally:u.shutdown();u.server_close()
result={'binary_sha256':hashlib.sha256(b.read_bytes()).hexdigest(),'runs':results,'real_provider_called':False,'scope':'synthetic correctness/call count; not throughput benchmark'}
(w/'result.json').write_text(json.dumps(result,indent=2)+'\n');print(json.dumps(result,indent=2))
