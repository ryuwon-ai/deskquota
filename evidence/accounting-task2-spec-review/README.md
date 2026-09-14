# Accounting Task 2 SPEC review

Verdict: **CHANGES REQUESTED — one P2 report completeness finding.** Execution is finished and remains HOLD. No new experiment or product change is required.

## P2: Report timeout outcomes by length and root

Location: `reports/accounting-ablation-results.md:40-49` (primary anchor line 42).
Requirement: `reports/accounting-ablation-plan.md:64-67` explicitly requires long/short and root completion **and timeout** outcomes; Task 2 requires long/root changes and all failures.

The report gives aggregate final timeouts of 25 reserved / 15 actual, but the subgroup table contains only fixed-time completions. The raw results show that long requests still time out 15 / 15, while short-request timeouts fall 10 / 0. All 15 final actual timeouts belong to root 0; reserved root timeouts are 16, 2, 3, 4. Thus the omitted breakdown materially distinguishes remaining long/root failure concentration from the aggregate improvement.

Focused remedy: derive length/root terminal counts from the preserved raw results and add timeout counts alongside completion counts, explicitly distinguishing fixed 300-second outcomes from final/drain outcomes. Keep submitted, cancellation, rejection and error counts visible or explicitly accounted for (including zeros), and do not present the drain change as fixed-time throughput or fairness improvement. The existing experiment, product, binary, raw files and journals need no edits or rerun. Update report-dependent HOLD hashes after the report is final.

Independent exact calculations are in `subgroup-outcomes.json`, with per-run and aggregate fixed/final outcomes. Relevant aggregate counts:

| Group | Submitted per arm | Fixed completed R/A | Fixed timeout R/A | Final completed R/A | Final timeout R/A | Final cancelled R/A | Final rejected R/A |
|---|---:|---:|---:|---:|---:|---:|---:|
| long | 100 | 25/25 | 5/5 | 33/35 | 15/15 | 50/50 | 2/0 |
| short | 400 | 260/265 | 0/0 | 364/375 | 10/0 | 25/25 | 1/0 |
| root 0 | 225 | 139/140 | 5/5 | 182/185 | 16/15 | 25/25 | 2/0 |
| root 1 | 125 | 64/65 | 0/0 | 97/100 | 2/0 | 25/25 | 1/0 |
| root 2 | 75 | 34/35 | 0/0 | 47/50 | 3/0 | 25/25 | 0/0 |
| root 3 | 75 | 48/50 | 0/0 | 71/75 | 4/0 | 0/0 | 0/0 |

Error is zero throughout these groups.

## Direct verification

- Fresh independent auditor CLI exited 0: 10 raw runs, 5 pairs, 1,000 ingress; current source identity, ordinary binary, config/TOML/runtime fingerprint, submitted schedules, terminal/attempt linkage, exact cutoff, completed-generation numeric usage formula, final empty queue and journals all passed (`independent-audit.json`). Inspected auditor imports: stdlib only, no product harness imports.
- Fresh auditor CLI rejected all five retained rehashed copied-artifact manipulations with exit 1: float usage, runtime policy mismatch, NaN end time, changed submitted schedule, missing terminal. Their original recorded checks predate first-pair acceptance (`negative-*.json`, `verification.json`). Failures preserved.
- 59 held source files match sizes/hashes and archive member bytes; 8 usage-basis files match. Retained prior test-log hashes and named usage/debt/late-terminal regression passes match. These are prior test evidence, not a new Rust test run or build attestation.
- Exact pilot/resume invocations match the plan. First checkpoint has 2 runs/1 pair with planned 5 and matrix false. Final first-pair entries and raw/journal hashes are unchanged. Logs have 2 pilot starts and exactly 8 resume starts. Final matrix has alternating order and no prior control outcome reuse.
- Recorded startup holds are 61.1431–61.1503 seconds. Measurement-end observations are 300.0004–300.0017 seconds; completion cutoff uses exactly start + 300, not measurement-end overshoot.
- Seed fixed completions independently audit to 57/58 each, total 285/290. All final outcomes, drain counts, usage and success-only latency reconcile with the report. Report correctly limits five seeds to timing-jitter repeats of one synthetic workload and rejects actual-provider, internal-refund, real-agent, low-end, competitor and default-switch conclusions.
- Previous failed final-verification and corrected reread remain preserved. Cleanup is supported by raw cleanup flags, terminal journals, final inactive/empty queue, and retained owner/parent PID-port-temp evidence; this reviewer did not rerun the gateway or independently remeasure runtime resources.
- All 104 files fingerprinted before review are byte-identical afterward (`preservation.json`). The reviewer wrote only this review evidence directory and did not edit product, report, source archive, held binaries, original pilot or raw accounting artifacts.

No additional SPEC finding was established. Resolve the report finding, then obtain fresh SPEC rereview followed by QUALITY before Native wizard work.
