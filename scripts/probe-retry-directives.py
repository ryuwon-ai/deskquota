#!/usr/bin/env python3
"""Check explicit upstream retry directives using the existing native retry fixture."""
import argparse
from datetime import datetime, timezone
import hashlib
import importlib.util
import json
import os
from pathlib import Path
import sys
import tempfile

sys.dont_write_bytecode = True
ROOT = Path(__file__).resolve().parents[1]
FIXTURE = ROOT / "scripts/probe-product-retry.py"
spec = importlib.util.spec_from_file_location("retry_fixture", FIXTURE)
fixture = importlib.util.module_from_spec(spec)
spec.loader.exec_module(fixture)


def run_case(binary, name, *, status=429, headers=(), body=fixture.TRANSIENT,
             expected_attempts=1, expected_pause=False, followup=False):
    rejection = fixture.reply(status, body, headers)
    plans = {"A1": [rejection, rejection], "B1": [fixture.reply()]}
    with fixture.gateway(binary, plans, True) as run:
        run.send("A1")
        run.receive("A1", rejection)
        first_trace = run.upstream.recorded()
        _, state = run.control("status")
        pause = state["admission"]["shared_cooldown_ms"]
        if followup:
            run.send("B1", "retry-b", "models")
            run.receive("B1", fixture.reply())
        trace = run.upstream.recorded()
        result = run.finish([row["label"] for row in trace])
    attempts = len(first_trace)
    observed_pause = pause > 0
    followup_delay = None
    if followup:
        followup_delay = trace[-1]["at"] - first_trace[0]["at"]
    checks = {
        "attempts_match": attempts == expected_attempts,
        "shared_pause_matches": observed_pause == expected_pause,
        # The explicit fixture hint is 800ms. This is an ordering lower bound,
        # not an estimate of proxy overhead or provider recovery performance.
        "followup_respected_hint": not (followup and expected_pause)
        or followup_delay >= 0.79,
        "original_wire_preserved": True,
    }
    return dict(name=name, status=status, expected_attempts=expected_attempts,
                observed_initial_attempts=attempts, expected_pause=expected_pause,
                shared_cooldown_ms=pause, followup_delay_seconds=followup_delay,
                checks=checks, passed=all(checks.values()), **result)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--binary", type=Path, required=True)
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument("--expect", choices=("retry-veto", "shared-pause-proposal"),
                        default="retry-veto",
                        help="retry-veto adopts only the replay veto; shared-pause-proposal also checks the unadopted 503 proposal")
    args = parser.parse_args()
    if args.output.exists():
        parser.error("output already exists")
    binary = args.binary.resolve(strict=True)
    result = dict(check="explicit_retry_directives", at=datetime.now(timezone.utc).isoformat(),
                  expectation=args.expect,
                  scope="Only retry-veto is adopted; shared 503 pause is an unadopted policy proposal",
                  binary=str(binary), binary_sha256=hashlib.sha256(binary.read_bytes()).hexdigest(),
                  probe_sha256=hashlib.sha256(Path(__file__).read_bytes()).hexdigest(),
                  fixture_sha256=hashlib.sha256(FIXTURE.read_bytes()).hexdigest(),
                  cases=[], passed=False,
                  limitations=["Synthetic loopback contract checks, not latency benchmarks",
                               "One configured upstream shared by two roots",
                               "Unlimited RPM and unknown TPM, no quota-accounting claim",
                               "No real provider, client SDK, Windows, or paid API calls"])
    plans = [
        dict(name="503_explicit_ms", status=503, headers=[("Retry-After-Ms", "800")],
             expected_pause=args.expect == "shared-pause-proposal", followup=True),
        dict(name="503_mixed_timing", status=503,
             headers=[("Retry-After", "malformed"), ("Retry-After-Ms", "800")],
             expected_pause=args.expect == "shared-pause-proposal", followup=True),
        dict(name="503_missing_timing", status=503, followup=True),
        dict(name="503_malformed_timing", status=503,
             headers=[("Retry-After", "malformed")], followup=True),
        dict(name="500_explicit_timing", status=500,
             headers=[("Retry-After-Ms", "800")], followup=True),
        dict(name="429_false", headers=[("Retry-After", "0"), ("X-Should-Retry", "false")]),
        dict(name="429_false_missing_timing", headers=[("X-Should-Retry", "false")],
             expected_pause=True),
        dict(name="429_conflicting_true_false",
             headers=[("Retry-After", "0"), ("X-Should-Retry", "true"),
                      ("X-Should-Retry", "false")]),
        dict(name="429_true_transient", headers=[("Retry-After", "0"), ("X-Should-Retry", "true")],
             expected_attempts=2),
        dict(name="429_true_permanent", headers=[("Retry-After", "0"), ("X-Should-Retry", "true")],
             body=b'{"error":{"code":"insufficient_quota"}}'),
        dict(name="429_false_explicit_pause", headers=[("Retry-After-Ms", "800"),
             ("X-Should-Retry", "false")], expected_pause=True, followup=True),
    ]
    original_environment = dict(os.environ)
    try:
        with tempfile.TemporaryDirectory(prefix="deskquota-directive-home-") as temporary:
            # Reuse the old fixture's lifecycle/transport; isolate inherited user
            # config and credentials without changing product or global settings.
            os.environ.clear()
            os.environ.update(HOME=temporary, PATH="/usr/bin:/bin:/usr/sbin:/sbin",
                              XDG_CONFIG_HOME=temporary + "/config",
                              XDG_CACHE_HOME=temporary + "/cache",
                              XDG_DATA_HOME=temporary + "/data")
            for plan in plans:
                try:
                    result["cases"].append(run_case(binary, **plan))
                except Exception as error:
                    result["cases"].append(dict(name=plan["name"], passed=False,
                                               harness_error=type(error).__name__,
                                               observation=getattr(error, "observation", None)))
    finally:
        os.environ.clear()
        os.environ.update(original_environment)
    result["passed"] = len(result["cases"]) == len(plans) and all(row["passed"] for row in result["cases"])
    result["binary_unchanged"] = result["binary_sha256"] == hashlib.sha256(binary.read_bytes()).hexdigest()
    result["passed"] &= result["binary_unchanged"]
    args.output.parent.mkdir(parents=True, exist_ok=True)
    args.output.write_text(json.dumps(result, indent=2) + "\n")
    print(json.dumps(dict(passed=result["passed"], cases=[
        {k: row.get(k) for k in ("name", "passed", "observed_initial_attempts", "shared_cooldown_ms", "harness_error")}
        for row in result["cases"]])))
    return 0 if result["passed"] else 1


if __name__ == "__main__":
    raise SystemExit(main())
