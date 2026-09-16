#!/usr/bin/env python3
"""Recalculate held local efficiency results; no gateway or provider calls."""
from collections import Counter
import hashlib
import json
from math import ceil, isclose
from pathlib import Path
from statistics import median

ROOT = Path(__file__).resolve().parents[1]
EVIDENCE = ROOT / "evidence/queued-cache-2026-09-16"
MEASUREMENTS = EVIDENCE / "measurements"


def read(name):
    return json.loads((MEASUREMENTS / name).read_text())


def band(values):
    return {"min": min(values), "median": median(values), "max": max(values)}


def check_distribution(values, reported):
    values = sorted(values)
    assert reported["n"] == len(values) and values
    for name, fraction in (("p50", .5), ("p95", .95), ("p99", .99), ("max", 1)):
        assert isclose(reported[name], values[ceil(fraction * len(values)) - 1],
                       rel_tol=1e-9, abs_tol=1e-6), (name, reported, values)


def main():
    matrix = read("queued-matrix.json")
    assert matrix["passed"] and len(matrix["runs"]) == 80
    assert len({(r["repeat"], r["case"]["name"], r["arm"]) for r in matrix["runs"]}) == 80
    hashes = {arm: r["sha256_before"] for arm, r in matrix["binaries"].items()}
    assert all(r["sha256_before"] == r["sha256_after"] for r in matrix["binaries"].values())
    result = {"passed": False, "scope": "synthetic loopback; fixed gate and service delay; not a competitor ranking",
              "binary_sha256": hashes, "matrix": {}, "no_wait": {}, "sequential_cache": {}}
    for case in matrix["cases"]:
        entry = result["matrix"][case["name"]] = {}
        for arm in ("baseline", "candidate"):
            runs = [r for r in matrix["runs"] if r["case"] == case and r["arm"] == arm]
            assert len(runs) == 5 and {r["repeat"] for r in runs} == set(range(5))
            for r in runs:
                assert r["passed"] and len(r["outcomes"]) == 10
                assert {o["id"] for o in r["outcomes"]} == set(range(10))
                assert all(o["submitted_s"] < r["gate_release_s"] < o["terminal_s"] for o in r["outcomes"])
                assert all(a["outcome"] == "completed" for a in r["attempts"])
                status, cache = r["status_after"], r["status_after"]["exact_cache"]
                assert status["upstream_attempts"] == len(r["attempts"])
                assert int(status["admission"]["rpm_debited"]) == len(r["attempts"])
                assert int(status["admission"]["tpm_debited"]) == len(r["attempts"]) * 4
                assert int(status["admission"]["tpm_held"]) == status["admission"]["active"] == status["admission"]["queue_length"] == 0
                assert cache["considered"] == cache["hits"] + cache["misses"] + sum(cache["bypasses"].values())
                assert all(o.get("payload_valid") for o in r["outcomes"] if o["outcome"] == "completed")
                completed = [o for o in r["outcomes"] if o["outcome"] == "completed"]
                check_distribution([(o["terminal_s"] - r["gate_release_s"]) * 1000 for o in completed],
                                   r["summary"]["post_release_completion_ms_success_only"])
                check_distribution([(o["terminal_s"] - o["submitted_s"]) * 1000 for o in completed],
                                   r["summary"]["ingress_completion_ms_success_only"])
                check_distribution([(o["terminal_s"] - o["submitted_s"]) * 1000 for o in r["outcomes"]],
                                   r["summary"]["ingress_terminal_ms_all_outcomes"])
            entry[arm] = {"outcomes": dict(Counter(o["outcome"] for r in runs for o in r["outcomes"])),
                          "attempts": [len(r["attempts"]) for r in runs],
                          "post_gate_success_ms": {p: band([r["summary"]["post_release_completion_ms_success_only"][p] for r in runs]) for p in ("p50", "p95", "p99")},
                          "ingress_success_p95_ms": band([r["summary"]["ingress_completion_ms_success_only"]["p95"] for r in runs]),
                          "idle_rss_mib": band([r["resources_before"]["rss_bytes"] / 2**20 for r in runs]),
                          "sampled_max_rss_mib": max(r[field]["rss_bytes"] / 2**20 for r in runs for field in ("resources_before", "resources_at_gate", "resources_after"))}
    for arm in ("baseline", "candidate"):
        runs = [read(f"no-wait-{i}-{arm}.json") for i in range(1, 6)]
        for r in runs:
            assert r["passed"] and r["binary_sha256"] == hashes[arm]
            assert r["warm_summary"]["all"]["submitted"] == 100
            assert r["warm_summary"]["all"]["outcomes"] == {"completed": 100}
            assert len(r["outcomes"]) == len(r["attempts"]) == 105
            assert all(o["outcome"] == "completed" and o["body_exact"] for o in r["outcomes"])
            check_distribution([(o["ended_s"] - o["sent_s"]) * 1000 for o in r["outcomes"] if not o["warmup"]],
                               r["warm_summary"]["all"]["success_latency_ms"])
        result["no_wait"][arm] = {"measured_completed": 500, "warmup_completed": 25,
             "p95_ms": band([r["warm_summary"]["all"]["success_latency_ms"]["p95"] for r in runs]),
             "p99_ms": band([r["warm_summary"]["all"]["success_latency_ms"]["p99"] for r in runs]),
             "idle_rss_mib": band([r["idle_resource"]["rss_bytes"] / 2**20 for r in runs])}
        runs = [read(f"sequential-cache-{i}-{arm}.json") for i in range(1, 6)]
        for r in runs:
            assert r["passed"] and r["binary_sha256"] == hashes[arm]
            assert len(r["requests"]) == 180 and all(o["status"] == 200 and o["body_exact"] for o in r["requests"])
            assert r["counts"]["failed_requests"] == r["counts"]["cancelled_requests"] == 0
            for name, reported in r["summary"].items():
                check_distribution([o["elapsed_ms"] for o in r["requests"] if f'{o["format"]}_{o["arm"]}' == name], reported)
        result["sequential_cache"][arm] = {"all_completed_including_direct": 900,
            "gateway_completed": 600, "cache_hits": 300,
            "p95_ms": {name: band([r["summary"][name]["p95"] for r in runs]) for name in runs[0]["summary"]}}
    rejection = read("rejection-head.json")
    assert rejection["passed"] and rejection["binary_sha256"] == hashes["candidate"]
    assert len(rejection["cases"]) == 6 and all(c["passed"] and c["attempts"] == 1 and c["body_preserved"] for c in rejection["cases"])
    result["rejection_head"] = {"passed_cases": 6, "gateway_cases": 5, "direct_cases": 1}
    result["matrix_totals"] = dict(Counter(o["outcome"] for r in matrix["runs"] for o in r["outcomes"]))
    result["input_sha256"] = {p.name: hashlib.sha256(p.read_bytes()).hexdigest() for p in sorted(MEASUREMENTS.glob("*.json"))}
    result["passed"] = True
    (EVIDENCE / "result-audit.json").write_text(json.dumps(result, indent=2) + "\n")
    print(json.dumps({"passed": True, "matrix_runs": 80, "matrix_totals": result["matrix_totals"], "no_wait": result["no_wait"]}, indent=2))


if __name__ == "__main__":
    main()
