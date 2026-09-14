#!/usr/bin/env python3
"""Describe saved first-pair queue samples; never simulate a new scheduler."""
from collections import Counter
import hashlib
import json
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
CHECKPOINT = ROOT / "evidence/accounting-ablation-first-pair-manifest.json"
EXPECTED_SHA = "2d279742a59f4c113535b90b8dcd31170b84963871bbf96b0e62f231ea8f09e4"


def sha(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


assert sha(CHECKPOINT) == EXPECTED_SHA
manifest = json.loads(CHECKPOINT.read_text())
arms = []
for entry in manifest["runs"]:
    path = ROOT / "product" / entry["path"]
    assert sha(path) == entry["sha256"]
    run = json.loads(path.read_text())
    submitted = {r["id"]: r for r in run["submitted"]}
    outcomes = {r["id"]: r for r in run["outcomes"]}
    assert set(submitted) == set(outcomes) and len(outcomes) == 100
    samples = [s for s in run["snapshots"] if 0 <= s["elapsed_s"] <= run["measurement_duration_s"]]
    idle_queue = [s for s in samples if s["status"]["admission"]["queue_length"] > 0
                  and s["status"]["admission"]["active"] == 0]
    overestimated = []
    for key, row in submitted.items():
        if row["cost_case"] != "overestimated":
            continue
        attempts = [a for a in run["attempts"] if a["ingress_id"] == key]
        overestimated.append({
            "id": key, "submitted_offset_s": row["offset_s"],
            "client_outcome": outcomes[key]["outcome"], "within_measurement": outcomes[key]["within_measurement"],
            "attempts": [{"received_offset_s": a["received_s"] - run["start_monotonic_s"],
                          "estimated_cost": a["estimated_cost"], "actual_fixture_cost": a["actual_cost_fixture_units"],
                          "mock_outcome": a["outcome"]} for a in attempts],
        })
    arms.append({
        "arm": run["arm"], "raw_sha256": entry["sha256"],
        "within_cutoff_status_samples": len(samples),
        "reported_blocked_reason_samples": dict(Counter(s["status"]["admission"]["blocked_reason"] or "none" for s in samples)),
        "queued_positive_active_zero_samples": len(idle_queue),
        "queued_idle_reason_samples": dict(Counter(s["status"]["admission"]["blocked_reason"] or "none" for s in idle_queue)),
        "overestimated_requests": overestimated,
        "metadata_outcomes": [{"id": key, "outcome": outcomes[key]["outcome"],
                               "within_measurement": outcomes[key]["within_measurement"], "elapsed_ms": outcomes[key]["elapsed_ms"]}
                              for key, row in submitted.items() if row.get("metadata")],
    })
report = {
    "checkpoint_sha256": EXPECTED_SHA, "observer_sha256": sha(Path(__file__)),
    "source": "Already measured, independently audited first pair; no new runtime or policy change.",
    "arms": arms,
    "limits": [
        "Status sample counts are not durations or a fraction of time; control status sampling is not continuous.",
        "The reason is the gateway's reported blocker, not an independently established causal effect size.",
        "Zero active slots with queued work does not prove that a request could safely bypass RPM/TPM/fairness constraints.",
        "Refund after a completed response cannot provide the actual cost before that request is admitted.",
        "One seed does not establish a general scheduler bottleneck or performance improvement; no counterfactual scheduler is simulated."
    ],
}
output = ROOT / "evidence/accounting-first-pair-queue-observation.json"
with output.open("x") as handle:
    json.dump(report, handle, indent=2, allow_nan=False)
    handle.write("\n")
print(json.dumps({"arms": [{k: a[k] for k in ("arm", "within_cutoff_status_samples", "reported_blocked_reason_samples", "queued_positive_active_zero_samples")} for a in arms]}))
