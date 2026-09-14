#!/usr/bin/env python3
"""Check pinned source citations structurally, not the truth of their claims."""

import argparse
import json
import re
from pathlib import Path
from urllib.parse import unquote

ROOT = Path(__file__).resolve().parents[1]
LINK = re.compile(
    r"https://github\.com/([^/]+/[^/]+)/blob/([0-9a-f]{40})/"
    r"([^\s)#]+)#L(\d+)(?:-L(\d+))?"
)
INLINE = re.compile(r"`([^\s`:]+):(\d+(?:[–-]\d+)?(?:,\d+(?:[–-]\d+)?)*)`")


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--output", required=True, type=Path)
    args = parser.parse_args()
    rows = json.loads((ROOT / "evidence/repositories.json").read_text())
    by_name = {row["name"]: row for row in rows}
    by_remote = {
        row["remote"].removesuffix(".git").removeprefix("https://github.com/").lower(): row
        for row in rows
    }
    errors, reports = [], []

    def check(report, row, path, start, end):
        if row is None:
            errors.append(f"{report}: unknown repository")
            return
        directory = (ROOT / "references" / row["name"]).resolve()
        source = (directory / unquote(path)).resolve()
        if not source.is_relative_to(directory) or not source.is_file():
            errors.append(f"{report}: missing or invalid source path {path}")
        elif not 1 <= start <= end <= len(source.read_bytes().splitlines()):
            errors.append(f"{report}: invalid line range {path}:{start}-{end}")

    source_reports = list((ROOT / "reports").glob("*source-deep-dive.md"))
    source_reports.append(ROOT / "reports/stream-protocol-contracts.md")
    source_reports.append(ROOT / "reports/retry-classification-notes.md")
    source_reports.append(ROOT / "reports/benchmark-reference-audit.md")
    source_reports.append(ROOT / "reports/scheduler-utilization-followup.md")
    for report in sorted(source_reports):
        content = report.read_text()
        link_count = inline_count = 0
        for remote, sha, path, start, end in LINK.findall(content):
            row = by_remote.get(remote.lower())
            if row and sha != row["sha"]:
                errors.append(f"{report.name}: SHA mismatch for {remote}")
            check(report.name, row, path, int(start), int(end or start))
            link_count += 1
        # The gateway report uses a section-level commit pin with inline paths.
        if report.name == "gateway-source-deep-dive.md":
            row = None
            for line in content.splitlines():
                root = re.search(r"소스 루트: \[references/([^\]]+)\]", line)
                if root:
                    row = by_name.get(root[1])
                    if row and f"/tree/{row['sha']}" not in line:
                        errors.append(f"{report.name}: section commit pin mismatch")
                if row:
                    for path, ranges in INLINE.findall(line):
                        for span in ranges.split(","):
                            bounds = re.split("[–-]", span)
                            check(report.name, row, path, int(bounds[0]), int(bounds[-1]))
                            inline_count += 1
        reports.append({
            "report": str(report.relative_to(ROOT)),
            "pinned_line_links": link_count,
            "section_pinned_line_ranges": inline_count,
        })
    result = {
        "check": "source_citation_structure_only",
        "reports": reports,
        "passed": not errors,
        "errors": errors,
        "limitations": [
            "Does not establish interpretation accuracy, runtime behavior or performance",
            "Does not cover prose-only shorthand references or external non-source URLs",
            "Use research.py verify separately for actual Git HEAD, remote and clean state",
        ],
    }
    args.output.parent.mkdir(parents=True, exist_ok=True)
    args.output.write_text(json.dumps(result, ensure_ascii=False, indent=2) + "\n")
    print(json.dumps(result, ensure_ascii=False, indent=2))
    return int(bool(errors))


if __name__ == "__main__":
    raise SystemExit(main())
