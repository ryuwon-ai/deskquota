"""Reclassify preserved synthetic runs; never start a gateway or call an API."""
import hashlib
import json
import math
import statistics
from collections import Counter
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
BASE = ROOT / "product/artifacts/backfill-resume-2026-09-14"
ARMS = ("benchmark_rr", "benchmark_backfill")


def stats(rows):
    values = sorted(r["elapsed_ms"] for r in rows if r["outcome"] == "completed")
    return {
        "submitted": len(rows),
        "outcomes": dict(Counter(r["outcome"] for r in rows)),
        "within_window_completed": sum(
            r["outcome"] == "completed" and r["within_measurement"] for r in rows
        ),
        "success_mean_ms": statistics.mean(values) if values else None,
        "success_p95_nearest_rank_ms": values[math.ceil(len(values) * 0.95) - 1]
        if values else None,
    }


def analyze(directory):
    manifest = json.loads((directory / "manifest.json").read_text())
    pinned = {r["path"]: r["sha256"] for r in manifest["runs"]}
    groups = ("metadata", "generation_short", "generation_long", "generation_all")
    combined = {g: {a: [] for a in ARMS} for g in groups}
    pairs = []
    sources = []
    for seed in manifest["seeds"]:
        runs = {}
        for arm in ARMS:
            path = directory / f"quota-seed{seed}-{arm}.json"
            raw = path.read_bytes()
            digest = hashlib.sha256(raw).hexdigest()
            assert digest == pinned[path.name], path
            sources.append({"path": str(path.relative_to(ROOT)), "sha256": digest})
            runs[arm] = json.loads(raw)
        left, right = (runs[a] for a in ARMS)
        assert left["submitted"] == right["submitted"]
        submitted = {r["id"]: r for r in left["submitted"]}
        assert len(submitted) == len(left["submitted"])
        outcomes = {a: {r["id"]: r for r in runs[a]["outcomes"]} for a in ARMS}
        for arm in ARMS:
            assert set(submitted) == set(outcomes[arm])
            assert len(outcomes[arm]) == len(runs[arm]["outcomes"])
        assert {i: r["outcome"] for i, r in outcomes[ARMS[0]].items()} == {
            i: r["outcome"] for i, r in outcomes[ARMS[1]].items()
        }
        ids = {
            "metadata": [i for i, r in submitted.items() if r.get("metadata", False)],
            "generation_short": [i for i, r in submitted.items()
                                 if not r.get("metadata", False) and r["length"] == "short"],
            "generation_long": [i for i, r in submitted.items()
                                if not r.get("metadata", False) and r["length"] == "long"],
            "generation_all": [i for i, r in submitted.items() if not r.get("metadata", False)],
        }
        assert sum(len(ids[g]) for g in groups[:3]) == len(submitted)
        by_group = {}
        for group in groups:
            by_group[group] = {}
            for arm in ARMS:
                rows = [outcomes[arm][i] for i in ids[group]]
                combined[group][arm].extend(rows)
                by_group[group][arm] = stats(rows)
        pairs.append({"seed": seed, "groups": by_group,
                      "upstream_attempts": {a: len(runs[a]["attempts"]) for a in ARMS}})
    return {
        "directory": str(directory.relative_to(ROOT)),
        "cap": manifest["cap"], "windows_per_run": manifest["windows"],
        "pairs": pairs,
        "pooled_descriptive": {
            g: {a: stats(rows) for a, rows in arms.items()} for g, arms in combined.items()
        },
        "sources": sources,
    }


if __name__ == "__main__":
    # Small arithmetic check; source hashes, partitions and terminal sets are checked above.
    check = stats([{"outcome": "completed", "elapsed_ms": n, "within_measurement": n < 10}
                   for n in range(1, 21)])
    assert check["success_p95_nearest_rank_ms"] == 19
    assert check["within_window_completed"] == 9
    print(json.dumps({
        "scope": "Post-hoc descriptive reanalysis of preserved synthetic runs; no new workload, "
                 "provider call, holdout, confidence interval or competitor performance claim. "
                 "Pooled requests are not independent experimental replications. "
                 "Success latency includes post-window drain; all terminal outcomes are retained.",
        "experiments": [analyze(BASE / name)
                        for name in ("repeat-cap2-window1", "negative-cap1")],
    }, indent=2))
