import asyncio,sys,pathlib,json,argparse,copy,tempfile,hashlib
root=pathlib.Path('/Users/ryuwon/Desktop/ryuwon-project/llm-gateway-research'); sys.path.insert(0,str(root/'product/scripts'));sys.dont_write_bytecode=True
import benchmark as b
out=pathlib.Path(__file__).resolve().parent
async def probe():
 prior=json.loads((root/'product/artifacts/pilot.json').read_text());entry=next(e for e in prior['runs'] if e['id']=='no_wait-seed1-direct');source=pathlib.Path(entry['path']);source=source if source.is_absolute() else root/'product'/source
 calls=[]
 async def forbidden(*args):calls.append(args);raise AssertionError('new arm forbidden')
 b.ARMS=('direct',);b.run_arm=forbidden
 with tempfile.TemporaryDirectory(prefix='resume-orphan-',dir=out) as tmp:
  p=pathlib.Path(tmp);output=p/'orphan.json';directory=p/'orphan-runs';directory.mkdir();completed=directory/'no_wait-seed999-direct.json';completed.write_bytes(source.read_bytes())
  args=argparse.Namespace(binary=root/'product/target/release/llmgw',reference_binary=root/'product/target/bench/release/examples/bench_gateway',seeds='999',windows=1,output=output,smoke=False,phase='no_wait')
  await b.main(args)
  result=json.loads(output.read_text());reused=json.loads(completed.read_text())
  evidence={'requested_seed':999,'requested_windows':1,'input_manifest_existed':False,'copied_original_run':reused['id'],'copied_seed':reused['seed'],'source_identity_matches_current':prior['identity']==result['identity'],'new_arms_launched':len(calls),'result_status':result['status'],'recorded_run_id':result['runs'][0]['id'],'actual_run_id':reused['id'],'actual_run_sha256':hashlib.sha256(completed.read_bytes()).hexdigest(),'fresh_http_ingress':0,'fresh_upstream':0}
  (out/'resume-orphan-result.json').write_text(json.dumps(evidence,indent=2)+'\n');print(json.dumps(evidence,indent=2));assert result['status']=='completed_no_wait_only' and not calls
asyncio.run(probe())
