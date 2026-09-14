#!/usr/bin/env python3
import importlib.util
import sys
import unittest
from pathlib import Path


SCRIPTS = Path(__file__).resolve().parents[1] / "scripts"
sys.path.insert(0, str(SCRIPTS))
SPEC = importlib.util.spec_from_file_location("verify_clients", SCRIPTS / "verify_clients.py")
assert SPEC and SPEC.loader
verify_clients = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(verify_clients)


class ToolResultBoundaryTests(unittest.TestCase):
    def test_codex_rejects_markers_in_unrelated_text(self) -> None:
        payload = {
            "input": [
                {
                    "type": "message",
                    "content": (
                        f"{verify_clients.CODEX_TOOL_CALL_ID} "
                        f"{verify_clients.CODEX_TOOL_CONTENT} Process exited with code 0"
                    ),
                }
            ]
        }
        self.assertFalse(verify_clients.codex_tool_result_matches(payload))

    def test_claude_rejects_markers_in_unrelated_text(self) -> None:
        payload = {
            "messages": [
                {
                    "role": "user",
                    "content": (
                        "toolu_task4_read " + verify_clients.CODEX_TOOL_CONTENT
                    ),
                }
            ]
        }
        self.assertFalse(verify_clients.claude_tool_result_matches(payload))


if __name__ == "__main__":
    unittest.main()
