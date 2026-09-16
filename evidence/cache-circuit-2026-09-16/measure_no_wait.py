"""Reuse the existing no-wait harness for five alternating binary pairs."""
import argparse
import asyncio
from pathlib import Path
import sys

ROOT = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(ROOT / "product/scripts"))
from benchmark import digest, run_arm, write_json


async def main(args):
    args.output.mkdir(parents=True, exist_ok=False)
    binaries = {arm: getattr(args, arm).resolve(strict=True) for arm in ("baseline", "candidate")}
    report = {"scope": "synthetic cache-off no-wait; 100 measured plus 5 warmup requests per run",
              "binaries": {arm: {"sha256": digest(path), "bytes": path.stat().st_size} for arm, path in binaries.items()},
              "runs": []}
    for pair in range(5):
        for arm in (("baseline", "candidate") if pair % 2 == 0 else ("candidate", "baseline")):
            directory = args.output / f"{pair + 1}-{arm}"
            directory.mkdir()
            run = await run_arm("production_rr", pair + 1, 1, binaries[arm], None, "no_wait", directory)
            assert len(run["outcomes"]) == 100
            assert all(r["outcome"] == "completed" for r in run["outcomes"])
            report["runs"].append({"pair": pair + 1, "arm": arm, "summary": run["summary"],
                                   "idle_resource": run["idle_resource"], "run_file": str(directory / f"no_wait-seed{pair+1}-production_rr.json")})
            print(f"pair={pair+1} arm={arm} completed=100/100", flush=True)
    for arm, path in binaries.items():
        assert digest(path) == report["binaries"][arm]["sha256"]
    report["passed"] = True
    write_json(args.output / "summary.json", report)


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--baseline", type=Path, required=True)
    parser.add_argument("--candidate", type=Path, required=True)
    parser.add_argument("--output", type=Path, required=True)
    asyncio.run(main(parser.parse_args()))
