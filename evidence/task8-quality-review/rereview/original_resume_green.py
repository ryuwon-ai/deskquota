import asyncio,sys,pathlib,json,argparse,copy,tempfile,hashlib
root=pathlib.Path(__file__).resolve().parents[3]; sys.path.insert(0,str(root/'product/scripts'));sys.dont_write_bytecode=True
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
  try: await b.main(args)
  except ValueError as e:
   assert "unregistered" in str(e)
   evidence={"original_counterexample":"actual measured seed1 file copied to fresh seed999 output","result":"REJECTED","error":str(e),"copied_original_run":json.loads(completed.read_text())["id"],"manifest_created":output.exists(),"new_arms_launched":len(calls),"fresh_http_ingress":0,"fresh_upstream":0}
   assert not evidence["manifest_created"] and not calls
   (out/"original-resume-green-result.json").write_text(json.dumps(evidence,indent=2)+"\n");print(json.dumps(evidence,indent=2))
  else:raise AssertionError("original unregistered wrong-seed counterexample remains")

asyncio.run(probe())
