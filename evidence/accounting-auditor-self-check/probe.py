#!/usr/bin/env python3
"""Exercise the independent reader on retained copies; no product import or IO."""
from copy import deepcopy
import importlib.util
import json
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
spec = importlib.util.spec_from_file_location("independent_audit", ROOT / "scripts/audit-accounting-results.py")
audit = importlib.util.module_from_spec(spec)
spec.loader.exec_module(audit)
source = ROOT / "product/artifacts/accounting-ablation/spec-fix/smoke-final.json"
manifest = audit.load(source)
originals = [audit.load(ROOT / "product" / row["path"]) for row in manifest["runs"]]
cases = []


def check(name, mutation=None):
    run = deepcopy(originals[1])
    if mutation:
        mutation(run)
    try:
        result, usage = audit.audit_run(run, originals[1]["submitted"], manifest["identity"]["production"], phase="smoke")
        accepted, error = True, None
    except (ValueError, KeyError, TypeError) as exc:
        accepted, error = False, str(exc)
    cases.append({"name": name, "accepted": accepted, "expected_accepted": mutation is None, "error": error})
    assert accepted is (mutation is None), cases[-1]


def first_generation(run):
    return next(row for row in run["outcomes"] if row["usage"]["status"] == "observed")


check("unmodified_actual_with_started20_received19")
for field in ("initial_status", "final_status"):
    check(field + "_accounting", lambda run, field=field: run[field]["admission"].update(accounting="reserved"))
    check(field + "_fingerprint", lambda run, field=field: run[field]["identity"].update(fingerprint="0" * 64))
check("equal_float_usage", lambda run: first_generation(run)["usage"].update(prompt_tokens=float(first_generation(run)["usage"]["prompt_tokens"])))
check("missing_completed_usage", lambda run: first_generation(run).update(usage={"status": "not_observed"}))
check("nonfinite_ended", lambda run: first_generation(run).update(ended_s=float("nan"), within_measurement=False))
check("false_cutoff_flag", lambda run: first_generation(run).update(within_measurement=False))
check("missing_terminal", lambda run: run["outcomes"].pop())
check("unlinked_attempt", lambda run: run["attempts"][0].update(ingress_id="unknown"))
check("changed_schedule", lambda run: run["submitted"][0].update(root=3))
check("changed_api_path", lambda run: run["config"]["snapshot"]["upstream"].update(api_base="http://127.0.0.1:1234/changed"))
summaries = []
for run in originals:
    result, usage = audit.audit_run(run, run["submitted"], manifest["identity"]["production"], phase="smoke")
    assert result == manifest["evaluation"]["pairs"][0]["arms"][run["arm"]]
    summaries.append({"arm": run["arm"], "summary_matches": True, "usage": usage})
report = {
    "passed": True, "auditor_sha256": audit.sha(ROOT / "scripts/audit-accounting-results.py"),
    "probe_sha256": audit.sha(Path(__file__)), "source_manifest_sha256": audit.sha(source),
    "scope": "Retained three-second smoke copies; independent audit_run only. Full quota CLI gate is not executed yet.",
    "cases": cases, "summary_checks": summaries, "gateway_or_model_executed": False,
    "development_corrections": [
        "Initial reader incorrectly named mock disconnection outcome cancelled; actual schema is disconnected. The first positive control rejected it; corrected the auditor, not product.",
        "Initial reader incorrectly required gateway_started_attempts equal received attempts; retained actual smoke has20started/19received due to cancellation before wire. Corrected to independent bounded counts, not product."
    ]
}
output = Path(__file__).with_name("result.json")
with output.open("x") as handle:
    json.dump(report, handle, indent=2, allow_nan=False)
    handle.write("\n")
print(json.dumps({"passed": True, "cases": len(cases), "paired_summaries": len(summaries)}))
