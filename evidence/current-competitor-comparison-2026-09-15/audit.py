"""Recompute this comparison from raw outcomes; never start a gateway."""
import collections
import hashlib
import json
import math
from pathlib import Path

BASE = Path(__file__).resolve().parent
ROOT = BASE.parents[1]


def stats(values):
    values = sorted(values)
    return {"n": len(values), "mean": sum(values) / len(values) if values else None,
            **{f"p{p}": values[math.ceil(p / 100 * len(values)) - 1] if values else None
               for p in (50, 95, 99)}, "max": max(values) if values else None}


assert stats([])["p95"] is None and stats([1, 2, 3, 4])["p95"] == 4
schedule = json.loads((BASE / "schedule.json").read_text())["runs"]
assert len(schedule) == 35
assert {p.parent.name for p in BASE.glob("*/result.json")} == {r["name"] for r in schedule}
preflight = json.loads((BASE / "preflight.json").read_text())
for name, digest in {**preflight["compiled_source"], **preflight["harness_files"]}.items():
    assert hashlib.sha256((ROOT / name).read_bytes()).hexdigest() == digest, name
assert hashlib.sha256(Path(preflight["binary"]["path"]).read_bytes()).hexdigest() == preflight["binary"]["sha256"]

runs, inputs_by_profile, totals = [], {}, collections.Counter()
for plan in schedule:
    path = BASE / plan["name"] / "result.json"
    data = json.loads(path.read_text())
    assert data["status"] == "completed" and data["owned_processes_cleaned"]
    assert not data["protocol_errors"] and data["client_retries"] == data["gateway_retries"] == 0
    assert data["cache"] == "off" and data["cap"] == 2
    assert all(data[k] == plan[k] for k in ("arm", "profile", "seed", "cost_contract"))
    inputs = {r["id"]: r for r in data["submitted"]}
    outputs = {r["id"]: r for r in data["outcomes"]}
    assert len(inputs) == len(outputs) == len(data["submitted"]) == len(data["outcomes"])
    assert inputs.keys() == outputs.keys()
    shape = [{k: v for k, v in r.items() if k not in ("canonical_input_units", "canonical_output_units")}
             for r in data["submitted"]]
    previous = inputs_by_profile.setdefault(data["profile"], shape)
    assert shape == previous, plan["name"]
    attempts = data["attempts"]
    assert len({a["id"] for a in attempts}) == len(attempts)
    assert all(a["ingress_id"] in inputs and a["outcome"] != "pending" for a in attempts)
    assert max(collections.Counter(a["ingress_id"] for a in attempts).values(), default=0) <= 1
    for outcome in outputs.values():
        assert math.isclose(outcome["elapsed_ms"], (outcome["ended_s"] - outcome["sent_s"]) * 1000, abs_tol=.001)
        if outcome["outcome"] == "completed":
            assert outcome["payload_valid"] and outcome["terminal_marker_ms"] is not None
            assert outcome["within_measurement"] == (outcome["ended_s"] <= data["measurement_start_s"] + 60)
    for name, group in data["summary"].items():
        selected = [r for r in outputs.values() if name in ("all", "generation_all") or
                    (name == "generation_short" and inputs[r["id"]]["length"] == "short") or
                    (name == "generation_long" and inputs[r["id"]]["length"] == "long")]
        observed = stats([r["elapsed_ms"] for r in selected if r["outcome"] == "completed"])
        assert group["submitted"] == len(selected)
        assert group["outcomes"] == collections.Counter(r["outcome"] for r in selected)
        assert group["within_window_completed"] == sum(r["outcome"] == "completed" and r["within_measurement"] for r in selected)
        assert all(group["success_latency_ms"][k] == observed[k] for k in ("n", "p50", "p95", "p99", "max"))
    if data["arm"] == "deskquota_native_actual":
        assert data["config"]["accounting"] == "actual"
        assert data["config"]["launched_binary"]["features"] == []
        assert data["config"]["launched_binary"]["sha256"] == preflight["binary"]["sha256"]
    warmup = data.get("warmup_outcomes", [])
    if data["profile"] == "no_wait":
        assert len(warmup) == len(data["warmup_attempts"]) == 5
        assert all(r["outcome"] == "completed" and r["payload_valid"] for r in warmup)
        assert len(outputs) == len(attempts) == 100
    samples = [data.get(k) for k in ("idle_resource", "workload_start_resource", "workload_end_resource")] + data["resources"]
    samples = [s for s in samples if s]
    start, end = data.get("workload_start_resource"), data.get("workload_end_resource")
    counts = collections.Counter(r["outcome"] for r in outputs.values())
    runs.append({"name": plan["name"], "sha256": hashlib.sha256(path.read_bytes()).hexdigest(),
                 **{k: data[k] for k in ("arm", "profile", "seed", "cost_contract", "started_utc")},
                 "groups": data["summary"], "attempts": len(attempts),
                 "upstream_outcomes": dict(collections.Counter(a["outcome"] for a in attempts)),
                 "usage": dict(collections.Counter(r.get("usage", {}).get("status", "not_observed") for r in outputs.values())),
                 "idle_rss_mib": data["idle_resource"]["rss_bytes"] / 2**20 if data.get("idle_resource") else None,
                 "max_sample_rss_mib": max(s["rss_bytes"] for s in samples) / 2**20 if samples else None,
                 "cpu_delta_s": end["cpu_seconds"] - start["cpu_seconds"] if start and end else None,
                 "first_output_delta_ms": stats([r["first_output_delta_ms"] for r in outputs.values()
                                                 if r["outcome"] == "completed" and r.get("first_output_delta_ms") is not None]),
                 "submission_to_upstream_ms": stats([(a["received_s"] - outputs[a["ingress_id"]]["sent_s"]) * 1000 for a in attempts]),
                 "root_max_terminal_ms": {str(root): max(r["elapsed_ms"] for r in outputs.values() if inputs[r["id"]]["root"] == root)
                                          for root in sorted({r["root"] for r in inputs.values()})}})
    totals.update(counts)
    totals.update(submitted=len(inputs), attempts=len(attempts), warmup=len(warmup), discovery=len(data["discovery_requests"]))

assert totals["submitted"] == 2680 and totals["warmup"] == 125
assert sum(totals[k] for k in ("completed", "rejected", "error", "timeout", "cancelled")) == totals["submitted"]
cache = json.loads((BASE / "cache.json").read_text())
assert cache["passed"] and cache["binary_sha256"] == preflight["binary"]["sha256"]
assert cache["script_sha256"] == preflight["harness_files"]["product/scripts/benchmark_cache.py"]
assert len(cache["requests"]) == 180 and cache["fixture_attempts"] == 120
assert all(r["status"] == 200 and r["body_exact"] and r["upstream_attempts"] == (0 if r["arm"] == "hit" else 1)
           for r in cache["requests"])
for name, expected in cache["summary"].items():
    observed = stats([r["elapsed_ms"] for r in cache["requests"] if f'{r["format"]}_{r["arm"]}' == name])
    assert observed["n"] == 30 and all(expected[k] == observed[k] for k in expected), name
assert all(r["first_output_delta_ms"] is not None for r in cache["requests"] if r["format"] == "sse")
assert cache["status_after"]["exact_cache"]["hits"] == cache["status_after"]["exact_cache"]["misses"] == 60
assert cache["status_after"]["admission"]["rpm_debited"] == "60"
assert cache["status_after"]["admission"]["tpm_debited"] == "240"
(BASE / "summary.json").write_text(json.dumps(runs, indent=2) + "\n")
audit = {"status": "PASS_WITH_LIMITS", "runs": len(runs), "totals": dict(totals),
         "separate_cache_requests": 180, "cache_hits_avoiding_upstream": 60,
         "source_and_harness_hashes_held": True, "same_input_shapes_across_arms": True,
         "limits": ["5 no-wait process repetitions of one workload; 1 quota run per contract and arm",
                    "not company RPM18/TPM450000/concurrency3; synthetic RPM16/TPM6000/concurrency2",
                    "different product quota policies; not a scheduler-only ablation",
                    "RSS samples not continuous peak; CPU from coarse ps process counter",
                    "no real provider, model quality, low-end or Windows verification"]}
(BASE / "audit.json").write_text(json.dumps(audit, indent=2) + "\n")
print(json.dumps(audit, indent=2))
