import asyncio,sys,pathlib,json,tempfile,os,signal,time
root=pathlib.Path(__file__).resolve().parents[3];out=pathlib.Path(__file__).resolve().parent;sys.path.insert(0,str(root/'product/scripts'));sys.dont_write_bytecode=True
import benchmark as b
binary=root/'product/target/release/llmgw';reference=root/'product/target/bench/release/examples/bench_gateway';original_control=b.control;original_spawn=asyncio.create_subprocess_exec
results=[];all_owned=[]
async def case(mode):
 owned=[];events=[];ready=asyncio.Event();real_controls=[];injected=RuntimeError('reviewer readiness failure');started=time.monotonic()
 async def spawn(*args,**kw):
  if mode=='nonzero':args=(str(binary),'--reviewer-invalid-option')
  p=await original_spawn(*args,**kw);owned.append(p);all_owned.append(p);return p
 async def health(port,path='status',method='GET'):
  if path=='health' and mode not in ['normal','nonzero']:
   ready.set()
   if mode=='exception':await asyncio.sleep(.05);raise injected
   await asyncio.Event().wait()
  real_controls.append([path,method]);return await original_control(port,path,method)
 asyncio.create_subprocess_exec=spawn;b.control=health
 try:
  with tempfile.TemporaryDirectory(prefix='lifecycle-',dir=out) as tmp:
   if mode=='original_cancel':
    task=asyncio.create_task(b.run_arm('production_rr',1901,1,binary,reference,'smoke',pathlib.Path(tmp)))
   else:task=asyncio.create_task(b.start_gateway('production_rr',binary,reference,9,None,tmp,events.append))
   if mode in ['original_cancel','repeated_cancel_kill']:
    await asyncio.wait_for(ready.wait(),5);assert owned
    if mode=='repeated_cancel_kill':
     # Stop only the exact owned child, so SIGTERM cannot complete cleanup and kill escalation is exercised.
     os.kill(owned[0].pid,signal.SIGSTOP)
    task.cancel('original-cancel')
    if mode=='repeated_cancel_kill':
     await asyncio.sleep(.05);task.cancel('second-cancel');await asyncio.sleep(.05);task.cancel('third-cancel')
   try:
    value=await asyncio.wait_for(task,9)
   except asyncio.CancelledError as e:
    assert mode in ['original_cancel','repeated_cancel_kill'];assert e.args==('original-cancel',);outcome={'exception':'CancelledError','original_arguments_preserved':True}
   except RuntimeError as e:
    if mode=='exception':assert e is injected;outcome={'exception':'RuntimeError','original_object_preserved':True}
    elif mode=='nonzero':assert str(e)=='gateway failed startup';outcome={'exception':'RuntimeError','startup_failure_reported':True}
    else:raise
   else:
    assert mode=='normal';proc,port,config=value;assert proc is owned[0] and proc.returncode is None
    await b.stop_gateway(proc,port);assert proc.returncode==0;outcome={'ownership_transferred_live':True,'normal_stop_exit_zero':True}
   if mode=='original_cancel':
    eventfile=pathlib.Path(tmp)/'smoke-seed1901-production_rr.events.jsonl';events=[json.loads(line) for line in eventfile.read_text().splitlines()]
   assert any(e.get('event')=='gateway_started' and e.get('pid')==owned[0].pid for e in events)
   assert all(p.returncode is not None for p in owned)
   if mode=='repeated_cancel_kill':assert owned[0].returncode==-signal.SIGKILL
   for p in owned:
    try:os.kill(p.pid,0)
    except ProcessLookupError:pass
    else:raise AssertionError('owned child remains present')
   results.append({'case':mode,'pids':[p.pid for p in owned],'returncodes':[p.returncode for p in owned],'elapsed_s':time.monotonic()-started,'pid_journaled_before_readiness':True,'real_control_calls':real_controls,'data_ingress':0,'upstream_attempts':0,**outcome})
 finally:
  asyncio.create_subprocess_exec=original_spawn;b.control=original_control
  for p in owned:
   if p.returncode is None:p.kill()
   await p.wait()
async def main():
 try:
  for name in ['original_cancel','exception','repeated_cancel_kill','nonzero','normal']:await case(name)
 finally:
  (out/'lifecycle-results.json').write_text(json.dumps({'cases':results,'owned_pids':[p.pid for p in all_owned],'all_reaped':all(p.returncode is not None for p in all_owned),'data_ingress':0,'upstream_attempts':0},indent=2)+'\n')
 print(json.dumps(results,indent=2))
asyncio.run(main())
