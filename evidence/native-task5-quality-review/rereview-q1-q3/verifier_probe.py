import sys
sys.dont_write_bytecode = True
import pathlib, types, unittest, tempfile, subprocess
from unittest import mock
OUT=pathlib.Path(__file__).resolve().parent
SOURCE=OUT.parents[2]/'product/scripts/verify_native.py'
def load():
    module=types.ModuleType('review_verify_native')
    module.__file__=str(SOURCE)
    exec(compile(SOURCE.read_bytes(),str(SOURCE),'exec'),module.__dict__)
    return module
class VerifierQuality(unittest.TestCase):
    def test_failed_login_observations_exit_nonzero(self):
        for stage,state,registration in [('observe-login-on','stopped','registered'),('observe-login-off','running','disabled')]:
            with self.subTest(stage=stage):
                module=load()
                args=types.SimpleNamespace(stage=stage,execute=True,confirm_temporary_os_account=True,confirm_human_login_observed=True,human_login_at='2026-09-13T15:00:00+09:00')
                observed={'state':state,'autostart':{'registration':registration}}
                env={'HOME':str(OUT/'synthetic-home'),'USERPROFILE':str(OUT/'synthetic-home')}
                with mock.patch.object(module,'status',return_value=observed),mock.patch.object(module,'terminal_child_env',return_value=env),mock.patch.object(module.pathlib.Path,'home',return_value=OUT/'synthetic-home'),mock.patch.object(module.subprocess,'run',side_effect=AssertionError('No subprocess permitted')):
                    record,code=module.manual_stage(args,OUT/'synthetic-binary',OUT/'synthetic-config')
                print(f'{stage}: actual_login_verified={record["actual_login_verified"]}, exit_code={code}',flush=True)
                self.assertFalse(record['actual_login_verified'])
                self.assertNotEqual(code,0,'Failed attested login observation is returned as successful exit 0')
    def test_successful_login_observations_remain_success(self):
        for stage,state,registration in [('observe-login-on','running','registered'),('observe-login-off','stopped','disabled')]:
            with self.subTest(stage=stage):
                module=load()
                args=types.SimpleNamespace(stage=stage,execute=True,confirm_temporary_os_account=True,confirm_human_login_observed=True,human_login_at='2026-09-13T15:00:00+09:00')
                observed={'state':state,'autostart':{'registration':registration}}
                with mock.patch.object(module,'status',return_value=observed),mock.patch.object(module,'terminal_child_env',return_value={}),mock.patch.object(module.pathlib.Path,'home',return_value=OUT/'synthetic-home'),mock.patch.object(module.subprocess,'run',side_effect=AssertionError('No subprocess permitted')):
                    record,code=module.manual_stage(args,OUT/'synthetic-binary',OUT/'synthetic-config')
                self.assertTrue(record['actual_login_verified']);self.assertEqual(code,0)
    def test_cleanup_and_recovery_boundaries(self):
        for mode in ['cleanup_ok','off_failed','status_running','start_raised']:
            with self.subTest(mode=mode):
                module=load();events=[];homes=[];calls=0;configs=[]
                real_mkdtemp=tempfile.mkdtemp
                def owned_temp(**kwargs):
                    home=pathlib.Path(real_mkdtemp(prefix='owned-mocked-driver-',dir=OUT));homes.append(home);return str(home)
                def fake_status(binary,config,home):
                    nonlocal calls
                    calls+=1;events.append('status');configs.append(config)
                    if calls==3 and mode!='start_raised':
                        raise RuntimeError('synthetic original status failure after successful manual on')
                    if calls>2:
                        return {'state':'running' if mode=='status_running' else 'stopped'}
                    return {'state':'stopped','autostart':{'target':str(home/'fixture.plist')}}
                def fake_autostart(binary,config,home,action,*args,**kwargs):
                    events.append('autostart '+action);(home/'fixture.plist').write_text('synthetic definition')
                    return 'preview hash: synthetic',subprocess.CompletedProcess([],0,'','')
                def fake_cli(binary,config,home,*args,**kwargs):
                    events.append('cli '+' '.join(args));configs.append(config)
                    if args==('on',) and mode=='start_raised':raise RuntimeError('synthetic original start failure')
                    return subprocess.CompletedProcess([],1 if args==('off',) and mode=='off_failed' else 0,'','')
                try:
                    with mock.patch.object(module.sys,'platform','darwin'),mock.patch.object(module,'cargo_template_tests',return_value={'passed':True}),mock.patch.object(module.tempfile,'mkdtemp',side_effect=owned_temp),mock.patch.object(module,'free_port',return_value=12345),mock.patch.object(module,'status',side_effect=fake_status),mock.patch.object(module,'apply_autostart',side_effect=fake_autostart),mock.patch.object(module,'cli',side_effect=fake_cli),mock.patch.object(module.shutil,'which',return_value=None),mock.patch.object(module.subprocess,'run',return_value=subprocess.CompletedProcess([],0,'synthetic plutil OK','')):
                        with self.assertRaisesRegex(RuntimeError,'synthetic original') as failure:
                            module.templates_only(OUT/'synthetic-binary',OUT/'unused-target')
                    retained=homes[0].exists();message=str(failure.exception)
                    print(f'{mode}: calls={events}, recovery_retained={retained}, exception={message}; no real worker or subprocess',flush=True)
                    self.assertIn('cli off',events);self.assertEqual(len(set(configs)),1)
                    if mode in ['off_failed','status_running']:
                        self.assertTrue(retained);self.assertTrue(configs[0].is_file());self.assertIn('authenticated cleanup failed',message);self.assertIn(str(homes[0]),message)
                    else:
                        self.assertFalse(retained);self.assertEqual(events[-2:],['cli off','status'])
                finally:
                    for home in homes:
                        if home.exists():module.shutil.rmtree(home)
if __name__=='__main__':unittest.main(verbosity=2)
