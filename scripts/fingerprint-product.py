#!/usr/bin/env python3
"""Capture a stable product source fingerprint and match optional binary probes."""

import argparse
from datetime import datetime, timezone
import hashlib
import json
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]
PRODUCT = ROOT / "product"


def digest(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def source_files():
    top_level = (".gitignore", "Cargo.toml", "Cargo.lock", "rust-toolchain.toml",
                 "README.md", "AGENTS.md", "LICENSE", "LICENSE.md")
    paths = [PRODUCT / name for name in top_level if (PRODUCT / name).is_file()]
    for name in ("src", "tests", "examples", "fixtures", "docs", "scripts"):
        paths.extend(path for path in (PRODUCT / name).rglob("*")
                     if path.is_file() and "__pycache__" not in path.parts)
    return sorted(paths)


def snapshot():
    return [{"path": str(path.relative_to(ROOT)), "bytes": path.stat().st_size,
             "sha256": digest(path)} for path in source_files()]


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--phase", required=True)
    parser.add_argument("--output", required=True, type=Path)
    parser.add_argument("--binary", type=Path, default=PRODUCT / "target/debug/llmgw")
    parser.add_argument("--probe", action="append", default=[], type=Path)
    args = parser.parse_args()
    binary = args.binary.resolve(strict=True)
    binary_hash = digest(binary)
    files = snapshot()
    checks = []
    for probe in args.probe:
        path = probe.resolve(strict=True)
        data = json.loads(path.read_text())
        checks.append({"artifact": str(path.relative_to(ROOT)),
                       "passed": data.get("passed") is True,
                       "binary_sha256": data.get("binary_sha256"),
                       "matches_binary": data.get("binary_sha256") == binary_hash})
    stable = files == snapshot() and digest(binary) == binary_hash
    passed = stable and all(check["passed"] and check["matches_binary"] for check in checks)
    result = {"phase": args.phase, "captured_at": datetime.now(timezone.utc).isoformat(),
              "files": files, "binary": str(binary.relative_to(ROOT)),
              "binary_sha256": binary_hash, "stable_during_capture": stable,
              "probe_checks": checks, "passed": passed,
              "limitations": ["Filesystem fingerprint, not Git commit or release provenance",
                              "Does not prove source was used to produce the binary; retain build/test evidence separately",
                              "Excludes runtime state, build directories and artifacts; no performance or completion verdict"]}
    args.output.parent.mkdir(parents=True, exist_ok=True)
    args.output.write_text(json.dumps(result, indent=2) + "\n")
    print(json.dumps({"passed": passed, "source_files": len(files),
                      "stable_during_capture": stable, "probe_checks": checks}))
    return 0 if passed else 1


if __name__ == "__main__":
    raise SystemExit(main())
