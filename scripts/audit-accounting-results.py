#!/usr/bin/env python3
"""Read-only, independent audit of the fixed five-seed accounting experiment.

No product modules are imported. The accepted original pilot supplies only its
hash-pinned submitted schedules, never control outcomes or performance values.
This checks recorded consistency; it does not attest a build or replay payloads.
"""

import argparse
from collections import Counter
from copy import deepcopy
from datetime import datetime, timezone
import hashlib
import json
import math
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
PRODUCT = ROOT / "product"
BASELINE = PRODUCT / "artifacts/pilot.json"
BASELINE_SHA = "9e6cbbb57bfaac2149eb39efaf43169dbf8e47cf708075cf83b83716615854f6"
BINARY_SHA = "e16ba39c54fcec9bc5ce4fb4039d551dccdd5cdfd3bec9e0a7c412e58bd4364f"
ARMS = ("production_rr", "production_actual")
QUOTA = {"rpm": 16, "tpm": 6000}


def require(condition, message):
    if not condition:
        raise ValueError(message)


def sha(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def reject_constant(value):
    raise ValueError(f"nonfinite JSON value: {value}")


def load(path):
    return json.loads(path.read_text(), parse_constant=reject_constant)


def finite(value, label):
    try:
        valid = type(value) in (int, float) and math.isfinite(value)
    except OverflowError:
        valid = False
    require(valid, f"{label}: finite number required")
    return value


def indexed(rows, label):
    result = {row["id"]: row for row in rows}
    require(len(result) == len(rows), f"{label}: duplicate ID")
    return result


def expected_snapshot(config, policy, quota):
    for key in ("listen_port", "upstream_port"):
        require(type(config[key]) is int and 1 <= config[key] <= 65535, f"invalid {key}")
    return {
        "listen": f"127.0.0.1:{config['listen_port']}",
        "concurrency": 2, "cancel_policy": "close", "accounting": policy,
        "retry_transient_429": False,
        "upstream": {"api_base": f"http://127.0.0.1:{config['upstream_port']}/v1", "auth": {"mode": "none"}},
        "quota": {name: {"kind": "known", "value": quota[name]} if quota else {"kind": "unlimited"}
                  for name in ("rpm", "tpm")},
        "models": [{"id": "synthetic", "max_output_tokens": 4096}],
        "roots": [{"id": f"r{i}", "endpoints": ["chat/completions", "models"], "models": ["synthetic"]}
                  for i in range(4)],
    }


def recorded_toml(config, policy, quota):
    """Reconstruct the experiment's fixed serialization, independently of its renderer."""
    lines = [f'listen = "127.0.0.1:{config["listen_port"]}"', 'concurrency = 2',
             'cancel_policy = "close"', f'accounting = "{policy}"',
             'retry_transient_429 = false', '[upstream]',
             f'api_base = "http://127.0.0.1:{config["upstream_port"]}/v1"',
             '[upstream.auth]', 'mode = "none"']
    for name in ("rpm", "tpm"):
        lines += ['', f'[quota.{name}]', 'kind = "known"' if quota else 'kind = "unlimited"']
        if quota:
            lines.append(f'value = {quota[name]}')
    lines += ['', '[[models]]', 'id = "synthetic"', 'max_output_tokens = 4096']
    for i in range(4):
        lines += ['', '[[roots]]', f'id = "r{i}"', 'endpoints = ["chat/completions", "models"]', 'models = ["synthetic"]']
    return ('\n'.join(lines) + '\n').encode()


def normalized(snapshot):
    result = deepcopy(snapshot)
    # Only these two allocated port numbers vary in the current fixture.
    result["listen"] = "127.0.0.1:<port>"
    result["upstream"]["api_base"] = "http://127.0.0.1:<port>/v1"
    return result


def distribution(values):
    values = sorted(values)
    return {"n": len(values), **{key: values[min(len(values)-1, math.ceil(len(values)*p)-1)] if values else None
                                for key, p in (("p50", .5), ("p95", .95), ("max", 1))}}


def audit_run(run, expected_rows, binary, phase="quota"):
    arm = run["arm"]
    require(arm in ARMS, "unexpected arm")
    policy = "actual" if arm == ARMS[1] else "reserved"
    duration, windows, quota = (300, 5, QUOTA) if phase == "quota" else (3, 0, None)
    require(run["phase"] == phase and run["measurement_duration_s"] == duration and run["quota_windows"] == windows,
            "measurement phase/duration/windows mismatch")
    require(run["mock_quota"] == quota and run["config"]["quota"] == quota, "quota mismatch")
    require(run["client_retry"] == 0 and run["gateway_retry"] == 0 and run["cache"] == "off", "retry/cache mismatch")
    require(run["owned_processes_cleaned"] is True, "owned cleanup incomplete")
    config = run["config"]
    require(config["launched_binary"] == binary and binary["features"] == [], "ordinary binary identity mismatch")
    require(config["accounting"] == policy and config["concurrency"] == 2 and config["roots"] == 4
            and config["cancel_policy"] == "close" and config["retry_transient_429"] is False, "config policy mismatch")
    require(config["snapshot"] == expected_snapshot(config, policy, quota), "config snapshot mismatch")
    require(config["raw_toml_sha256"] == hashlib.sha256(recorded_toml(config, policy, quota)).hexdigest(), "TOML hash mismatch")
    for name in ("initial_status", "final_status"):
        status = run[name]
        require(status["identity"]["fingerprint"] == config["raw_toml_sha256"] and status["admission"]["accounting"] == policy,
                f"{name}: runtime evidence mismatch")
        require(status["identity"]["address"] == config["snapshot"]["listen"], f"{name}: address mismatch")
    require(run["initial_status"]["identity"] == run["final_status"]["identity"], "worker identity changed")
    final = run["final_status"]
    require(final["active"] == 0 and final["admission"]["queue_length"] == 0, "remaining active/queued work")
    require(run["submitted"] == expected_rows, "submitted schedule differs from pinned reference")
    submitted = indexed(run["submitted"], "submitted")
    outcomes = indexed(run["outcomes"], "outcomes")
    attempts = indexed(run["attempts"], "attempts")
    require(set(submitted) == set(outcomes), "terminal denominator mismatch")
    require(not run["warmup_attempts"], "unexpected quota/smoke warmup attempts")
    # A cancellation can close an acquired upstream attempt before it reaches
    # the mock. Started and received are separate observations.
    require(len(attempts) <= run["gateway_started_attempts"] <= len(submitted), "gateway/mock attempt count bounds")
    by_ingress = {key: [] for key in submitted}
    for attempt in attempts.values():
        key = attempt["ingress_id"]
        require(key in submitted, "unlinked attempt")
        by_ingress[key].append(attempt)
        row = submitted[key]
        require(attempt["outcome"] in {"completed", "disconnected", "rejected"}, "nonterminal mock attempt")
        require(finite(attempt["received_s"], "received_s") <= finite(attempt["ended_s"], "attempt ended_s"), "attempt time order")
        payload = {"model": "synthetic", "messages": [{"role": "user", "content": "x" * row["input_bytes"]}],
                   "max_tokens": row["output_reservation"], "stream": True}
        size = 0 if row.get("metadata") else len(json.dumps(payload, separators=(",", ":")).encode())
        cost = 0 if row.get("metadata") else math.ceil(size * row["actual_ratio"]) + max(1, math.ceil(row["output_reservation"] * row["actual_ratio"]))
        require(attempt["body_bytes"] == size and attempt["actual_cost_fixture_units"] == cost
                and attempt["estimated_cost"] == (0 if row.get("metadata") else size + row["output_reservation"]), "attempt fixture cost mismatch")
    require(all(len(rows) <= 1 for rows in by_ingress.values()), "unexpected retry attempt")
    start = finite(run["start_monotonic_s"], "start")
    cutoff = start + finite(run["measurement_duration_s"], "duration")
    if phase == "quota":
        require(start - finite(config["started_monotonic_s"], "worker start") >= 60, "startup hold bypassed")
    within, usage_count = [], Counter()
    for key, outcome in outcomes.items():
        kind = outcome["outcome"]
        require(kind in {"completed", "rejected", "timeout", "cancelled", "error"}, "unknown terminal kind")
        ended = finite(outcome["ended_s"], "ended_s")
        sent = finite(outcome["sent_s"], "sent_s")
        require(start <= sent <= ended and math.isclose((ended-sent)*1000, finite(outcome["elapsed_ms"], "elapsed_ms"), abs_tol=.01), "terminal timing mismatch")
        require(outcome["within_measurement"] is (ended <= cutoff), "cutoff flag mismatch")
        if ended <= cutoff:
            within.append(outcome)
        row, usage = submitted[key], outcome["usage"]
        usage_count[usage["status"]] += 1
        if kind == "completed":
            require(outcome["payload_valid"] is True and outcome["status"] == 200, "completed payload/status invalid")
            require(len(by_ingress[key]) == 1 and by_ingress[key][0]["outcome"] == "completed", "completion attempt linkage mismatch")
            if row.get("metadata"):
                require(usage == {"status": "not_applicable"}, "metadata usage mismatch")
            else:
                require(usage["status"] == "observed", "completed generation usage missing")
                values = [usage.get("prompt_tokens"), usage.get("completion_tokens")]
                require(all(type(v) is int and 0 <= v < 2**64 for v in values), "usage must be u64 integers")
                attempt = by_ingress[key][0]
                require(values == [math.ceil(attempt["body_bytes"]*row["actual_ratio"]), max(1, math.ceil(row["output_reservation"]*row["actual_ratio"]))]
                        and sum(values) == attempt["actual_cost_fixture_units"], "usage/fixture mismatch")
                require(outcome["terminal_marker_ms"] is not None and outcome["body_eof_ms"] is not None, "completion terminal evidence missing")
    scheduled = {key for key, row in submitted.items() if row.get("cancel_after_ms") is not None}
    summary = {
        "submitted": len(submitted), "all_terminal": dict(Counter(o["outcome"] for o in outcomes.values())),
        "within_measurement_terminal": dict(Counter(o["outcome"] for o in within)),
        "within_measurement_completed": sum(o["outcome"] == "completed" for o in within),
        "post_measurement_drain_completed": sum(o["outcome"] == "completed" and o["ended_s"] > cutoff for o in outcomes.values()),
        "upstream_attempts": len(attempts), "mock_429_attempts": sum(a["outcome"] == "rejected" for a in attempts.values()),
        "additional_attempts_beyond_first": 0, "scheduled_cancellations": len(scheduled),
        "scheduled_cancellation_outcomes": dict(Counter(outcomes[key]["outcome"] for key in scheduled)),
        "completion_ms_success_only": distribution([o["elapsed_ms"] for o in outcomes.values() if o["outcome"] == "completed"]),
    }
    for field in ("root", "length"):
        summary[field + "_results"] = {
            str(value): {"submitted": sum(r[field] == value for r in submitted.values()),
                         "within_measurement_completed": sum(o["outcome"] == "completed" and submitted[o["id"]][field] == value for o in within)}
            for value in sorted({r[field] for r in submitted.values()}, key=str)}
    return summary, dict(usage_count)


def audit(manifest_path, expected_pairs):
    require(1 <= expected_pairs <= 5, "expected-pairs must be 1..5")
    manifest = load(manifest_path)
    require(manifest["mode"] == "accounting" and manifest["seeds"] == [1, 2, 3, 4, 5] and manifest["windows"] == 5,
            "five-seed accounting manifest required")
    schedule = [["quota", seed, arm] for seed in range(1, 6) for arm in (ARMS if seed % 2 else ARMS[::-1])]
    require(manifest["schedule"] == schedule, "planned schedule mismatch")
    require(manifest["planned_pairs"] == 5 and manifest["completed_pairs"] == expected_pairs, "pair count mismatch")
    complete = expected_pairs == 5
    require(manifest["matrix_complete"] is complete, "matrix_complete mismatch")
    require(manifest["status"] == ("completed_accounting_matrix" if complete else "accounting_pilot_completed"), "checkpoint status mismatch")
    identity = manifest["identity"]
    binary = identity["production"]
    require(binary == {"path": str(PRODUCT / "target/native/release/llmgw"), "sha256": BINARY_SHA, "features": []}, "held binary metadata mismatch")
    require(sha(Path(binary["path"])) == BINARY_SHA and identity["benchmark"] == {"used": False}, "binary changed or benchmark used")
    paths = [PRODUCT / name for name in ("Cargo.toml", "Cargo.lock", "rust-toolchain.toml")]
    for folder in ("src", "scripts", "examples", "fixtures/workloads", "tests", "docs"):
        paths += [p for p in (PRODUCT / folder).rglob("*") if p.is_file() and "__pycache__" not in p.parts]
    require(identity["sources"] == {str(p.relative_to(PRODUCT)): sha(p) for p in paths}, "measured source identity differs from current HOLD")
    require(sha(BASELINE) == BASELINE_SHA, "pinned schedule reference changed")
    baseline = indexed(load(BASELINE)["runs"], "baseline runs")
    entries = indexed(manifest["runs"], "manifest runs")
    expected_ids = [f"{phase}-seed{seed}-{arm}" for phase, seed, arm in schedule[:expected_pairs*2]]
    require(list(entries) == expected_ids, "completed run order/count mismatch")
    directory = manifest_path.with_name(manifest_path.stem + "-runs")
    allowed = {name + suffix for name in expected_ids for suffix in (".json", ".events.jsonl")}
    require({p.name for p in directory.iterdir()} == allowed, "orphan/missing run or journal")
    results, files, usages = {}, [], {}
    for run_id, entry in entries.items():
        path = directory / (run_id + ".json")
        require((PRODUCT / entry["path"]).resolve() == path.resolve() and sha(path) == entry["sha256"], "raw artifact path/hash mismatch")
        run = load(path)
        require(run["id"] == run_id == f'quota-seed{run["seed"]}-{run["arm"]}', "raw content identity mismatch")
        ref_entry = baseline[f'quota-seed{run["seed"]}-production_rr']
        ref_path = PRODUCT / ref_entry["path"]
        require(sha(ref_path) == ref_entry["sha256"], "pinned submitted-schedule artifact changed")
        summary, usage = audit_run(run, load(ref_path)["submitted"], binary)
        require(entry["summary"] == run["summary"], "registered/raw summary mismatch")
        require(entry["provenance"] == {key: run["config"][key] for key in ("accounting", "raw_toml_sha256", "launched_binary")}, "registered provenance mismatch")
        journal = path.with_suffix(".events.jsonl")
        events = [json.loads(line, parse_constant=reject_constant) for line in journal.read_text().splitlines()]
        require(events[0]["event"] == "started" and events[0]["id"] == run_id, "journal start mismatch")
        require(events[-1] == {"event": "validated", "attempts": len(run["attempts"]), "outcomes": 100, "submitted": 100}, "journal not validated")
        require([e["pid"] for e in events if e["event"] == "gateway_started"] == [run["initial_status"]["identity"]["pid"]], "owned PID journal mismatch")
        terminals = indexed([e for e in events if e["event"] == "terminal"], "journal terminal")
        require(set(terminals) == {o["id"] for o in run["outcomes"]}, "journal terminal denominator mismatch")
        for outcome in run["outcomes"]:
            recorded = {k: v for k, v in terminals[outcome["id"]].items() if k != "event"}
            require(recorded == {k: v for k, v in outcome.items() if k not in ("within_measurement", "send_to_mock_receive_ms")}, "journal/raw terminal mismatch")
        results[run_id] = (run, summary)
        usages[run_id] = usage
        files.append({"id": run_id, "path": str(path.relative_to(ROOT)), "sha256": sha(path), "journal_sha256": sha(journal),
                      "schedule_reference_sha256": ref_entry["sha256"], "owned_pid": run["initial_status"]["identity"]["pid"]})
    pairs = []
    for seed in range(1, expected_pairs+1):
        runs = [results[f"quota-seed{seed}-{arm}"] for arm in ARMS]
        left, right = [normalized(run["config"]["snapshot"]) for run, _ in runs]
        require(left.pop("accounting") == "reserved" and right.pop("accounting") == "actual" and left == right, "paired config differs outside accounting")
        require(runs[0][0]["submitted"] == runs[1][0]["submitted"], "paired schedule mismatch")
        summaries = {arm: result[1] for arm, result in zip(ARMS, runs)}
        pairs.append({"seed": seed, "arms": summaries, "actual_minus_reserved_within_measurement_completed":
                      summaries[ARMS[1]]["within_measurement_completed"] - summaries[ARMS[0]]["within_measurement_completed"]})
    require(manifest["evaluation"]["pairs"] == pairs, "derived paired evaluation mismatch")
    return {"passed": True, "completed_pairs": expected_pairs, "ingress": 200 * expected_pairs,
            "pairs": pairs, "usage_statuses": usages, "artifacts": files, "source_files_checked": len(paths),
            "binary_sha256": BINARY_SHA, "schedule_reference_manifest_sha256": BASELINE_SHA,
            "cleanup_basis": "Recorded cleanup, empty final queue and journal PID linkage; live resource inspection is separate."}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--manifest", required=True, type=Path)
    parser.add_argument("--expected-pairs", required=True, type=int)
    parser.add_argument("--output", required=True, type=Path)
    args = parser.parse_args()
    result = {"auditor_sha256": sha(Path(__file__)), "checked_at": datetime.now(timezone.utc).isoformat(),
              "manifest": str(args.manifest.resolve()), "performance_claim": False}
    try:
        result["manifest_sha256"] = sha(args.manifest)
        result.update(audit(args.manifest.resolve(), args.expected_pairs))
    except (OSError, ValueError, KeyError, TypeError) as error:
        result.update(passed=False, error=f"{type(error).__name__}: {error}")
    args.output.parent.mkdir(parents=True, exist_ok=True)
    with args.output.open("x") as handle:
        json.dump(result, handle, indent=2, allow_nan=False)
        handle.write("\n")
    print(json.dumps({key: result[key] for key in ("passed", "error", "completed_pairs", "ingress") if key in result}))
    return 0 if result["passed"] else 1


if __name__ == "__main__":
    raise SystemExit(main())
