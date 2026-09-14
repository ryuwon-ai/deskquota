#!/usr/bin/env python3
"""Pair the existing native HTTP harness; never change its historical schedule."""
import argparse
import asyncio
import copy
import json
import platform
import sys
import time
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT / "product/scripts"))
import benchmark as bench


def paired_result(baseline, candidate):
    if (baseline["arm"], candidate["arm"]) != ("benchmark_rr", "benchmark_backfill"):
        raise ValueError("unexpected paired arms")
    if baseline["submitted"] != candidate["submitted"]:
        raise ValueError("paired workload differs")
    a, b = baseline["config"], candidate["config"]
    if a["launched_binary"] != b["launched_binary"]:
        raise ValueError("paired benchmark binary differs")
    for field in ("quota", "concurrency", "roots", "accounting", "cancel_policy", "retry_transient_429"):
        if a[field] != b[field]:
            raise ValueError("paired config differs: " + field)
    groups = {}
    for group in ("all", "length:short", "length:long"):
        if group not in baseline["summary"]:
            continue
        groups[group] = {
            arm: run["summary"][group]
            for arm, run in (("baseline", baseline), ("candidate", candidate))
        }
    return {"seed": baseline["seed"], "groups": groups,
            "fixed_window_completion_delta": candidate["summary"]["all"]["within_measurement_completed"] - baseline["summary"]["all"]["within_measurement_completed"],
            "scope": "synthetic fixture requests, not agent task goodput or provider tokens"}


def self_check():
    run = {"arm": "benchmark_rr", "seed": 1, "submitted": [{"id": "synthetic"}],
           "config": dict.fromkeys(("quota", "concurrency", "roots", "accounting", "cancel_policy", "retry_transient_429", "launched_binary"), 1),
           "summary": {"all": {"within_measurement_completed": 1}}}
    candidate = copy.deepcopy(run)
    candidate["arm"] = "benchmark_backfill"
    assert paired_result(run, candidate)["fixed_window_completion_delta"] == 0
    for mutate in (lambda r: r["submitted"].clear(),
                   lambda r: r["config"].update(concurrency=2),
                   lambda r: r["config"].update(launched_binary=2),
                   lambda r: r.update(arm="benchmark_rr")):
        broken = copy.deepcopy(candidate)
        mutate(broken)
        try:
            paired_result(run, broken)
        except ValueError:
            continue
        raise AssertionError("mismatched pair accepted")
    print("PASS: equal pair accepted; workload/config/binary/arm mismatches rejected; no gateway executed")


async def main(args):
    seeds = [int(s) for s in args.seeds.split(",")]
    if not seeds or len(set(seeds)) != len(seeds) or len(seeds) > 5:
        raise ValueError("one to five unique seeds required")
    binary, reference = args.binary.resolve(strict=True), args.reference.resolve(strict=True)
    directory = args.output.resolve()
    directory.mkdir(parents=True, exist_ok=False)
    # ponytail: one sequential experiment process; this existing harness constant
    # changes only the explicit cap1 negative control, never a product setting.
    bench.CAP = args.cap
    identities = {"doctor_binary": {"path": str(binary), "sha256": bench.digest(binary)},
                  "benchmark_binary": {"path": str(reference), "sha256": bench.digest(reference)},
                  "runner_sha256": bench.digest(Path(__file__))}
    scripts = [ROOT / "product/scripts" / name for name in ("benchmark.py", "benchmark_http.py", "benchmark_accounting.py")]
    identities["harness"] = {str(p.relative_to(ROOT)): bench.digest(p) for p in scripts}
    manifest = {"status": "running", "identities": identities, "host": platform.platform(),
                "seeds": seeds, "windows": args.windows, "cap": args.cap, "runs": [], "pairs": [],
                "scope": "unchanged existing composite workload; cap1 is negative control; no new held-out workload yet",
                "runtime_budget_s": 2700, "started_utc": time.strftime("%Y-%m-%dT%H:%M:%SZ", time.gmtime())}
    bench.write_json(directory / "manifest.json", manifest)
    started = time.monotonic()
    try:
        async with asyncio.timeout(2700):
            for index, seed in enumerate(seeds):
                arms = ("benchmark_rr", "benchmark_backfill")
                if index % 2:
                    arms = arms[::-1]
                runs = {}
                for arm in arms:
                    if bench.digest(reference) != identities["benchmark_binary"]["sha256"]:
                        raise ValueError("benchmark binary changed during experiment")
                    print(json.dumps({"event": "run_start", "seed": seed, "arm": arm}), flush=True)
                    run = await bench.run_arm(arm, seed, args.windows, binary, reference, "quota", directory)
                    runs[arm] = run
                    path = directory / (run["id"] + ".json")
                    manifest["runs"].append({"id": run["id"], "path": path.name, "sha256": bench.digest(path)})
                    bench.write_json(directory / "manifest.json", manifest)
                    print(json.dumps({"event": "run_done", "seed": seed, "arm": arm,
                                      "outcomes": run["summary"]["all"]["outcomes"],
                                      "fixed_completed": run["summary"]["all"]["within_measurement_completed"]}), flush=True)
                pair = paired_result(runs["benchmark_rr"], runs["benchmark_backfill"])
                manifest["pairs"].append(pair)
                bench.write_json(directory / "manifest.json", manifest)
                print(json.dumps({"event": "pair_done", "seed": seed,
                                  "fixed_completion_delta": pair["fixed_window_completion_delta"]}), flush=True)
        manifest["status"] = "completed_pilot" if len(seeds) < 5 else "completed_five_pairs"
    except BaseException as error:
        manifest.update(status="failed", error_type=type(error).__name__)
        raise
    finally:
        manifest["elapsed_s"] = time.monotonic() - started
        manifest["harness_unchanged"] = all(bench.digest(p) == identities["harness"][str(p.relative_to(ROOT))] for p in scripts)
        bench.write_json(directory / "manifest.json", manifest)


if __name__ == "__main__":
    if sys.argv[1:] == ["--self-check"]:
        self_check()
        raise SystemExit(0)
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--binary", required=True, type=Path)
    parser.add_argument("--reference", required=True, type=Path)
    parser.add_argument("--output", required=True, type=Path)
    parser.add_argument("--seeds", default="1")
    parser.add_argument("--windows", type=int, choices=(1, 2), default=2)
    parser.add_argument("--cap", type=int, choices=(1, 2), default=2)
    asyncio.run(main(parser.parse_args()))
