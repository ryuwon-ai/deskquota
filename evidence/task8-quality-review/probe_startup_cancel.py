import asyncio,sys,pathlib,json,tempfile,os
root=pathlib.Path(__file__).resolve().parents[2];sys.path.insert(0,str(root/'product/scripts'));sys.dont_write_bytecode=True
import benchmark as b
out=pathlib.Path(__file__).resolve().parent
async def probe():
 started=asyncio.Event();owned=[];original_spawn=asyncio.create_subprocess_exec;original_control=b.control
 async def record_spawn(*args,**kw):
  proc=await original_spawn(*args,**kw)
  if str(args[0])==str(root/'product/target/release/llmgw'):owned.append(proc)
  return proc
 async def delayed_health(port,path='status',method='GET'):
  if path=='health':started.set();await asyncio.Event().wait()
  return await original_control(port,path,method)
 asyncio.create_subprocess_exec=record_spawn;b.control=delayed_health
 try:
  with tempfile.TemporaryDirectory(prefix='startup-cancel-',dir=out) as tmp:
   task=asyncio.create_task(b.run_arm('production_rr',901,1,root/'product/target/release/llmgw',root/'product/target/bench/release/examples/bench_gateway','smoke',pathlib.Path(tmp)))
   await asyncio.wait_for(started.wait(),5)
   await asyncio.sleep(.1)
   task.cancel()
   try:await task
   except asyncio.CancelledError:pass
   await asyncio.sleep(.1)
   alive=[p.pid for p in owned if p.returncode is None]
   fail=json.loads((pathlib.Path(tmp)/'smoke-seed901-production_rr.failed.json').read_text())
   journal=(pathlib.Path(tmp)/'smoke-seed901-production_rr.events.jsonl').read_text()
   result={'injected_fault':'cancel run_arm after child spawn before start_gateway returns readiness','owned_gateway_pids':[p.pid for p in owned],'alive_after_run_arm_finally':alive,'failure':fail['failure'],'journal_contains_gateway_started':any(json.loads(l).get('event')=='gateway_started' for l in journal.splitlines()),'fresh_http_ingress':0,'fresh_upstream':0}
 finally:
  asyncio.create_subprocess_exec=original_spawn;b.control=original_control
  for proc in owned:
   if proc.returncode is None:
    proc.terminate()
    try:await asyncio.wait_for(proc.wait(),15)
    except TimeoutError:proc.kill();await proc.wait()
  result['reviewer_cleanup_returncodes']={str(p.pid):p.returncode for p in owned}
  result['owned_pids_absent']=all(p.returncode is not None for p in owned)
  (out/'startup-cancel-result.json').write_text(json.dumps(result,indent=2)+'\n');print(json.dumps(result,indent=2))
asyncio.run(probe())
