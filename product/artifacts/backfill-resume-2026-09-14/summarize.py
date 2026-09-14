"""Derive descriptive per-pair latency; seeds, not individual requests, are repeats."""
import json
from pathlib import Path
import statistics
import sys

from audit import audit


def summarize(directory):
    checked = audit(directory)
    manifest = json.loads((directory / "manifest.json").read_text())
    pairs = []
    for pair in manifest["pairs"]:
        runs = [json.loads((directory / f"quota-seed{pair['seed']}-{arm}.json").read_text())
                for arm in ("benchmark_rr", "benchmark_backfill")]
        assert runs[0]["submitted"] == runs[1]["submitted"]
        outcomes = [{row["id"]: row for row in run["outcomes"]} for run in runs]
        assert {key: row["outcome"] for key, row in outcomes[0].items()} == {key: row["outcome"] for key, row in outcomes[1].items()}
        groups = {}
        for length in ("short", "long"):
            ids = [row["id"] for row in runs[0]["submitted"] if row["length"] == length]
            completed = [key for key in ids if outcomes[0][key]["outcome"] == "completed"]
            means = [statistics.mean(rows[key]["elapsed_ms"] for key in completed) for rows in outcomes]
            deltas = [outcomes[1][key]["elapsed_ms"]-outcomes[0][key]["elapsed_ms"] for key in completed]
            groups[length] = {
                "submitted_per_arm": len(ids), "completed_same_ids_per_arm": len(completed),
                "baseline_mean_ms": means[0], "candidate_mean_ms": means[1],
                "mean_reduction_percent": (means[0]-means[1])/means[0]*100,
                "candidate_minus_baseline_mean_ms": statistics.mean(deltas),
                "faster_by_at_least_1s": sum(delta <= -1000 for delta in deltas),
                "slower_by_at_least_1s": sum(delta >= 1000 for delta in deltas),
                "p95_success_ms": [run["summary"]["length:"+length]["completion_ms_success_only"]["p95"] for run in runs],
                "all_terminal_mean_ms": [statistics.mean(rows[key]["elapsed_ms"] for key in ids) for rows in outcomes],
            }
        pairs.append({"seed": pair["seed"], "groups": groups,
                      "fixed_completed_per_arm": [r["summary"]["all"]["within_measurement_completed"] for r in runs],
                      "same_terminal_outcome_for_every_id": True})
    return {"directory": directory.name, "windows": manifest["windows"], "cap": manifest["cap"],
            "pairs": pairs, "audit_passed": checked["passed"], "elapsed_s": manifest["elapsed_s"],
            "scope": "Descriptive synthetic pair results; success sets explicitly equal; cancellations remain in all-terminal denominator. No CI, holdout, provider or agent-task claim."}


if __name__ == "__main__":
    print(json.dumps(summarize(Path(sys.argv[1])), indent=2))
