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
  before=completed.read_bytes()
  try: await b.main(args)
  except ValueError as error: assert 'unregistered' in str(error); reason=str(error)
  else: raise AssertionError('orphan accepted')
  assert not calls and not output.exists() and completed.read_bytes()==before
  evidence={'requested_seed':999,'copied_original_run':json.loads(before)['id'],'new_arms_launched':len(calls),'orphan_rejected':True,'reason':reason,'input_preserved':True,'manifest_created':False,'fresh_http_ingress':0,'fresh_upstream':0}
  (out/'resume-orphan-green-result.json').write_text(json.dumps(evidence,indent=2)+'\n');print(json.dumps(evidence,indent=2))

asyncio.run(probe())
