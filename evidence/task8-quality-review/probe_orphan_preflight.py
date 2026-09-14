import asyncio,sys,pathlib,json,argparse,tempfile
root=pathlib.Path(__file__).resolve().parents[2];sys.path.insert(0,str(root/'product/scripts'));sys.dont_write_bytecode=True
import benchmark as b
out=pathlib.Path(__file__).resolve().parent
async def probe():
 calls=[]
 async def forbid(*args):calls.append({'arm':args[0],'seed':args[1]});raise RuntimeError('reviewer blocked new arm')
 b.ARMS=('direct',);b.run_arm=forbid
 with tempfile.TemporaryDirectory(prefix='orphan-preflight-',dir=out) as tmp:
  p=pathlib.Path(tmp);output=p/'resume.json';directory=p/'resume-runs';directory.mkdir();orphan=directory/'no_wait-seed999-direct.json';orphan.write_text('{}')
  binary=(root/'product/target/release/llmgw').resolve();ref=(root/'product/target/bench/release/examples/bench_gateway').resolve()
  args=argparse.Namespace(binary=binary,reference_binary=ref,seeds='998,999',windows=1,output=output,smoke=False,phase='no_wait')
  b.write_json(output,{'schema':1,'status':'in_progress','identity':b.identities(binary,ref),'seeds':[998,999],'windows':1,'schedule':[['no_wait',998,'direct'],['no_wait',999,'direct']],'host':{},'runs':[]})
  try:await b.main(args)
  except RuntimeError as e:assert str(e)=='reviewer blocked new arm'
  result={'existing_manifest':True,'unregistered_later_orphan':orphan.name,'launches_attempted_before_orphan_detection':calls,'actual_launched_arms':0,'fresh_http_ingress':0,'fresh_upstream':0}
  assert calls==[{'arm':'direct','seed':998}];(out/'orphan-preflight-result.json').write_text(json.dumps(result,indent=2)+'\n');print(json.dumps(result,indent=2))
asyncio.run(probe())
