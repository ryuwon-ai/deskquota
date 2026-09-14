# Core Task 8 independent SPEC review

**SPEC PASS.** No required change was found in the bounded Task8 pilot. This is a
specification verdict only; it does not make a QUALITY, production readiness,
performance superiority, native lifecycle or whole-project completion judgment.

Review date: 2026-09-12. Normative scope: `docs/superpowers/plans/2026-09-12-gateway-core.md:122-130`,
core design sections 6–8, and the bounded application of `reports/benchmark-spec.md`.
`evidence/task8-review-brief.md` was used as navigation, not an acceptance result.
Root/research AGENTS, README, HARNESS and work-items were read. There is no Git
repository at research/product and no product README. No Git mutation occurred.

## Requirement coverage

| Requirement | Source / direct evidence | Verdict |
|---|---|---|
| Required-feature FIFO/RR example, common production transport, ordinary production RR | `product/Cargo.toml:10-15`, `examples/bench_gateway.rs:10-30`, `src/server.rs:290-345`, `src/admission/queue.rs:126-166`. Task7 archive diff shows unchanged transport, estimator, CLI, config and lockfile. Fresh binary help checks show `--policy` only in the example. | Covered |
| Real-body HTTP estimated costs; exact costs only in separate virtual traces | `scripts/benchmark_http.py:139-147,205-218`; unchanged `src/protocol/request.rs`. Independent reconstructed body lengths, estimated costs and actual fixture costs agree for all original received attempts. `tests/benchmark_trace.rs:1-244` uses ManualClock/exact_fixture and no HTTP. Fresh default 3 / feature 4 traces pass. | Covered |
| Four arms, five seeds, five real quota windows and complete failures/pending | `scripts/benchmark.py:112-128,204-335,344-400`. Original manifest contains exactly 40 unique runs (20 no-wait, 20 quota), each quota run has 100 submissions and a 300-second cutoff. All 40 SHA/summary identities, fixture schedules, terminal denominators and reconstructed costs were independently checked. Separate quota auditor checks pending snapshot denominators and timing. | Covered |
| Harness rejects denominator loss/duplicate attempts and validates payload/EOF | `scripts/benchmark.py:6-46,447-550`; `scripts/benchmark_http.py:43-94,218-236`. Fresh self-check passes; independent loopback probes accept two valid responses and reject six wrong/malformed/truncated responses. Four-arm smoke preserves all 80 terminal outcomes. | Covered |
| Release identity, feature identity, workflow outcomes, distributions, RSS, lengths, unused budget and retry amplification | Original `artifacts/pilot.json`, all 40 run files, `benchmark.py:85-110,337-342,402-444`. Original build logs reviewed; binary SHAs independently match before/after. Raw quota totals and paired no-wait p95/RSS recomputed. Counts include drain and rejected/timeout/cancelled outcomes. | Covered |
| Separate tentative overhead/RSS decision; no exaggerated superiority | `docs/benchmark-method.md:45-57,115-173`; `reports/benchmark-pilot-results.md:23-87`. Paired extra p95 and idle RSS separately evaluated; negatives retained. Empty-ledger/M4 limitation explicit. Fixed-time throughput loss and artificial independent response-pair definition explicit. | Covered |
| Required fmt/clippy/default suite and remaining proof boundaries | Existing `artifacts/task8-development/{fmt,clippy-default,clippy-feature,test-default}.log` reviewed; default log sums to 191 passing Rust tests. No unchanged full-suite repeat. Method, results and work-items retain OS/client/real-task gaps. | Covered |

Missing bounded Task8 requirements: none found. Extra implementation is the small
`benchmark_http.py` fixture/client helper, bounded `admission.retained` status
field, and post-measurement resume integrity self-check. These support the task;
there is no deficit scheduler, competitor execution, native setup or added runtime dependency.

The broad research workload catalog is future context, not a demand that this
pilot implement every agent count, failure mode, provider or platform. The pilot
does not establish populated-known-ledger pure forwarding overhead, internal
gateway queue timestamps, provider token accuracy, actual task quality, low-end
PC performance, or Windows/Linux runtime behavior. These are explicitly bounded
unknowns, not hidden successful tests.

## What this reviewer actually executed

| Execution | Actual count and result |
|---|---|
| `benchmark.py --self-check` | Six malformed-record rejections, three quota mock HTTP requests (2 complete/1 reject), one missing-marker HTTP request, one truncated-trailer HTTP request, drain aggregation check, valid resume and four resume-integrity rejections. All passed. Five actual loopback HTTP requests; no gateway arm launched by resume checks. |
| Independent `probes.py` | Eight actual loopback HTTP requests: valid generation/metadata, wrong generation/metadata, missing marker, malformed JSON, truncated content-length, truncated chunk trailer. 8/8 expected results. |
| Four-arm `--smoke --seeds 31 --windows 1` | 80 ingress, 68 completed and 12 cancelled. Mock receives: direct20, production19, FIFO19, benchmarkRR19 (77 total). Each gateway separately reports20 starts: the three start/receive gaps are preserved, not invented into received attempts. Smoke uses unlimited quota/compressed arrival and is not a quota/performance rerun. |
| Exact trace builds in reviewer-owned offline target | Default3 + feature4 = 7 passing tests. Logs preserved; reviewer build directory removed. Product release/benchmark binaries were not rebuilt. |
| Original artifact inspections | 40 runs / 4,000 measured submissions / 100 separate warmups; 20 quota runs additionally inspected by the existing standalone parent auditor. These are fresh read-only audits, not 4,000 new executions or raw historical payload revalidation. |
| CLI help | Two process invocations; ordinary production run has no policy selector, feature example does. |

The original full ~139-minute matrix, 191-test default suite, fmt/clippy and release
builds are **existing logs/source/artifact evidence**, not fresh reviewer runs.
Preserved RED logs were inspected; their pre-fix failures were not re-executed.
`resume-red.log` is the implementer's uncanonicalized-path fixture error; the
canonical-path RED is the actual behavior failure. The final self-check verifies
that corrected branch. A reviewer one-off budget audit initially used research
cwd for product-relative artifact paths and failed with FileNotFoundError; the
corrected audit is recorded in `budget-and-lineage-audit.json`. Neither changed
product or original measurement data.

## Independent numerical observations

Fixed 300-second completion totals for 500 submitted per arm are direct328,
productionRR285, FIFO285, benchmarkRR284. Production399 total completions include
114 after the cutoff; its remaining outcomes are 1 reject, 25 timeout and 75 cancel.
This is not a throughput improvement. The predetermined independent response-pair
grouping is a fixture metric, not an agent/tool task or general superiority claim.

Production paired extra completion p95 across five seeds is
0.1985000017–0.3479580009 ms and sampled idle RSS8.625–8.6875 MiB: within the
tentative2ms/50MiB budgets **only in the M4 no-wait empty-ledger phase**. All no-wait
status snapshots retain0 ledger entries. The two original forwarded mock429s are
retained; this review does not establish their gateway-reservation timing cause.

## Identity and cleanup

- Task7 archive matched all41 entries of its accepted manifest. Only five existing
  files changed and nine files were added for Task8; see `task7-diff.patch`.
- Measured archive SHA256 `622225adc3c885d4a7a6ed659ca5b9e32604b1318fb5606354dd36b4ebdce075`.
- All49 final held sources match before/after this review. The measured-to-final
  diff is only `scripts/benchmark.py` and `docs/benchmark-method.md`; inspect
  `measured-diff.patch`. The code changes are confined to existing-output resume
  validation and its self-check, and do not change fresh matrix execution.
- Production binary SHA256 `410645600d70a69e69d9558420ea1cda61bf127a878497a6b8a0e88b17ba342e`.
- Benchmark binary SHA256 `38b24b2934448716c74bad2dceaa91492e1bbc168b351e1f61460b90572c5faa`.
- Original pilot SHA256 `9e6cbbb57bfaac2149eb39efaf43169dbf8e47cf708075cf83b83716615854f6`.
- Before/after identity records confirm unchanged product sources, binaries and
  pilot. Original31 owned PIDs and reviewer smoke4 PIDs are absent. Probe servers
  were closed, reviewer temporary directory is empty, and only the reviewer-owned
  build target was deleted. All writes stayed under this review directory.

Evidence entry points: `summary.json`, `identity-before.json`,
`identity-after-and-cleanup.json`, `independent-probes.json`,
`quota-artifact-audit.json`, `budget-and-lineage-audit.json`, `self-check.log`,
`smoke.json`, `trace-default.log`, `trace-feature.log`, `cli-surface.log`.

Proceed only to the separate fresh QUALITY review required by the parent workflow.
This review does not start the next implementation task.
