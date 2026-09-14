#!/usr/bin/env python3
"""A saved edit that predates setup is previewed and explicitly applied."""
import pathlib

src = pathlib.Path(__file__).with_name("probe_green_base.py")
exec(compile(src.read_text().split("\ntry:\n", 1)[0], str(src), "exec"))


def call(*args):
    result = subprocess.run(
        [str(BIN), *map(str, args)], capture_output=True, text=True, timeout=18
    )
    if result.returncode:
        raise AssertionError((args, result.returncode, result.stderr))
    return result.stdout


def status(config):
    return json.loads(call("status", "--json", "--config", config))


config = fixture("external_pending")
started = False
try:
    call("on", "--config", config)
    started = True
    before = status(config)
    config.write_text(config.read_text().replace("concurrency = 3", "concurrency = 4"))
    desired_fingerprint = digest(config.read_bytes())
    run = PTY(config)
    through(run)
    run.choose("Apply", b"\x1b[A\r")
    code = run.finish()
    after = status(config)
    result = {
        "setup_exit": code,
        "desired_fingerprint": desired_fingerprint,
        "old_worker_fingerprint": before["identity"]["fingerprint"],
        "new_worker_fingerprint": after["identity"]["fingerprint"],
        "saved_config_applied": after["identity"]["fingerprint"]
        == desired_fingerprint,
        "pending_restart": after["pending_restart"],
        "identity_changed": before["identity"] != after["identity"],
        "preview_disclosed_restart": b"Save and start explicitly restarts" in run.buf,
        "preview_disclosed_queue_cancel": b"cancels queued requests" in run.buf,
        "preview_disclosed_drain": b"drains active requests for up to 10 seconds"
        in run.buf,
    }
    (OUT / "external-pending-final-results.json").write_text(
        json.dumps(result, indent=2) + "\n"
    )
    (OUT / "external-pending-final.pty.txt").write_bytes(run.buf)
    print(json.dumps(result, indent=2))
finally:
    for child in RUNS:
        child.close()
    if started:
        call("off", "--config", config)
        assert status(config)["state"] == "stopped"
    shutil.rmtree(SCRATCH)
    (OUT / "external-pending-final-cleanup.json").write_text(
        json.dumps(
            {
                "all_pty_waited": all(child.p.poll() is not None for child in RUNS),
                "owned_pty_pids": [child.p.pid for child in RUNS],
                "scratch_removed": not SCRATCH.exists(),
                "worker_authenticated_stopped": True,
            },
            indent=2,
        )
        + "\n"
    )
