#!/usr/bin/env python3
"""Independently inspect completed quota artifacts; never execute the product."""

import argparse
from collections import Counter
from datetime import datetime, timezone
import hashlib
import json
import math
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
PRODUCT = ROOT / "product"
ARMS = {"direct", "production_rr", "benchmark_fifo", "benchmark_rr"}
TERMINAL = {"completed", "rejected", "error", "timeout", "cancelled"}


def require(condition, label):
    if not condition:
        raise ValueError(label)


def audit(entry, expected_windows):
    path = (PRODUCT / entry["path"]).resolve()
    require(path.is_relative_to(PRODUCT) and path.suffix == ".json", "artifact path")
    raw = path.read_bytes()
    require(hashlib.sha256(raw).hexdigest() == entry["sha256"], "manifest SHA")
    run = json.loads(raw)
    require(run["id"] == entry["id"], "run identity")
    require(run["phase"] == "quota" and run["arm"] in ARMS, "quota arm")
    require(run["quota_windows"] == expected_windows, "window count")
    require(run["measurement_duration_s"] == 60 * expected_windows, "real windows")
    require(run["client_retry"] == 0 and run["gateway_retry"] == 0, "retry off")
    require(run["mock_quota"] == {"rpm": 16, "tpm": 6000}, "pilot quota")
    plans = {item["id"]: item for item in run["submitted"]}
    outcomes = run["outcomes"]
    require(len(plans) == len(run["submitted"]) == 20 * expected_windows, "unique submissions")
    require(Counter(o["id"] for o in outcomes) == Counter(plans.keys()), "terminal IDs")
    limit = run["start_monotonic_s"] + run["measurement_duration_s"]
    for outcome in outcomes:
        require(outcome["outcome"] in TERMINAL, "terminal classification")
        require(outcome["within_measurement"] == (outcome["ended_s"] <= limit), "wall-time cutoff")
        if outcome["outcome"] != "completed":
            continue
        require(outcome.get("status") == 200 and outcome.get("payload_valid") is True, "valid success")
        marks = [outcome["first_http_ms"], outcome["first_body_ms"]]
        if not plans[outcome["id"]].get("metadata", False):
            marks += [outcome["first_output_delta_ms"], outcome["terminal_marker_ms"]]
        marks += [outcome["body_eof_ms"], outcome["elapsed_ms"]]
        require(all(isinstance(v, (int, float)) and math.isfinite(v) and v >= 0 for v in marks), "success landmarks")
        require(marks == sorted(marks), "success landmark order")
    attempts = run["attempts"]
    require(len({a["id"] for a in attempts}) == len(attempts), "unique attempt IDs")
    require(all(n == 1 for n in Counter(a["ingress_id"] for a in attempts).values()), "one attempt with retry off")
    for attempt in attempts:
        require(attempt["ingress_id"] in plans, "attempt owner")
        require(attempt["outcome"] in {"completed", "rejected", "disconnected"}, "attempt terminal")
        require(attempt["ended_s"] >= attempt["received_s"], "attempt clock order")
        plan = plans[attempt["ingress_id"]]
        expected = 0 if plan.get("metadata", False) else (
            math.ceil(attempt["body_bytes"] * plan["actual_ratio"])
            + max(1, math.ceil(plan["output_reservation"] * plan["actual_ratio"]))
        )
        require(attempt["actual_cost_fixture_units"] == expected, "fixture cost")
    starts = run["gateway_started_attempts"]
    if run["arm"] != "direct":
        require(len(attempts) <= starts <= len(plans), "gateway start denominator")
        cfg = run["config"]
        require(cfg["accounting"] == "reserved" and cfg["concurrency"] == 2 and cfg["roots"] == 4, "held proxy settings")
    require(run["owned_processes_cleaned"] is True, "owned process cleanup")
    total = Counter(o["outcome"] for o in outcomes)
    within = Counter(o["outcome"] for o in outcomes if o["ended_s"] <= limit)
    after = total - within
    summary = run["summary"]["all"]
    require(dict(total) == summary["outcomes"], "summary terminal denominator")
    require(dict(within) == summary["within_measurement_terminal"], "summary fixed-time denominator")
    require(dict(after) == summary["post_measurement_drain_outcomes"], "summary late denominator")
    require(within["completed"] == summary["within_measurement_completed"], "fixed-time completions")
    snapshot = run["at_measurement_end"]
    pending = set(snapshot["pending_ids"])
    require(len(pending) == len(snapshot["pending_ids"]) and pending <= plans.keys(), "pending IDs")
    require(snapshot["terminal"] + len(pending) == len(plans), "snapshot denominator")
    return {
        "run": run["id"], "seed": run["seed"], "arm": run["arm"],
        "sha256": entry["sha256"], "submitted": len(plans),
        "outcomes": dict(total), "within_measurement": dict(within),
        "post_measurement_drain": dict(after), "mock_attempts": len(attempts),
        "gateway_started_attempts": starts, "checks": "PASS",
    }


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--manifest", type=Path, default=PRODUCT / "artifacts/pilot.json")
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument("--allow-partial", action="store_true")
    args = parser.parse_args()
    manifest = json.loads(args.manifest.read_text())
    require(args.allow_partial or manifest["status"] == "completed_composite_matrix", "full matrix not completed")
    entries = [e for e in manifest["runs"] if e["id"].startswith("quota-")]
    require(len({e["id"] for e in entries}) == len(entries), "unique manifest quota IDs")
    results, errors = [], []
    for entry in entries:
        try:
            results.append(audit(entry, manifest["windows"]))
        except (ValueError, KeyError, TypeError, OSError) as error:
            errors.append({"run": entry["id"], "check_error": str(error)})
    expected = {(seed, arm) for seed in manifest["seeds"] for arm in ARMS}
    observed = {(r["seed"], r["arm"]) for r in results}
    require(observed <= expected, "unexpected seed/arm")
    if not args.allow_partial:
        require(observed == expected, "missing completed quota runs")
    record = {
        "created_utc": datetime.now(timezone.utc).isoformat(),
        "audit": "independent_completed_quota_artifact_inspection",
        "script_sha256": hashlib.sha256(Path(__file__).read_bytes()).hexdigest(),
        "manifest_status": manifest["status"], "partial_allowed": args.allow_partial,
        "verified_runs": len(results), "expected_runs": len(expected),
        "verified_ingress": sum(r["submitted"] for r in results),
        "runs": results, "passed": not errors, "errors": errors,
        "limits": [
            "Does not execute gateway, mock, benchmark or provider API",
            "Does not replace source review or validate actual payload bytes absent from artifacts",
            "Snapshot cutoff and nominal measurement cutoff can differ; no equality of pending lists is assumed",
            "No-wait measurements and real client/OS performance are outside this audit",
        ],
    }
    args.output.parent.mkdir(parents=True, exist_ok=True)
    with args.output.open("x") as handle:
        json.dump(record, handle, ensure_ascii=False, indent=2)
        handle.write("\n")
    print(json.dumps({key: record[key] for key in ("verified_runs", "expected_runs", "verified_ingress", "passed", "errors")}))
    return int(bool(errors))


if __name__ == "__main__":
    raise SystemExit(main())
