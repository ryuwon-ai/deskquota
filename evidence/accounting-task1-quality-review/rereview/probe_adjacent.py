"""Focused rereview driver: unchanged original triggers, then nearby boundaries."""
import copy
import json
import math
from pathlib import Path
import runpy
import sys

sys.dont_write_bytecode = True
OUT = Path(__file__).resolve().parent
ns = runpy.run_path(str(OUT / "replay_original.py"))
benchmark = ns["benchmark"]
accounting = ns["accounting"]
ROOT = ns["ROOT"]
original = ns["source_runs"]
replay = json.loads((OUT / "probe-resume.json").read_text())
assert replay["cases"][0]["accepted"]
assert not any(case["accepted"] for case in replay["cases"][1:])

cases = []


def check(name, run, accepted, message=None):
    try:
        accounting.validate_accounting_run(run)
        result = {"case": name, "accepted": True}
    except ValueError as error:
        result = {"case": name, "accepted": False, "error": str(error)}
    cases.append(result)
    assert result["accepted"] is accepted, result
    if message:
        assert message in result.get("error", ""), result


for status_field in ("initial_status", "final_status"):
    run = copy.deepcopy(original[1])
    run.pop(status_field)
    check(status_field + "_missing", run, False, status_field)
    run = copy.deepcopy(original[1])
    run[status_field] = None
    check(status_field + "_null", run, False, status_field)
    for nested, field, value, error in (
        ("identity", "fingerprint", "0" * 64, "fingerprint"),
        ("admission", "accounting", "reserved", "accounting"),
    ):
        run = copy.deepcopy(original[1])
        run[status_field][nested][field] = value
        check(status_field + "_" + field, run, False, error)

for field in ("start_monotonic_s", "measurement_duration_s", "ended_s"):
    for label, value in (("nan", math.nan), ("positive_inf", math.inf), ("negative_inf", -math.inf), ("bool", True)):
        run = copy.deepcopy(original[1])
        if field == "ended_s":
            run["outcomes"][0][field] = value
        else:
            run[field] = value
        check(field + "_" + label, run, False, "finite")

run = copy.deepcopy(original[1])
run.update(start_monotonic_s=1e308, measurement_duration_s=1e308)
check("finite_float_addition_overflow", run, False, "cutoff must be finite")
run = copy.deepcopy(original[1])
run.update(start_monotonic_s=10**400, measurement_duration_s=3.0)
check("integer_float_conversion_overflow", run, False, "cutoff must be finite")

for label, offset, within in (("exact", 0, True), ("just_after", 0.000001, False)):
    run = copy.deepcopy(original[1])
    row = next(row for row in run["outcomes"] if row["outcome"] == "completed")
    row.update(ended_s=run["start_monotonic_s"] + 3 + offset, within_measurement=within)
    check("cutoff_" + label, run, True)

manifest_path = ROOT / "product/artifacts/accounting-ablation/quality-fix/smoke-final.json"
manifest = json.loads(manifest_path.read_text())
held_smoke = []
for entry in manifest["runs"]:
    path = Path(entry["path"])
    if not path.is_absolute():
        path = ROOT / "product" / path
    assert benchmark.digest(path) == entry["sha256"]
    run = json.loads(path.read_text())
    check("current_held_smoke_" + run["arm"], run, True)
    for status_field in ("initial_status", "final_status"):
        status = run[status_field]
        assert status["identity"]["fingerprint"] == run["config"]["raw_toml_sha256"]
        assert status["admission"]["accounting"] == run["config"]["accounting"]
    held_smoke.append(run)
evaluation = accounting.paired_summary(held_smoke, [1], "smoke")
assert evaluation == manifest["evaluation"]
summary = {
    "original_replay": {"cases": 7, "positive_controls": 1, "negative_rejections": 6},
    "adjacent_cases": cases,
    "current_held_smoke_evaluation_matches": True,
    "current_held_smoke_sha256": benchmark.digest(manifest_path),
    "evidence_class": "direct copied-artifact/pure validation; smoke execution is retained implementer evidence",
    "owned_children_spawned": [],
    "owned_network_listeners": [],
    "execution_finished": True,
    "disposition": "HOLD",
}
benchmark.write_json(OUT / "adjacent-results.json", summary)
print(json.dumps({"original_cases_passed": 7, "adjacent_cases_passed": len(cases), "held_smoke_evaluation_matches": True}, indent=2))
