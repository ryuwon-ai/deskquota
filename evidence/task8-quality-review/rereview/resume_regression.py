import pathlib,sys,asyncio,json,argparse,tempfile,copy,hashlib
root=pathlib.Path(__file__).resolve().parents[3];out=pathlib.Path(__file__).resolve().parent;sys.path.insert(0,str(root/'product/scripts'));sys.dont_write_bytecode=True
import benchmark as b
results=[];launches=[]
async def forbidden(*args):launches.append(str(args));raise AssertionError('new arm attempted before validation')
b.ARMS=('direct',);b.run_arm=forbidden
async def case(name,change=None,fresh=False,seeds=(1,)):
 with tempfile.TemporaryDirectory(prefix='resume-',dir=out) as tmp:
  p=pathlib.Path(tmp);output=p/'case.json';directory=p/'case-runs';directory.mkdir();binary=(root/'product/target/release/llmgw').resolve();reference=(root/'product/target/bench/release/examples/bench_gateway').resolve();rid='no_wait-seed1-direct';completed=directory/(rid+'.json')
  run={'id':rid,'phase':'no_wait','seed':1,'arm':'direct','submitted':[],'outcomes':[],'attempts':[],'summary':{}}
  b.write_json(completed,run)
  manifest={'schema':1,'status':'in_progress','identity':b.identities(binary,reference),'seeds':list(seeds),'windows':1,'schedule':[['no_wait',seed,'direct'] for seed in seeds],'host':{},'runs':[{'id':rid,'path':str(completed),'sha256':b.digest(completed),'summary':{}}]}
  if change:change(manifest,run,completed,directory)
  if not fresh:b.write_json(output,manifest)
  snap={str(f.relative_to(p)):f.read_bytes() for f in p.rglob('*') if f.is_file()}
  args=argparse.Namespace(binary=binary,reference_binary=reference,seeds=','.join(map(str,seeds)),windows=1,output=output,smoke=False,phase='no_wait')
  try:await b.main(args)
  except ValueError as e:
   assert name!='valid';assert snap=={str(f.relative_to(p)):f.read_bytes() for f in p.rglob('*') if f.is_file()};results.append({'case':name,'result':'REJECTED before dispatch, inputs unchanged','error':str(e)})
  else:
   assert name=='valid';assert json.loads(output.read_text())['status']=='completed_no_wait_only';results.append({'case':name,'result':'PASS registered reuse, no dispatch'})
  assert not launches

def content(key,val):
 def edit(m,r,c,d):r[key]=val;b.write_json(c,r);m['runs'][0]['sha256']=b.digest(c)
 return edit
async def main():
 await case('valid')
 # Original wrong-seed file in fresh output, with no manifest.
 await case('original fresh orphan',lambda m,r,c,d:c.rename(d/'no_wait-seed999-direct.json'),fresh=True,seeds=(999,))
 # Original later orphan must fail before first pending arm (seed998) starts.
 await case('original later orphan',lambda m,r,c,d:(m.update(runs=[]),c.rename(d/'no_wait-seed999-direct.json')),seeds=(998,999))
 for suffix in ['events.jsonl','failed.json','json.tmp']:
  await case('later incomplete '+suffix,lambda m,r,c,d,suffix=suffix:(m.update(runs=[]),c.unlink(),(d/('no_wait-seed999-direct.'+suffix)).write_text('{}')),seeds=(998,999))
 for key,val in [('id','no_wait-seed2-direct'),('phase','quota'),('seed',2),('arm','production_rr')]:await case('content '+key,content(key,val))
 await case('stale hash',lambda m,r,c,d:c.write_text('{}'))
 await case('missing registered',lambda m,r,c,d:c.unlink())
 await case('duplicate registered',lambda m,r,c,d:m['runs'].append(copy.deepcopy(m['runs'][0])))
 await case('redirected path',lambda m,r,c,d:m['runs'][0].update(path=str(d/'other.json')))
 await case('old source identity',lambda m,r,c,d:m['identity']['sources'].update({'scripts/benchmark.py':'stale'}))
 await case('nonterminal outcome',lambda m,r,c,d:(r.update(submitted=[{'id':'x'}],outcomes=[{'id':'x','outcome':'pending'}]),b.write_json(c,r),m['runs'][0].update(sha256=b.digest(c))))
 (out/'resume-results.json').write_text(json.dumps({'cases':results,'new_arms_launched':0,'fresh_data_ingress':0,'fresh_upstream':0},indent=2)+'\n');print(json.dumps(results,indent=2))
asyncio.run(main())
