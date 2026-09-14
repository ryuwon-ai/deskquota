#!/usr/bin/env python3
"""Run exact held-binary client and native probes with exclusive outputs."""

from __future__ import annotations

from datetime import datetime, timezone
import hashlib
import json
import os
from pathlib import Path
import subprocess


RESEARCH = Path(__file__).resolve().parents[2]
PRODUCT = RESEARCH / "product"
OUT = Path(__file__).resolve().parent
BINARY = PRODUCT / "artifacts/native-task6/final-v2/llmgw-macos-arm64"
EXPECTED = "cf7c436c442c426a6e9a1d485a5261fe5e5a05c8b131b130e679e00eec2c646b"


def sha256(path: Path) -> str:
    return hashlib.sha256(path.read_bytes()).hexdigest()


def main() -> int:
    if sha256(BINARY) != EXPECTED:
        raise SystemExit("held binary identity mismatch")
    env = os.environ.copy()
    env["PYTHONDONTWRITEBYTECODE"] = "1"
    specifications = [
        ("pi", ["python3", "scripts/verify_clients.py", "--binary", str(BINARY), "--client", "pi", "--output", str(OUT / "runtime-pi.json")]),
        ("claude", ["python3", "scripts/verify_clients.py", "--binary", str(BINARY), "--client", "claude", "--output", str(OUT / "runtime-claude.json")]),
        ("codex", ["python3", "scripts/verify_clients.py", "--binary", str(BINARY), "--client", "codex", "--output", str(OUT / "runtime-codex.json")]),
        ("native", ["python3", "scripts/measure_native.py", "--binary", str(BINARY), "--cycles", "3", "--output", str(OUT / "runtime-native.json")]),
    ]
    records = []
    for name, command in specifications:
        completed = subprocess.run(
            command,
            cwd=PRODUCT,
            env=env,
            text=True,
            capture_output=True,
            timeout=120,
            check=False,
        )
        output_path = OUT / f"runtime-{name}.json"
        parsed = json.loads(output_path.read_text(encoding="utf-8")) if output_path.is_file() else None
        records.append({
            "name": name,
            "command": command,
            "exit_code": completed.returncode,
            "stdout": completed.stdout,
            "stderr": completed.stderr,
            "output": output_path.relative_to(RESEARCH).as_posix(),
            "output_sha256": sha256(output_path) if output_path.is_file() else None,
            "reported_passed": parsed.get("passed") if isinstance(parsed, dict) else None,
            "binary_sha256": parsed.get("binary_sha256") if isinstance(parsed, dict) else None,
        })
        if completed.returncode != 0 or not isinstance(parsed, dict) or not parsed.get("passed"):
            break
    payload = {
        "at": datetime.now(timezone.utc).isoformat(),
        "binary_sha256": EXPECTED,
        "records": records,
        "passed": len(records) == len(specifications) and all(
            record["exit_code"] == 0
            and record["reported_passed"] is True
            and record["binary_sha256"] == EXPECTED
            for record in records
        ),
    }
    with (OUT / "runtime-commands.json").open("x", encoding="utf-8") as stream:
        json.dump(payload, stream, indent=2)
        stream.write("\n")
    print(json.dumps({"passed": payload["passed"], "records": len(records)}))
    return 0 if payload["passed"] else 1


if __name__ == "__main__":
    raise SystemExit(main())
