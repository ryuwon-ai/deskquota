#!/usr/bin/env python3
"""Owned PTY check that existing metadata-only endpoints remain valid."""
import json
import pathlib
import shutil

source = pathlib.Path(__file__).with_name("green_probe.py")
exec(compile(source.read_text().split("\ntry:\n", 1)[0], str(source), "exec"))

config = fixture("metadata_only", endpoints=["models"])
before = config.read_bytes()
try:
    run = PTY(config)
    through(run)
    run.choose("Apply")
    code = run.finish()
    result = {
        "exit": code,
        "same_bytes": before == config.read_bytes(),
        "models_endpoint_preserved": '"models"' in config.read_text(),
        "rejected_empty_generation_selection": b"select at least one actually supported endpoint" in run.buf,
    }
    pathlib.Path(__file__).with_name("metadata-only-result.json").write_text(
        json.dumps(result, indent=2) + "\n"
    )
    pathlib.Path(__file__).with_name("metadata-only.pty.txt").write_bytes(run.buf)
    print(json.dumps(result, indent=2))
finally:
    for run in RUNS:
        run.close()
    shutil.rmtree(SCRATCH)
