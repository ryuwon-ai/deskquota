"""Native Pi fixture checks; no hosted API calls or real credentials."""
import argparse
import json
from pathlib import Path
import tempfile
import unittest
from unittest.mock import patch

import nvidia_pi as probe


class Checks(unittest.TestCase):
    def args(self, output):
        return argparse.Namespace(pi=None, model=None, gateway_base=None, gateway_first=False,
                                  live=False, self_check=False, gateway_retries_disabled=False,
                                  disable_template_thinking=False, key_env="PROBE_KEY", output=output)

    def test_dry_run_is_exclusive_and_live_requires_bounded_configuration(self):
        with tempfile.TemporaryDirectory() as temp:
            output = Path(temp) / "dry.json"
            args = self.args(output)
            with patch.object(probe, "run_arm", side_effect=AssertionError("dry-run executed Pi")), \
                 patch.object(probe, "self_check", side_effect=AssertionError("dry-run made loopback calls")):
                report = probe.execute(args)
            self.assertEqual(report["status"], "dry_run")
            self.assertEqual(report["max_client_requests"], 6)
            original = output.read_bytes()
            with self.assertRaises(FileExistsError):
                probe.execute(args)
            self.assertEqual(output.read_bytes(), original)
            args.live, args.output = True, Path(temp) / "live.json"
            with self.assertRaises(ValueError):
                probe.execute(args)
            self.assertFalse(args.output.exists())

    def test_installed_pi_guard_and_genuine_native_tool_loop(self):
        pi, _, _ = probe.native.resolve_pi(None)
        report = probe.self_check(pi)
        self.assertTrue(report["passed"], report)
        self.assertEqual(report["real_api_requests"], 0)
        self.assertEqual({c["case"]: c["http_requests"] for c in report["cases"]},
                         {"repair": 3, "path_escape": 1, "request_cap": 3, "guard_missing": 0, "http_429": 1})
        raw = json.dumps(report)
        for secret in ("synthetic-key", "PRIVATE_OFFLINE_CANARY", probe.TASK, probe.BROKEN_CODE, probe.FIXED_CODE):
            self.assertNotIn(secret, raw)


if __name__ == "__main__":
    unittest.main()
