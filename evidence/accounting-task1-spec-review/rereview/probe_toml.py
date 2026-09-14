"""Original SPEC S1 mismatch trigger plus focused runtime-fingerprint cleanup."""
import asyncio, hashlib, json, os, sys, tempfile, tomllib
from pathlib import Path
from unittest.mock import patch
ROOT=Path(__file__).resolve().parents[3]
PRODUCT=ROOT/'product'
OUT=Path(__file__).resolve().parent
sys.path.insert(0,str(PRODUCT/'scripts'))
import benchmark as b
import benchmark_accounting as a
original=a.build_gateway_config
BINARY=PRODUCT/'target/native/release/llmgw'

def wrong_actual_toml(**kwargs):
    text, metadata=original(**kwargs)
    return text.replace('accounting = "actual"','accounting = "reserved"'), metadata

async def main():
    results=[]
    with tempfile.TemporaryDirectory(prefix='accounting-spec-toml-') as temp:
        events=[]
        with patch.object(a,'build_gateway_config',wrong_actual_toml), patch.object(b.asyncio,'create_subprocess_exec',side_effect=AssertionError('subprocess spawned')) as spawn:
            try:
                await b.start_gateway('production_actual',BINARY,None,9,None,temp,events.append)
            except ValueError as error:
                assert 'written TOML' in str(error)
                results.append({'name':'original_builder_actual_metadata_reserved_toml','rejected':True,'reason':str(error),'subprocess_calls':spawn.call_count,'actual_file_accounting':tomllib.loads((Path(temp)/'gateway.toml').read_text())['accounting'],'gateway_started_events':events})
            else:
                raise AssertionError('original TOML mismatch accepted')
        assert spawn.call_count==0
    results[-1]['temp_removed']=not Path(temp).exists()

    # New readiness check is part of the same S1 contract. Only returned control
    # fingerprint is corrupted; the real ordinary child remains our owned fixture.
    original_control=b.control
    events=[]
    seen={}
    async def wrong_fingerprint(port,path='status',method='GET'):
        status=await original_control(port,path,method)
        if path=='status':
            seen['observed_accounting']=status['admission']['accounting']
            seen['observed_fingerprint']=status['identity']['fingerprint']
            seen['upstream_attempts']=status['upstream_attempts']
            status['identity']['fingerprint']='0'*64
        return status
    with tempfile.TemporaryDirectory(prefix='accounting-spec-runtime-') as temp:
        with patch.object(b,'control',wrong_fingerprint):
            try:
                returned=await b.start_gateway('production_actual',BINARY,None,9,None,temp,events.append)
            except ValueError as error:
                assert 'runtime config fingerprint mismatch' in str(error)
                pid=next(event['pid'] for event in events if event['event']=='gateway_started')
                try:
                    os.kill(pid,0)
                except ProcessLookupError:
                    reaped=True
                else:
                    reaped=False
                assert reaped
                raw=(Path(temp)/'gateway.toml').read_bytes()
                assert seen['observed_fingerprint']==hashlib.sha256(raw).hexdigest()
                results.append({'name':'runtime_fingerprint_mismatch','rejected':True,'reason':str(error),'pid':pid,'child_reaped':reaped,'actual_file_accounting':tomllib.loads(raw.decode())['accounting'],**seen})
            else:
                await b.stop_gateway(returned[0],returned[1])
                raise AssertionError('runtime fingerprint mismatch accepted')
    results[-1]['temp_removed']=not Path(temp).exists()
    assert all(result['temp_removed'] for result in results)
    data={'results':results,'execution_finished':True,'product_sources_edited':False}
    (OUT/'probe-toml.json').write_text(json.dumps(data,indent=2)+'\n')
    print(json.dumps(data,indent=2))
asyncio.run(main())
