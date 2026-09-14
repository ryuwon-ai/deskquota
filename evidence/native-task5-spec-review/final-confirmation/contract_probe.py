import pathlib, types, unittest
from unittest import mock
OUT = pathlib.Path(__file__).resolve().parent
SOURCE = OUT.parents[2] / 'product/scripts/verify_native.py'
class FinalSpec(unittest.TestCase):
    def test_preview_cannot_apply_or_assert_login(self):
        module = types.ModuleType('spec_final_driver')
        module.__file__ = str(SOURCE)
        exec(compile(SOURCE.read_bytes(), str(SOURCE), 'exec'), module.__dict__)
        args = types.SimpleNamespace(stage='install', execute=False, confirm_temporary_os_account=True)
        with mock.patch.object(module.pathlib.Path, 'home', return_value=OUT/'synthetic-home'), mock.patch.object(module, 'terminal_child_env', return_value={}), mock.patch.object(module, 'status', return_value={'state':'stopped','autostart':{}}), mock.patch.object(module, 'apply_autostart', side_effect=AssertionError('preview cannot mutate')) as apply, mock.patch.object(module.subprocess, 'run', side_effect=AssertionError('no subprocess permitted')):
            record, code = module.manual_stage(args, OUT/'synthetic-binary', OUT/'synthetic-config')
        self.assertEqual(code, 2)
        self.assertFalse(record['actual_login_verified'])
        self.assertEqual(record['result'], 'preview_only_explicit_execution_and_temporary_account_confirmation_required')
        apply.assert_not_called()
    def test_install_remains_incomplete_for_login_acceptance(self):
        module = types.ModuleType('spec_final_driver')
        module.__file__ = str(SOURCE)
        exec(compile(SOURCE.read_bytes(), str(SOURCE), 'exec'), module.__dict__)
        args = types.SimpleNamespace(stage='install', execute=True, confirm_temporary_os_account=True)
        with mock.patch.object(module.pathlib.Path, 'home', return_value=OUT/'synthetic-home'), mock.patch.object(module, 'terminal_child_env', return_value={}), mock.patch.object(module, 'status', return_value={'state':'stopped','autostart':{}}), mock.patch.object(module, 'apply_autostart', return_value=('synthetic reviewed preview', None)) as apply, mock.patch.object(module.subprocess, 'run', side_effect=AssertionError('no subprocess permitted')):
            record, code = module.manual_stage(args, OUT/'synthetic-binary', OUT/'synthetic-config')
        self.assertEqual(code, 0)
        self.assertFalse(record['actual_login_verified'])
        self.assertEqual(record['result'], 'installed_for_next_login')
        apply.assert_called_once_with(OUT/'synthetic-binary', OUT/'synthetic-config', OUT/'synthetic-home', 'on', {})
if __name__ == '__main__': unittest.main(verbosity=2)
