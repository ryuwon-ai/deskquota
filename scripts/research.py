#!/usr/bin/env python3
"""Read-only research status and artifact integrity checks, not product tests."""

import argparse
import hashlib
import json
import subprocess
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
EXPECTED_REPOS = {
    "overlaat", "promptforge", "obleth-gateway", "litellm", "bifrost",
    "smg", "llama-swap", "MindRouter", "pi", "hermes-agent",
    "codex", "ollama", "llama.cpp", "vllm", "sglang", "LMCache",
    "tensorzero", "portkey-gateway", "any-llm", "llm-d-router",
    "helicone-ai-gateway", "otari", "blitz-router", "LAPS", "FastServe", "dynamo",
    "bifrost-benchmarking",
}
EXPECTED_PAPERS = {"lmetric-osdi26", "laps-mlsys26", "fastserve-nsdi26"}


def read_json(path):
    return json.loads((ROOT / path).read_text(encoding="utf-8"))


def git(repo, *args):
    return subprocess.check_output(
        ["git", "-C", str(repo), *args], text=True, stderr=subprocess.PIPE
    ).strip()


def verify():
    errors = []
    rows = read_json("evidence/repositories.json")
    if len(rows) != len(EXPECTED_REPOS) or {r["name"] for r in rows} != EXPECTED_REPOS:
        errors.append("Repository manifest must contain the 27 research targets exactly once")
    for row in rows:
        repo = ROOT / "references" / row["name"]
        try:
            for actual, expected, label in [
                (git(repo, "rev-parse", "HEAD"), row["sha"], "SHA"),
                (git(repo, "remote", "get-url", "origin"), row["remote"], "remote"),
                (git(repo, "status", "--porcelain"), "", "working tree"),
            ]:
                if actual != expected:
                    errors.append(f"{row['name']}: {label} differs from recorded state")
        except (OSError, subprocess.CalledProcessError) as exc:
            errors.append(f"{row['name']}: Git inspection failed ({type(exc).__name__})")

    papers = read_json("papers/manifest.json")
    if len(papers) != len(EXPECTED_PAPERS) or {p["name"] for p in papers} != EXPECTED_PAPERS:
        errors.append("Paper manifest must contain the three reviewed PDFs exactly once")
    for paper in papers:
        path = ROOT / "papers" / (paper["name"] + ".pdf")
        if not path.is_file():
            errors.append(f"{paper['name']}: PDF missing")
        elif hashlib.sha256(path.read_bytes()).hexdigest() != paper["sha256"]:
            errors.append(f"{paper['name']}: PDF hash mismatch")

    items = read_json("evidence/work-items.json")["items"]
    ids = [item["id"] for item in items]
    if len(ids) != len(set(ids)):
        errors.append("Duplicate work item IDs")
    for item in items:
        if item["status"] not in {"done", "in_progress", "pending"}:
            errors.append(f"{item['id']}: invalid status")
        if item["status"] == "done" and not item["evidence"]:
            errors.append(f"{item['id']}: completed item has no evidence pointer")
        for evidence in item["evidence"]:
            if not (ROOT / evidence).is_file():
                errors.append(f"{item['id']}: missing evidence file {evidence}")

    report = {
        "check": "research_artifact_integrity_only",
        "passed": not errors,
        "repository_count": len(rows),
        "paper_count": len(papers),
        "done_items": sum(i["status"] == "done" for i in items),
        "unfinished_items": sum(i["status"] != "done" for i in items),
        "errors": errors,
        "limitations": [
            "Does not verify the truth or sufficiency of source interpretations",
            "Does not run product, competitor, API, or Windows benchmarks",
            "Does not prove the overall project goal complete",
        ],
    }
    print(json.dumps(report, ensure_ascii=False, indent=2))
    return 0 if not errors else 1


def status():
    data = read_json("evidence/work-items.json")
    print(data["objective"])
    for item in data["items"]:
        print(f"[{item['status']}] {item['id']}: {item['title']}")
        if item["status"] != "done":
            print(f"  next: {item['next_action']}")
    print("Status is a work record; verify underlying evidence before claiming completion.")
    return 0


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("command", choices=["verify", "status"])
    args = parser.parse_args()
    try:
        return verify() if args.command == "verify" else status()
    except (OSError, ValueError, KeyError, TypeError) as exc:
        print(f"Research records could not be checked: {exc}", file=sys.stderr)
        return 1


if __name__ == "__main__":
    sys.exit(main())
