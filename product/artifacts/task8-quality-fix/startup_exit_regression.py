import asyncio
import json
from pathlib import Path
import sys
import tempfile
ROOT=Path(__file__).resolve().parents[2]
sys.path.insert(0,str(ROOT/'scripts'));sys.dont_write_bytecode=True
import benchmark as b
OUT=Path(__file__).resolve().parent
async def check():
    binary=ROOT/'target/release/llmgw'; reference=ROOT/'target/bench/release/examples/bench_gateway'
    spawn=asyncio.create_subprocess_exec; owned=[]
    async def invalid_arguments(*args,**kwargs):
        if args[0]==str(binary): args=(*args,'--invalid-startup-fixture')
        proc=await spawn(*args,**kwargs)
        if args[0]==str(binary): owned.append(proc)
        return proc
    asyncio.create_subprocess_exec=invalid_arguments
    try:
        with tempfile.TemporaryDirectory(prefix='startup-exit-',dir=OUT) as tmp:
            try: await asyncio.wait_for(b.run_arm('production_rr',950,1,binary,reference,'smoke',Path(tmp)),5)
            except RuntimeError as error: assert str(error)=='gateway failed startup'
            else: raise AssertionError('invalid startup succeeded')
            failure=json.loads(next(Path(tmp).glob('*.failed.json')).read_text())
            events=[json.loads(l) for l in next(Path(tmp).glob('*.events.jsonl')).read_text().splitlines()]
            assert len(owned)==1 and owned[0].returncode is not None and owned[0].returncode!=0
            assert any(e.get('pid')==owned[0].pid and e['event']=='gateway_started' for e in events)
            assert not failure['attempts'] and not failure['outcomes']
            result={'owned_pid':owned[0].pid,'returncode':owned[0].returncode,'failure':failure['failure'],'pid_journaled':True,'owned_processes_cleaned':True,'fresh_http_ingress':0,'fresh_upstream':0}
            (OUT/'startup-exit-green-result.json').write_text(json.dumps(result,indent=2)+'\n');print(json.dumps(result,indent=2))
    finally:
        asyncio.create_subprocess_exec=spawn
        for proc in owned:
            if proc.returncode is None: await b.abort_startup(proc)
asyncio.run(check())
