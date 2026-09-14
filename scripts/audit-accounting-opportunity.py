#!/usr/bin/env python3
"""Describe recorded overestimation cohorts without simulating a different policy."""

import hashlib
import json
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]
PILOT = ROOT / "product/artifacts/pilot.json"
EXPECTED_PILOT_SHA = "9e6cbbb57bfaac2149eb39efaf43169dbf8e47cf708075cf83b83716615854f6"


def digest(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def main():
    assert digest(PILOT) == EXPECTED_PILOT_SHA
    manifest = json.loads(PILOT.read_text())
    registered = {row["id"]: row for row in manifest["runs"]}
    groups = []
    for arm in ("direct", "production_rr", "benchmark_fifo", "benchmark_rr"):
        runs = []
        for seed in range(1, 6):
            run_id = f"quota-seed{seed}-{arm}"
            path = PILOT.parent / "pilot-runs" / f"{run_id}.json"
            assert digest(path) == registered[run_id]["sha256"]
            data = json.loads(path.read_text())
            assert data["id"] == run_id
            submitted = {r["id"]: r for r in data["submitted"]}
            outcomes = {r["id"]: r for r in data["outcomes"]}
            cohort = {key for key, row in submitted.items()
                      if row["cost_case"] == "overestimated"}
            assert len(cohort) == 5
            attempts = []
            for attempt in data["attempts"]:
                key = attempt["ingress_id"]
                if key not in cohort:
                    continue
                assert attempt["estimated_cost"] == 3222
                assert attempt["actual_cost_fixture_units"] == 806
                result = outcomes[key]
                within = result["ended_s"] <= (
                    data["start_monotonic_s"] + data["measurement_duration_s"])
                assert result["within_measurement"] == within
                attempts.append({
                    "ingress_id": key,
                    "mock_outcome": attempt["outcome"],
                    "client_outcome": result["outcome"],
                    "within_measurement": within,
                    "estimated_cost": attempt["estimated_cost"],
                    "actual_cost_fixture_units": attempt["actual_cost_fixture_units"],
                    "received_relative_s": attempt["received_s"] - data["start_monotonic_s"],
                    "ended_relative_s": attempt["ended_s"] - data["start_monotonic_s"],
                })
            completed = [r for r in attempts if r["client_outcome"] == "completed"]
            runs.append({
                "seed": seed, "artifact": str(path.relative_to(ROOT)),
                "sha256": digest(path), "submitted": len(cohort),
                "completed_within_measurement": sum(r["within_measurement"] for r in completed),
                "completed_total": len(completed),
                "not_received_by_mock": sorted(cohort - {r["ingress_id"] for r in attempts}),
                "attempts": attempts,
            })
        groups.append({
            "arm": arm, "submitted": sum(r["submitted"] for r in runs),
            "completed_within_measurement": sum(r["completed_within_measurement"] for r in runs),
            "completed_total": sum(r["completed_total"] for r in runs),
            "runs": runs,
        })
    result = {
        "status": "PASS", "pilot_sha256": EXPECTED_PILOT_SHA,
        "groups": groups,
        "limits": [
            "Read-only reaggregation of existing synthetic results; zero new requests",
            "Per-request difference 2416 is not instantaneous reusable balance or throughput benefit",
            "Different-policy dispatch, completion, ledger timestamps and costs were not simulated",
            "Mock fixture cost is not a provider tokenizer or quota contract",
        ],
    }
    output = ROOT / "evidence/product-task8-accounting-opportunity.json"
    output.write_text(json.dumps(result, indent=2) + "\n")
    print(json.dumps({"status": "PASS", "output": str(output.relative_to(ROOT)),
                      "cohorts": [{k: v for k, v in g.items() if k != "runs"}
                                  for g in groups]}))


if __name__ == "__main__":
    main()
