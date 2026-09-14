#!/usr/bin/env python3
import importlib.util
from pathlib import Path
import tempfile
import unittest


PRODUCT = Path(__file__).resolve().parents[1]
SCRIPT = PRODUCT / "scripts" / "measure_native.py"


def load_module():
    spec = importlib.util.spec_from_file_location("measure_native", SCRIPT)
    assert spec and spec.loader
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


class MeasureNativeTests(unittest.TestCase):
    def test_environment_is_fixed_allowlist(self):
        module = load_module()
        with tempfile.TemporaryDirectory(prefix="llmgw-measure-env-") as raw:
            env = module.safe_environment(Path(raw), {"SECRET": "real", "PATH": "/unsafe"})
        self.assertEqual(
            set(env), {"HOME", "USERPROFILE", "XDG_CONFIG_HOME", "TMPDIR", "LC_ALL", "PATH"}
        )
        self.assertNotIn("SECRET", env)
        self.assertEqual(env["PATH"], "/usr/bin:/bin:/usr/sbin:/sbin")

    def test_macos_time_parser_labels_command_tree_bytes(self):
        module = load_module()
        output = "     12345678  maximum resident set size\n"
        self.assertEqual(module.parse_macos_peak(output), 12345678)
        self.assertEqual(module.macos_peak_line(output), "     12345678  maximum resident set size")
        with self.assertRaises(ValueError):
            module.parse_macos_peak("missing")

    def test_saved_config_summary_requires_loopback_and_no_auth(self):
        module = load_module()
        summary = module.saved_upstream_summary(
            '[upstream]\napi_base = "http://127.0.0.1:8080/v1"\n[upstream.auth]\nmode = "none"\n'
        )
        self.assertEqual(summary, {"api_base": "http://127.0.0.1:8080/v1", "auth": "none"})
        with self.assertRaises(ValueError):
            module.saved_upstream_summary(
                '[upstream]\napi_base = "https://example.com/v1"\n[upstream.auth]\nmode = "none"\n'
            )


if __name__ == "__main__":
    unittest.main()
