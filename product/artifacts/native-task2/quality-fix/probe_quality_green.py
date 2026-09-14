#!/usr/bin/env python3
"""Final release PTYs for Q1/Q2 and the adjacent F8/F6 runtime contracts."""
from support_green import *


def call(*args, check=True):
    result = subprocess.run(
        [str(BIN), *map(str, args)], capture_output=True, text=True, timeout=18
    )
    if check and result.returncode:
        raise AssertionError((args, result.returncode, result.stdout, result.stderr))
    return result


def status(config):
    return json.loads(call("status", "--json", "--config", config).stdout)


def free_port():
    with socket.socket() as probe:
        probe.bind(("127.0.0.1", 0))
        return probe.getsockname()[1]


def default_connection(runner):
    for marker in [
        "Setup: Environment",
        "Environment preset",
        "Setup: Connection",
        "Upstream API base",
        "Actually supported endpoint",
        "Upstream auth",
    ]:
        runner.choose(marker)


def corporate_connection(runner, ca_path):
    runner.choose("Setup: Environment")
    runner.choose("Environment preset", b"\x1b[A\r")
    for marker in [
        "Setup: Connection",
        "Upstream API base",
        "Actually supported endpoint",
        "Upstream auth",
        "Use an explicit upstream proxy",
    ]:
        runner.choose(marker)
    runner.choose("Merge an explicit PEM CA bundle", b"y")
    runner.choose("PEM CA bundle path", str(ca_path).encode() + b"\r")


def finish_defaults(runner, *, port=None, save_and_start=False):
    runner.choose("Setup: Models")
    runner.choose("Keep all existing models")
    for marker in [
        "Setup: Quota",
        "RPM",
        "TPM",
        "same quota shared",
        "separate input/output",
        "Concurrency",
        "Setup: Run",
    ]:
        runner.choose(marker)
    runner.choose(
        "Local loopback port",
        b"\r" if port is None else f"{port}\r".encode(),
    )
    for marker in [
        "Request start at next login",
        "Setup: Tools",
        "Client intents",
        "Setup: Apply",
    ]:
        runner.choose(marker)
    runner.choose("Apply", b"\x1b[A\r" if save_and_start else b"\r")
    return runner.finish()


def configure_model(runner, model_id, cap):
    runner.choose("Setup: Models")
    runner.choose("Keep all existing models", b"n")
    runner.choose("Try one bounded GET /models")
    runner.choose("Manual model ID", b"\x15" + model_id.encode() + b"\r")
    runner.choose("Set a user-chosen reservation fallback")
    runner.choose("Nonzero reservation fallback", b"\x15" + str(cap).encode() + b"\r")
    for marker in [
        "Setup: Quota",
        "RPM",
        "TPM",
        "same quota shared",
        "separate input/output",
        "Concurrency",
        "Setup: Run",
        "Local loopback port",
        "Request start at next login",
        "Setup: Tools",
        "Client intents",
        "Setup: Apply",
    ]:
        runner.choose(marker)
    runner.choose("Apply")
    return runner.finish()


def no_upstream_attempt(listener):
    try:
        listener.accept()
    except BlockingIOError:
        return True
    return False


def multi_route_fixture(name):
    directory = SCRATCH / name
    directory.mkdir()
    config = directory / "gateway 한글 config.toml"
    config.write_text(
        f'''# preserved user heading
listen = "127.0.0.1:{free_port()}" # port comment
concurrency = 3
cancel_policy = "close"
accounting = "actual"
retry_transient_429 = true
[upstream]
api_base = "http://127.0.0.1:9/company/v1"
auth = {{ mode = "none" }}
[quota]
rpm = {{ kind = "unlimited" }}
tpm = {{ kind = "unlimited" }}
[[models]]
id = "model-a"
max_output_tokens = 64
[[models]]
id = "model-b"
max_output_tokens = 32
[[roots]]
id = "pi"
endpoints = ["responses"] # first endpoint comment
models = ["model-a"]
[[roots]]
id = "claude" # second route comment
endpoints = [
  # untouched endpoint array note
  "messages",
]
models = [
  # untouched models array note
  "model-b",
]
'''
    )
    return config


configs = []
listeners = []
results = {}
cleanup = []
try:
    # Q1 endpoint edit: first route changes; unrelated and second-route comments survive.
    config = multi_route_fixture("q1-endpoint 편집")
    configs.append(config)
    before = config.read_bytes()
    runner = PTY(config)
    for marker in [
        "Setup: Environment",
        "Environment preset",
        "Setup: Connection",
        "Upstream API base",
    ]:
        runner.choose(marker)
    runner.choose("Actually supported endpoint", b" \r")
    runner.choose("Upstream auth")
    code = finish_defaults(runner)
    after = config.read_bytes()
    text_after = after.decode()
    endpoint_result = {
        "exit": code,
        "heading_preserved": "# preserved user heading" in text_after,
        "port_comment_preserved": "# port comment" in text_after,
        "second_endpoint_note_preserved": "# untouched endpoint array note" in text_after,
        "second_models_note_preserved": "# untouched models array note" in text_after,
        "responses_kept": '"responses"' in text_after,
        "completions_added": '"chat/completions"' in text_after,
        "before_sha256": digest(before),
        "after_sha256": digest(after),
    }
    assert all(
        endpoint_result[key]
        for key in [
            "heading_preserved",
            "port_comment_preserved",
            "second_endpoint_note_preserved",
            "second_models_note_preserved",
            "responses_kept",
            "completions_added",
        ]
    )
    assert code == 0
    results["q1_endpoint_edit"] = endpoint_result
    (OUT / "q1-endpoint-before.toml").write_bytes(before)
    (OUT / "q1-endpoint-after.toml").write_bytes(after)
    (OUT / "q1-endpoint.pty.txt").write_bytes(runner.buf)

    # Q1 model edit: unrelated comments and field comments survive the replacement.
    config = fixture("q1-model 편집")
    configs.append(config)
    original = config.read_text().replace(
        'id = "model-a"', 'id = "model-a" # model id comment'
    ).replace(
        'endpoints = ["responses"]',
        'endpoints = ["responses"] # route endpoint comment',
    )
    config.write_text(original)
    before = config.read_bytes()
    runner = PTY(config)
    default_connection(runner)
    code = configure_model(runner, "model-next", 96)
    after = config.read_bytes()
    text_after = after.decode()
    model_result = {
        "exit": code,
        "heading_preserved": "# preserved user heading" in text_after,
        "port_comment_preserved": "# port comment" in text_after,
        "model_comment_preserved": "# model id comment" in text_after,
        "route_comment_preserved": "# route endpoint comment" in text_after,
        "model_changed": 'id = "model-next"' in text_after,
        "cap_changed": "max_output_tokens = 96" in text_after,
        "route_model_changed": 'models = ["model-next"]' in text_after,
        "before_sha256": digest(before),
        "after_sha256": digest(after),
    }
    assert code == 0 and all(
        model_result[key]
        for key in [
            "heading_preserved",
            "port_comment_preserved",
            "model_comment_preserved",
            "route_comment_preserved",
            "model_changed",
            "cap_changed",
            "route_model_changed",
        ]
    )
    results["q1_model_edit"] = model_result
    (OUT / "q1-model-before.toml").write_bytes(before)
    (OUT / "q1-model-after.toml").write_bytes(after)
    (OUT / "q1-model.pty.txt").write_bytes(runner.buf)

    # Q2 missing and malformed local CA: old identity remains live and no GET occurs.
    for case, ca_bytes, expected in [
        ("missing", None, "failed to read configured CA bundle"),
        (
            "malformed",
            b"-----BEGIN CERTIFICATE-----\nnot-valid-base64!\n-----END CERTIFICATE-----\n",
            "configured CA bundle is not valid PEM certificates",
        ),
    ]:
        upstream = socket.socket()
        upstream.bind(("127.0.0.1", 0))
        upstream.listen()
        upstream.setblocking(False)
        listeners.append(upstream)
        config = fixture(f"q2-{case} CA")
        configs.append(config)
        config.write_text(
            config.read_text().replace(
                "127.0.0.1:9", f"127.0.0.1:{upstream.getsockname()[1]}"
            )
        )
        ca_path = config.parent / f"{case}.pem"
        if ca_bytes is not None:
            ca_path.write_bytes(ca_bytes)
        call("on", "--config", config)
        before_status = status(config)
        runner = PTY(config)
        corporate_connection(runner, ca_path)
        code = finish_defaults(runner, save_and_start=True)
        after_status = status(config)
        transcript = runner.buf.decode(errors="replace")
        result = {
            "exit": code,
            "before_state": before_status["state"],
            "after_state": after_status["state"],
            "identity_preserved": before_status["identity"] == after_status["identity"],
            "pending_restart": after_status["pending_restart"],
            "actionable_error": expected in transcript,
            "generic_child_error_absent": "worker_start_failed" not in transcript,
            "upstream_attempts": 0 if no_upstream_attempt(upstream) else 1,
            "saved_ca_path": str(ca_path) in config.read_text(),
        }
        assert code == 1
        assert result["after_state"] == "running"
        assert result["identity_preserved"]
        assert result["pending_restart"]
        assert result["actionable_error"] and result["generic_child_error_absent"]
        assert result["upstream_attempts"] == 0 and result["saved_ca_path"]
        results[f"q2_{case}_ca"] = result
        (OUT / f"q2-{case}-ca.pty.txt").write_bytes(runner.buf)

    # Q2 valid CA path starts a stopped worker and still performs no upstream request.
    upstream = socket.socket()
    upstream.bind(("127.0.0.1", 0))
    upstream.listen()
    upstream.setblocking(False)
    listeners.append(upstream)
    config = fixture("q2-valid CA startup")
    configs.append(config)
    config.write_text(
        config.read_text().replace(
            "127.0.0.1:9", f"127.0.0.1:{upstream.getsockname()[1]}"
        )
    )
    ca_path = ROOT / "tests/fixtures/setup-localhost-cert.pem"
    runner = PTY(config)
    corporate_connection(runner, ca_path)
    code = finish_defaults(runner, save_and_start=True)
    valid_status = status(config)
    valid_result = {
        "exit": code,
        "state": valid_status["state"],
        "authenticated_fingerprint": valid_status["identity"]["fingerprint"],
        "desired_fingerprint": digest(config.read_bytes()),
        "pending_restart": valid_status["pending_restart"],
        "upstream_attempts": 0 if no_upstream_attempt(upstream) else 1,
        "saved_ca_path": str(ca_path) in config.read_text(),
    }
    assert code == 0 and valid_result["state"] == "running"
    assert valid_result["authenticated_fingerprint"] == valid_result["desired_fingerprint"]
    assert not valid_result["pending_restart"] and valid_result["upstream_attempts"] == 0
    assert valid_result["saved_ca_path"]
    results["q2_valid_ca_startup"] = valid_result
    (OUT / "q2-valid-ca.pty.txt").write_bytes(runner.buf)

    # F8: SaveOnly keeps the old worker; the next SaveAndStart applies the saved fingerprint.
    config = fixture("f8 saved pending")
    configs.append(config)
    call("on", "--config", config)
    original_status = status(config)
    next_port = free_port()
    first = PTY(config)
    default_connection(first)
    first_code = finish_defaults(first, port=next_port)
    pending_status = status(config)
    desired_fingerprint = digest(config.read_bytes())
    second = PTY(config)
    default_connection(second)
    second_code = finish_defaults(second, save_and_start=True)
    applied_status = status(config)
    f8_result = {
        "save_only_exit": first_code,
        "save_only_identity_preserved": pending_status["identity"]
        == original_status["identity"],
        "save_only_pending_restart": pending_status["pending_restart"],
        "save_and_start_exit": second_code,
        "desired_fingerprint": desired_fingerprint,
        "applied_fingerprint": applied_status["identity"]["fingerprint"],
        "pending_restart_after": applied_status["pending_restart"],
        "identity_changed_on_apply": applied_status["identity"]
        != original_status["identity"],
        "restart_disclosed": b"explicitly restarts" in second.buf,
    }
    assert first_code == 0 and second_code == 0
    assert f8_result["save_only_identity_preserved"] and f8_result["save_only_pending_restart"]
    assert f8_result["applied_fingerprint"] == desired_fingerprint
    assert not f8_result["pending_restart_after"] and f8_result["identity_changed_on_apply"]
    assert f8_result["restart_disclosed"]
    results["f8_saved_pending"] = f8_result
    (OUT / "f8-save-only.pty.txt").write_bytes(first.buf)
    (OUT / "f8-save-and-start.pty.txt").write_bytes(second.buf)

    # F6: exact no-op SaveAndStart is authenticated and keeps PID/nonce/fingerprint.
    config = fixture("f6 running no-op")
    configs.append(config)
    before_bytes = config.read_bytes()
    call("on", "--config", config)
    before_status = status(config)
    runner = PTY(config)
    default_connection(runner)
    code = finish_defaults(runner, save_and_start=True)
    after_status = status(config)
    f6_result = {
        "exit": code,
        "same_config_bytes": before_bytes == config.read_bytes(),
        "same_identity": before_status["identity"] == after_status["identity"],
        "pending_restart": after_status["pending_restart"],
        "idempotent_disclosed": b"authenticated idempotent on" in runner.buf,
        "restart_side_effect_absent": b"cancels queued" not in runner.buf
        and b"drains active" not in runner.buf,
    }
    assert code == 0 and all(
        f6_result[key]
        for key in [
            "same_config_bytes",
            "same_identity",
            "idempotent_disclosed",
            "restart_side_effect_absent",
        ]
    )
    assert not f6_result["pending_restart"]
    results["f6_running_noop"] = f6_result
    (OUT / "f6-running-noop.pty.txt").write_bytes(runner.buf)

    (OUT / "pty-final-results.json").write_text(json.dumps(results, indent=2) + "\n")
    print(json.dumps(results, indent=2))
finally:
    for runner in RUNS:
        runner.close()
    cleanup_ok = True
    for config in configs:
        off = call("off", "--config", config, check=False)
        try:
            stopped = status(config)
            is_stopped = stopped["state"] == "stopped"
        except Exception as error:
            is_stopped = False
            stopped = {"error": str(error)}
        cleanup.append(
            {
                "config": str(config),
                "off_exit": off.returncode,
                "state": stopped.get("state"),
                "authenticated_stopped": is_stopped,
            }
        )
        cleanup_ok = cleanup_ok and is_stopped
    for listener in listeners:
        listener.close()
    cleanup_record = {
        "workers": cleanup,
        "all_pty_waited": all(runner.p.poll() is not None for runner in RUNS),
        "owned_pty_pids": [runner.p.pid for runner in RUNS],
        "scratch": str(SCRATCH),
        "scratch_removed": False,
    }
    if cleanup_ok and cleanup_record["all_pty_waited"]:
        shutil.rmtree(SCRATCH)
        cleanup_record["scratch_removed"] = not SCRATCH.exists()
    (OUT / "pty-final-cleanup.json").write_text(
        json.dumps(cleanup_record, indent=2) + "\n"
    )
    if not cleanup_record["scratch_removed"]:
        raise AssertionError("owned cleanup was not authenticated; scratch retained")
