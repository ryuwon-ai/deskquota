"""Bounded native Bifrost readiness and one synthetic request, not benchmark."""
import asyncio, contextlib, hashlib, json, os, signal, socket, subprocess, sys, time
from pathlib import Path
ROOT=Path(__file__).resolve().parents[2]
sys.path.insert(0,str(ROOT/'product/scripts'))
from benchmark_http import Client, encode
CACHE=Path('/Users/ryuwon/Library/Caches/deskquota-reference-runtimes/2026-09-15/native')
OUT=Path(__file__).resolve().parent

class Mock:
    def __init__(self):
        self.attempts=[]
        self.writers=set()
    async def start(self):
        self.server=await asyncio.start_server(self.handle,'127.0.0.1',0)
        self.port=self.server.sockets[0].getsockname()[1]
    async def close(self):
        self.server.close();await self.server.wait_closed()
        for w in list(self.writers):w.close()
    async def handle(self,reader,writer):
        self.writers.add(writer)
        try:
            while True:
                head=await reader.readuntil(b'\r\n\r\n')
                lines=head.decode().split('\r\n');headers=dict(v.lower().split(':',1) for v in lines[1:] if v)
                body=await reader.readexactly(int(headers.get('content-length','0')))
                payload=json.loads(body) if body else {}
                ingress=headers.get('x-benchmark-ingress','').strip()
                event={'path':lines[0].split()[1],'ingress':ingress,'body_bytes':len(body),'body_sha256':hashlib.sha256(body).hexdigest(),'payload_keys':list(payload),'model':payload.get('model'),'max_tokens':payload.get('max_tokens'),'max_completion_tokens':payload.get('max_completion_tokens'),'stream_options':payload.get('stream_options'),'received_s':time.monotonic()}
                self.attempts.append(event)
                if event['path']=='/v1/models':
                    data=encode({'object':'list','data':[{'id':'synthetic','object':'model','created':0,'owned_by':'fixture'}]})
                    writer.write(f'HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {len(data)}\r\n\r\n'.encode()+data)
                elif event['path']!='/v1/chat/completions':
                    raise ValueError('unexpected request path: '+event['path'])
                elif ingress=='reject':
                    data=encode({'error':{'type':'rate_limit_error','code':'rate_limit_exceeded','message':'Synthetic quota exhausted'}})
                    writer.write(f'HTTP/1.1 429 Too Many Requests\r\nContent-Type: application/json\r\nRetry-After: 1\r\nContent-Length: {len(data)}\r\n\r\n'.encode()+data)
                else:
                    writer.write(b'HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\nTransfer-Encoding: chunked\r\n\r\n')
                    events=[{'choices':[{'index':0,'delta':{'role':'assistant'}}]},{'choices':[{'index':0,'delta':{'content':'fixture:'+ingress}}]},{'choices':[{'index':0,'delta':{},'finish_reason':'stop'}]},{'choices':[],'usage':{'prompt_tokens':32,'completion_tokens':16,'total_tokens':48}}]
                    for value in events:
                        frame=b'data: '+encode(value)+b'\n\n';writer.write(f'{len(frame):x}\r\n'.encode()+frame+b'\r\n')
                    frame=b'data: [DONE]\n\n';writer.write(f'{len(frame):x}\r\n'.encode()+frame+b'\r\n0\r\n\r\n')
                await writer.drain()
        except (OSError,asyncio.IncompleteReadError):pass
        finally:
            self.writers.discard(writer);writer.close()
            with contextlib.suppress(OSError):await writer.wait_closed()

async def main():
    quota='--quota' in sys.argv
    mock=Mock()
    await mock.start()
    app=CACHE/(f'bifrost-quota-smoke-{time.time_ns()}' if quota else 'bifrost-smoke-app');app.mkdir(exist_ok=True)
    config=json.loads((OUT/('bifrost-quota-config.json' if quota else 'bifrost-config.json')).read_text())
    if quota:
        config['config_store']['config']['path']=str(app/'config.db')
        config['governance']['rate_limits'][0]['request_max_limit']=1
    config['providers']['openai']['network_config']['base_url']=f'http://127.0.0.1:{mock.port}'
    (app/'config.json').write_text(json.dumps(config,indent=2)+'\n')
    with socket.socket() as s:s.bind(('127.0.0.1',0));port=s.getsockname()[1]
    env={k:v for k,v in os.environ.items() if k in ['PATH','TMPDIR','HOME','USER']}
    env['GOMAXPROCS']='2'
    cmd=[str(CACHE/'bifrost-http-source'),'-app-dir',str(app),'-host','127.0.0.1','-port',str(port),'-log-level','warn']
    result={'command':cmd,'mock_port':mock.port,'gateway_port':port,'binary_sha256':hashlib.sha256((CACHE/'bifrost-http-source').read_bytes()).hexdigest(),'config_sha256':hashlib.sha256((app/'config.json').read_bytes()).hexdigest(),'scope':'smoke only; one successful generation; no performance comparison'}
    result['quota_enforcement_probe']=quota
    result['app_dir']=str(app)
    logfile=CACHE/('bifrost-quota-smoke.log' if quota else 'bifrost-smoke.log')
    proc=None;log=logfile.open('w')
    try:
        proc=subprocess.Popen(cmd,env=env,stdout=log,stderr=subprocess.STDOUT,start_new_session=True)
        ready=False
        for _ in range(100):
            if proc.poll() is not None:break
            try:
                rd,wr=await asyncio.open_connection('127.0.0.1',port);wr.close();await wr.wait_closed();ready=True;break
            except OSError:await asyncio.sleep(.1)
        result['listener_ready']=ready
        if not ready:raise RuntimeError('Bifrost listener not ready')
        client=Client(port)
        body=encode({'model':'synthetic','messages':[{'role':'user','content':'x'*32}],'max_tokens':16,'stream':True})
        try:
            response=await asyncio.wait_for(client.exchange('POST','/openai/v1/chat/completions',body,{'x-bf-eh-x-benchmark-ingress':'smoke'}),10)
            result['response']={k:v.decode(errors='replace') if isinstance(v,bytes) else v for k,v in response.items()}
            events=[json.loads(frame[6:]) for frame in response['body'].decode().split('\n\n') if frame.startswith('data: {')]
            result['object_usage_events']=[event['usage'] for event in events if isinstance(event.get('usage'),dict)]
            result['null_usage_event_count']=sum(event.get('usage','absent') is None for event in events)
            result['success']=response['status']==200 and response['output']=='fixture:smoke' and response['terminal_marker_s'] is not None
        finally:await client.close()
        reject_client=Client(port)
        try:
            reject=await asyncio.wait_for(reject_client.exchange('POST','/openai/v1/chat/completions',body,{'x-bf-eh-x-benchmark-ingress':'quota-second' if quota else 'reject'}),5)
            result['reject_status']=reject['status']
        finally:await reject_client.close()
        result['attempts']=mock.attempts
        result['no_429_retry']=not any(a['ingress']=='quota-second' for a in mock.attempts) if quota else sum(a['ingress']=='reject' for a in mock.attempts)==1
        result['quota_enforced']=quota and result['reject_status']==429 and not any(a['ingress']=='quota-second' for a in mock.attempts)
        result['original_body_bytes']=len(body)
        result['runtime_files']=[str(p.relative_to(app)) for p in app.rglob('*') if p.is_file()]
    except Exception as exc:result['error']=type(exc).__name__+': '+str(exc)
    finally:
        if proc is not None:
            proc.terminate()
            try:await asyncio.to_thread(proc.wait,5)
            except subprocess.TimeoutExpired:proc.kill();await asyncio.to_thread(proc.wait)
            result['process_exit_code']=proc.returncode
        await mock.close();log.close()
        result['log_excerpt']=logfile.read_text()[-6000:]
        (OUT/('bifrost-quota-smoke.json' if quota else 'bifrost-smoke.json')).write_text(json.dumps(result,indent=2)+'\n')
        print(json.dumps({k:v for k,v in result.items() if k not in ['log_excerpt','response']},indent=2))
        assert result.get('success') and (result.get('quota_enforced') if quota else result.get('no_429_retry'))
        assert result['object_usage_events']==[{'prompt_tokens':32,'completion_tokens':16,'total_tokens':48}]
        if not quota:assert result['runtime_files']==['config.json']
asyncio.run(main())
