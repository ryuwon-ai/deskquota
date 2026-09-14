"""Owned loopback-only identity/stdio check; no credential values in output."""
from pathlib import Path
import hashlib,json,os,subprocess,tempfile
ROOT=Path(__file__).resolve().parents[2]
binary=ROOT/'target/native/release/llmgw'
result={'binary_sha256':hashlib.sha256(binary.read_bytes()).hexdigest(),'platform':os.uname().sysname}
with tempfile.TemporaryDirectory(prefix='llmgw-native-session-') as temporary:
    config=Path(temporary)/'fixture.toml'
    text=(ROOT/'examples/fixture.toml').read_text().replace('127.0.0.1:4141','127.0.0.1:0').replace('127.0.0.1:18080','127.0.0.1:9')
    config.write_text(text)
    def call(*args):return subprocess.run([str(binary),'--config',str(config),*args],capture_output=True,timeout=17)
    try:
        on=call('on');assert on.returncode==0
        status=call('status','--json');assert status.returncode==0
        data=json.loads(status.stdout);pid=data['identity']['pid']
        result.update(worker_pid=pid,session_id=os.getsid(pid),process_group=os.getpgid(pid),parent_session=os.getsid(0))
        assert os.getsid(pid)==pid==os.getpgid(pid)
        assert os.getsid(0)!=pid
        files=subprocess.run(['/usr/sbin/lsof','-a','-p',str(pid),'-d','0,1,2','-Fn'],capture_output=True,text=True,timeout=5)
        names=[line[1:] for line in files.stdout.splitlines() if line.startswith('n')]
        assert files.returncode==0 and names==['/dev/null']*3
        result['stdio_dev_null_count']=len(names)
        state=Path(data['state_directory']);record=(state/'runtime.json').read_bytes()
        for name in ('data-token','control-token'):
            token=(state/name).read_bytes()
            assert token not in record and token not in on.stdout and token not in status.stdout
        assert (state/'worker.log').stat().st_size<=128
        result['credentials_absent_from_output_and_record']=True
        stopped=call('off');assert stopped.returncode==0
        result['off_exit_code']=stopped.returncode
        result['post_off_state']=json.loads(call('status','--json').stdout)['state']
        assert result['post_off_state']=='stopped'
        result['worker_log_bytes_after_stop']=(state/'worker.log').stat().st_size
        result['passed']=True
    finally:
        cleanup=call('off');result['cleanup_exit_code']=cleanup.returncode
        assert cleanup.returncode==0
result['temporary_directory_removed']=not Path(temporary).exists()
(ROOT/'artifacts/native-task1/unix-session-final.json').write_text(json.dumps(result,indent=2)+'\n')
print(json.dumps(result))
