"""Review-only copied-artifact probes; no gateway, build, or model execution."""
import asyncio
import copy
import hashlib
import json
import math
import os
from pathlib import Path
import sys

ROOT = Path(__file__).resolve().parents[2]
OUT = Path(__file__).resolve().parent
sys.dont_write_bytecode = True
sys.path.insert(0, str(ROOT / "product/scripts"))
import benchmark
import benchmark_accounting as accounting

source_path = ROOT / "product/artifacts/accounting-ablation/spec-fix/smoke-final.json"
source_manifest = json.loads(source_path.read_text())
source_runs = []
for entry in source_manifest["runs"]:
    path = Path(entry["path"])
    if not path.is_absolute():
        path = ROOT / "product" / path
    assert benchmark.digest(path) == entry["sha256"]
    source_runs.append(json.loads(path.read_text()))


def runtime_accounting(runs):
    runs[1]["initial_status"]["admission"]["accounting"] = "reserved"


def runtime_fingerprint(runs):
    runs[1]["initial_status"]["identity"]["fingerprint"] = "0" * 64


def relabel_reserved(runs):
    original_actual_id = runs[1]["id"]
    candidate = copy.deepcopy(runs[0])
    candidate.update(id=original_actual_id, arm="production_actual")
    old = candidate["config"]
    _, config = accounting.build_gateway_config(
        arm="production_actual", listen_port=old["listen_port"],
        upstream_port=old["upstream_port"], quota=old["quota"],
        launched_binary=old["launched_binary"],
    )
    config["started_monotonic_s"] = old["started_monotonic_s"]
    candidate["config"] = config
    runs[1] = candidate


def nan_terminal(runs):
    outcome = next(row for row in runs[1]["outcomes"] if row["outcome"] == "completed")
    outcome.update(ended_s=float("nan"), within_measurement=False)
    runs[1]["summary"] = benchmark.aggregate(runs[1])


def bad_usage(runs):
    outcome = next(row for row in runs[1]["outcomes"] if row["usage"]["status"] == "observed")
    outcome["usage"]["prompt_tokens"] = float(outcome["usage"]["prompt_tokens"])


def bad_cutoff_flag(runs):
    runs[1]["outcomes"][0]["within_measurement"] = False


results = []
for name, mutate in [
    ("unchanged_control", lambda runs: None),
    ("runtime_accounting_mismatch", runtime_accounting),
    ("runtime_fingerprint_mismatch", runtime_fingerprint),
    ("reserved_run_relabelled_actual", relabel_reserved),
    ("nonfinite_terminal_nan", nan_terminal),
    ("equal_float_usage_control", bad_usage),
    ("false_cutoff_flag_control", bad_cutoff_flag),
]:
    runs = copy.deepcopy(source_runs)
    mutate(runs)
    directory = OUT / name / "runs"
    directory.mkdir(parents=True, exist_ok=False)
    manifest = copy.deepcopy(source_manifest)
    for entry, run in zip(manifest["runs"], runs):
        path = directory / (entry["id"] + ".json")
        benchmark.write_json(path, run)
        entry.update(path=str(path), sha256=benchmark.digest(path), summary=run["summary"])
        entry["provenance"] = {
            key: run["config"][key]
            for key in ("accounting", "raw_toml_sha256", "launched_binary")
        }
    benchmark.write_json(directory.parent / "manifest.json", manifest)
    result = {"case": name, "fixture": "copied smoke artifacts, explicitly mutated; not a new benchmark"}
    try:
        accepted = benchmark.resume_preflight(
            manifest, directory, manifest["schedule"], "accounting", 5
        )
        evaluation = accounting.paired_summary(list(accepted.values()), [1], "smoke")
        result.update(accepted=True, resumed_runs=len(accepted), evaluation=evaluation)
    except (ValueError, KeyError, TypeError) as error:
        result.update(accepted=False, error=type(error).__name__ + ": " + str(error))
    result["runtime_accounting"] = runs[1]["initial_status"]["admission"]["accounting"]
    result["declared_accounting"] = runs[1]["config"]["accounting"]
    result["runtime_fingerprint_matches"] = (
        runs[1]["initial_status"]["identity"]["fingerprint"]
        == runs[1]["config"]["raw_toml_sha256"]
    )
    results.append(result)

summary = {
    "source_manifest": str(source_path.relative_to(ROOT)),
    "source_manifest_sha256": benchmark.digest(source_path),
    "cases": results,
    "owned_children_spawned": [],
    "owned_network_listeners": [],
    "temporary_paths_created": [],
    "retained_output_directory": str(OUT),
    "execution_finished": True,
    "disposition": "HOLD",
}
benchmark.write_json(OUT / "probe-resume.json", summary)
print(json.dumps([
    {key: row[key] for key in ("case", "accepted")}
    | ({"delta": row["evaluation"]["pairs"][0]["actual_minus_reserved_within_measurement_completed"]}
       if row["accepted"] else {"error": row["error"]})
    for row in results
], indent=2))
