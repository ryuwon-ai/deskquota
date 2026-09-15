"""Bounded native Bifrost readiness and one synthetic request, not benchmark."""
import asyncio, contextlib, hashlib, json, os, signal, socket, subprocess, sys, time
from pathlib import Path
ROOT=Path(__file__).resolve().parents[2]
sys.path.insert(0,str(ROOT/'product/scripts'))
from benchmark_http import Mock, Client, encode
CACHE=Path('/Users/ryuwon/Library/Caches/deskquota-reference-runtimes/2026-09-15/native')
OUT=Path(__file__).resolve().parent

async def main():
    mock=Mock(None)
    await mock.start()
    app=CACHE/'bifrost-smoke-app';app.mkdir(exist_ok=True)
    config=json.loads((OUT/'bifrost-config.json').read_text())
    config['providers']['openai']['network_config']['base_url']=f'http://127.0.0.1:{mock.port}/v1'
    (app/'config.json').write_text(json.dumps(config,indent=2)+'\n')
    with socket.socket() as s:s.bind(('127.0.0.1',0));port=s.getsockname()[1]
    env={k:v for k,v in os.environ.items() if k in ['PATH','TMPDIR','HOME','USER']}
    env['GOMAXPROCS']='2'
    cmd=[str(CACHE/'bifrost-http-source'),'-app-dir',str(app),'-host','127.0.0.1','-port',str(port),'-log-level','warn']
    result={'command':cmd,'mock_port':mock.port,'gateway_port':port,'binary_sha256':hashlib.sha256((CACHE/'bifrost-http-source').read_bytes()).hexdigest(),'config_sha256':hashlib.sha256((app/'config.json').read_bytes()).hexdigest(),'scope':'smoke only; one successful generation; no performance comparison'}
    proc=None;log=(CACHE/'bifrost-smoke.log').open('w')
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
        mock.plans['smoke']={'actual_ratio':1,'service_ms':1,'metadata':False}
        client=Client(port)
        body=encode({'model':'synthetic','messages':[{'role':'user','content':'x'*32}],'max_tokens':16,'stream':True})
        try:
            response=await asyncio.wait_for(client.exchange('POST','/openai/v1/chat/completions',body,{'x-bf-eh-x-benchmark-ingress':'smoke'}),10)
            result['response']={k:v.decode(errors='replace') if isinstance(v,bytes) else v for k,v in response.items()}
            result['success']=response['status']==200 and response['output']=='fixture:smoke' and response['terminal_marker_s'] is not None
        finally:await client.close()
        result['attempts']=mock.attempts
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
        result['log_excerpt']=(CACHE/'bifrost-smoke.log').read_text()[-6000:]
        (OUT/'bifrost-smoke.json').write_text(json.dumps(result,indent=2)+'\n')
        print(json.dumps(result,indent=2))
asyncio.run(main())
