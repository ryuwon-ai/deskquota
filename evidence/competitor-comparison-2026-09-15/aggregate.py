"""Recalculate the completed local experiment without starting any runtime."""
import collections
import hashlib
import json
import math
import sys
from pathlib import Path

BASE = Path(__file__).resolve().parent
ROOT = BASE.parents[1]
sys.path.insert(0, str(ROOT / "product/scripts"))
from benchmark_accounting import differing_paths, normalize_config_snapshot


def stats(values):
    ordered = sorted(values)
    return {"n": len(values), "mean": sum(values) / len(values) if values else None,
            **{f"p{p}": ordered[math.ceil(p / 100 * len(values)) - 1] if values else None
               for p in (50, 95, 99)}, "max": max(values) if values else None}


assert stats([1, 2, 3, 4])["p95"] == 4 and stats([])["mean"] is None
paths = sorted(BASE.glob("*/result.json"))
assert len(paths) == 44, len(paths)
runs = []
raw = {}
totals = collections.Counter()
for path in paths:
    data = json.loads(path.read_text())
    assert data["status"] == "completed" and not data["protocol_errors"], path
    assert data["owned_processes_cleaned"] and data["client_retries"] == data["gateway_retries"] == 0
    inputs = {r["id"]: r for r in data["submitted"]}
    outputs = {r["id"]: r for r in data["outcomes"]}
    assert len(inputs) == len(data["submitted"]) == len(outputs) == len(data["outcomes"])
    assert inputs.keys() == outputs.keys()
    attempts = data["attempts"]
    assert len({a["id"] for a in attempts}) == len(attempts)
    assert all(a["ingress_id"] in inputs and a["outcome"] != "pending" for a in attempts)
    assert max(collections.Counter(a["ingress_id"] for a in attempts).values(), default=0) <= 1
    counts = collections.Counter(r["outcome"] for r in outputs.values())
    assert counts == data["summary"]["all"]["outcomes"]
    for outcome in outputs.values():
        assert outcome["elapsed_ms"] >= 0
        assert math.isclose(outcome["elapsed_ms"], (outcome["ended_s"] - outcome["sent_s"]) * 1000, abs_tol=0.001)
        if outcome["outcome"] == "completed":
            assert outcome["payload_valid"] is True
            assert outcome["within_measurement"] == (outcome["ended_s"] <= data["measurement_start_s"] + 60)
    for name, group in data["summary"].items():
        selected = [r for r in outputs.values() if name == "all" or
                    (name == "metadata" and inputs[r["id"]].get("metadata")) or
                    (name.startswith("generation") and not inputs[r["id"]].get("metadata") and
                     (name == "generation_all" or name.endswith(inputs[r["id"]]["length"])))]
        observed = stats([r["elapsed_ms"] for r in selected if r["outcome"] == "completed"])
        assert group["submitted"] == len(selected)
        assert group["outcomes"] == collections.Counter(r["outcome"] for r in selected)
        assert group["within_window_completed"] == sum(r["outcome"] == "completed" and r["within_measurement"] for r in selected)
        assert all(group["success_latency_ms"][key] == observed[key] for key in ("n", "p50", "p95", "p99", "max"))
        assert group["success_mean_ms"] == observed["mean"]
    samples = [data.get(k) for k in ("idle_resource", "workload_start_resource", "workload_end_resource")] + data["resources"]
    samples = [s for s in samples if s]
    start, end = data.get("workload_start_resource"), data.get("workload_end_resource")
    delays = [(a["received_s"] - outputs[a["ingress_id"]]["sent_s"]) * 1000 for a in attempts]
    assert all(delay >= 0 for delay in delays)
    row = {"path": str(path.relative_to(ROOT)), "sha256": hashlib.sha256(path.read_bytes()).hexdigest(),
           **{key: data[key] for key in ("arm", "profile", "seed", "cap", "cost_contract")},
           "groups": data["summary"], "attempts": len(attempts),
           "upstream_outcomes": dict(collections.Counter(a["outcome"] for a in attempts)),
           "usage": dict(collections.Counter(r.get("usage", {}).get("status", "not_observed") for r in outputs.values())),
           "rss_idle_mib": data["idle_resource"]["rss_bytes"] / 2**20 if data.get("idle_resource") else None,
           "rss_max_sample_mib": max(s["rss_bytes"] for s in samples) / 2**20 if samples else None,
           "cpu_delta_s": end["cpu_seconds"] - start["cpu_seconds"] if start and end else None,
           "root_max_terminal_ms": {str(root): max(r["elapsed_ms"] for r in outputs.values() if inputs[r["id"]]["root"] == root)
                                    for root in sorted({r["root"] for r in inputs.values()})},
           "submission_to_upstream_ms": stats(delays), "load_start": data["load_start"], "load_end": data["load_end"]}
    runs.append(row)
    raw[path.parent.name] = data
    totals.update(counts)
    totals.update(submitted=len(inputs), attempts=len(attempts), warmup=len(data.get("warmup_outcomes", [])),
                  discovery=len(data["discovery_requests"]))

pairs = []
input_shapes = {}
for data in raw.values():
    shape = [{k: v for k, v in row.items() if k not in ("canonical_input_units", "canonical_output_units")}
             for row in data["submitted"]]
    key = data["profile"]
    assert key not in input_shapes or shape == input_shapes[key], data["arm"]
    input_shapes[key] = shape
for cost in ("byte_reserved", "quarter_actual"):
    reserved, actual = [raw[f"generation-{cost}-cap2-s101-deskquota_native_{arm}"] for arm in ("rr", "actual")]
    assert reserved["submitted"] == actual["submitted"]
    assert reserved["config"]["launched_binary"] == actual["config"]["launched_binary"]
    assert reserved["config"]["launched_binary"]["features"] == []
    assert differing_paths(normalize_config_snapshot(reserved["config"]["snapshot"]),
                           normalize_config_snapshot(actual["config"]["snapshot"])) == ["accounting"]
    pairs.append({"cost_contract": cost, "identical_submitted": True, "identical_native_binary": True,
                  "features": [], "config_differences": ["accounting", "ephemeral listen/upstream ports"]})

(BASE / "completed-run-summary.json").write_text(json.dumps(runs, indent=2) + "\n")
audit = {"status": "PASS_WITH_LIMITS", "runs": len(runs), "totals": dict(totals), "native_pairs": pairs,
         "input_check": "Identical request shapes/arrival/cancellation per profile across arms and cost contracts; canonical cost units excluded",
         "scope": "44 synthetic loopback runs; descriptive results, not real-provider or task-quality evidence",
         "latency_boundary": "submission_to_upstream includes transport/admission and excludes requests never attempted; not pure queue wait",
         "resource_boundary": "gateway PID samples only; ps CPU resolution is coarse; no process-tree/continuous-peak assertion"}
(BASE / "completed-run-audit.json").write_text(json.dumps(audit, indent=2) + "\n")
print(json.dumps(audit, indent=2))
