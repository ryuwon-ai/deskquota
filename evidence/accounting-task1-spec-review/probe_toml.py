"""Inject the specifically forbidden metadata/TOML disagreement at config creation."""
import asyncio,hashlib,json,sys,tempfile,tomllib,os
from pathlib import Path
from unittest.mock import patch
ROOT=Path(__file__).resolve().parents[2]; PRODUCT=ROOT/'product'; OUT=Path(__file__).resolve().parent
sys.path.insert(0,str(PRODUCT/'scripts'))
import benchmark as b
import benchmark_accounting as a
original=a.build_gateway_config
async def main():
 events=[]; proc=None; port=None; result={}
 def wrong_actual_toml(**kwargs):
  text,metadata=original(**kwargs)
  return text.replace('accounting = "actual"','accounting = "reserved"'),metadata
 with tempfile.TemporaryDirectory(prefix='accounting-spec-toml-') as temp:
  try:
   with patch.object(a,'build_gateway_config',wrong_actual_toml):
    proc,port,metadata=await b.start_gateway('production_actual',PRODUCT/'target/native/release/llmgw',None,9,None,temp,events.append)
   actual_bytes=(Path(temp)/'gateway.toml').read_bytes(); actual_snapshot=tomllib.loads(actual_bytes.decode())
   status=await b.control(port)
   result={'injection':'builder returns reserved TOML with actual metadata; no product source edit', 'real_gateway_launched':True,'pid':proc.pid,'recorded_accounting':metadata['accounting'],'snapshot_accounting':metadata['snapshot']['accounting'],'actual_file_accounting':actual_snapshot['accounting'],'recorded_raw_toml_sha256':metadata['raw_toml_sha256'],'actual_raw_toml_sha256':hashlib.sha256(actual_bytes).hexdigest(),'metadata_validation_accepted':bool(a.validate_config_provenance(metadata,expected_arm='production_actual',expected_quota=None)),'gateway_health':{key:status[key] for key in ('requests','upstream_attempts')},'actual_status_accounting':status['admission']['accounting']}
  finally:
   if proc is not None: await b.stop_gateway(proc,port)
  result['child_exit_code']=proc.returncode
  try:os.kill(proc.pid,0);result['child_reaped']=False
  except ProcessLookupError:result['child_reaped']=True
 result['temp_removed']=not Path(temp).exists(); result['execution_finished']=True
 (OUT/'probe-toml.json').write_text(json.dumps(result,indent=2)+'\n');print(json.dumps(result,indent=2))
asyncio.run(main())
