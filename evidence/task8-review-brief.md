# Task 8 review handoff preparation

Prepared while the full matrix is still running. This file is a navigation aid,
not a SPEC/QUALITY result. Start fresh SPEC review only after the implementer
returns HOLD. Product edits/builds must remain stopped during the measured run.

## Approved scope

The normative task is `docs/superpowers/plans/2026-09-12-gateway-core.md`, Task 8,
with the core design and `reports/benchmark-spec.md` as context. Its pilot is
direct / production RR / same-transport benchmark FIFO / benchmark RR, five
seeds, five real quota windows, plus separate no-wait measurements and exact-cost
virtual-clock traces. Deficit scheduling, competitors, native lifecycle and real
provider performance are later work. Do not require every research workload in
the broad benchmark specification to be implemented by this bounded pilot.

Parent-approved implementation choices include the small
`scripts/benchmark_http.py` helper, separate no-wait/quota phases, a composite
arrival schedule sharing one ledger, and TPM 6,000 synthetic units selected
before quota measurements. Production retries/cache are off. No new runtime
dependency is intended. The benchmark feature and example must not expose a
policy selector in the production CLI/config or alter ordinary RR behavior.

## Identities and prior acceptance

- Accepted Task 7 source: `evidence/product-task7-quality-held-source.tar.gz`
  and `evidence/product-task7-quality-parent-probed-source-manifest.json`.
- Source actually used for the full Task 8 measurements:
  `evidence/product-task8-measured-source.tar.gz` and its adjacent archive JSON.
  The archive includes 50 product files and the 49-file implementer hold
  manifest; `.gitignore` is the additional product file. It has no binaries.
- Actual executable hashes and per-run source/config hashes are in
  `product/artifacts/pilot.json` and the exclusive run files.
- Keep the measured identity separate from any subsequent documentation or
  resume-integrity correction. Determine whether a correction affects measured
  behavior before deciding that measurements require repetition.

## Evidence to inspect at HOLD

1. The complete manifest must say `completed_composite_matrix` and map exactly
   40 runs with matching SHA256. No partial journal or `.failed.json` may silently
   become a completed result. Every ingress and actual attempt has a terminal
   outcome, with pending-at-end followed through drain/timeout.
2. The independent no-wait audit is
   `evidence/product-task8-no-wait-parent-audit.json`. It covers 20 runs,
   2,000 measured requests and 100 separate warmups. All observed no-wait
   ledger-retained counts are zero. This is not populated-ledger overhead proof.
3. Run `scripts/audit-product-benchmark.py` on the final manifest with a new
   exclusive evidence output. Its partial two-seed audit v2 passes 800 ingress;
   v1 failed because the parent auditor expected boolean false instead of
   recorded integer zero for retries. The unchanged source data passed after
   correcting that auditor comparison. Neither audit replaces payload source
   review or fixture execution.
4. Inspect harness negative self-checks, exact-clock RR/FIFO traces, production
   and feature verification logs, and owned-process cleanup. Reuse passing full
   suite evidence unless a new change/failure justifies repeating it. A focused
   independent counterexample is preferable to another identical full matrix.
5. Success requires payload semantics, terminal marker and valid body EOF.
   First HTTP byte, first body, first valid output delta, terminal marker and
   EOF must remain distinct. Gateway starts and mock-received HTTP attempts
   have separate counters.

## Findings already sent to implementer

- **P8-C1:** `benchmark-method.md` currently says warmup-populated ledger state,
  but all no-wait snapshots show retained zero. Correct after measurement and
  keep the populated-known-quota overhead gap explicit.
- **P8-C2:** the resume branch validates an existing run structurally but does
  not compare its current SHA with the old manifest entry SHA. The implementer
  says this branch has not been used by the uninterrupted matrix. Reproduce and
  fix after measurement; do not rewrite original measured data.
- One seed-1 benchmark-RR mock 429 is preserved. Parent recomputation found
  5,843 live units plus requested 221 exceeded 6,000; one earlier debit expired
  about 30.667 microseconds later. This is consistent with a rolling-boundary
  timing difference, but the gateway reservation timestamp was not captured,
  so exact causality is not established.
- A second mock 429 occurred in seed-4 production RR during drain. At the
  recorded receive timestamp, 16 live RPM debits remained; the next one expired
  about 3.734 milliseconds later. Its total completion count is 79 with 22
  drain completions, while within-300 completions remain 57. This is an RPM
  boundary observation, distinct from the first case's TPM boundary. See
  `product-task8-quota-boundary-observations-partial.json`; retain both failures.

## Conclusions that the data must not overstate

- The preliminary no-wait paired additional p95 and idle RSS fit the tentative
  2 ms / 50 MiB budgets only on this M4 Mac with an empty ledger. Overlapping
  arm variation does not establish a faster scheduler or competitor advantage.
- In seeds 1/2, direct completes 68/69 ingress within 300 seconds, while
  production RR completes 57. Production's 80 total completions include 23
  after the measurement interval. Report the extra wait and both denominators.
- These same within-300 completion counts remain after excluding the 15
  requests with a non-null scheduled cancellation. A key present with a null
  value is not a scheduled cancellation. See
  `product-task8-cancellation-cohort-observation.json`.
- Metadata waiting, barrier occupancy, reservation error, concurrency and
  rolling-budget timing are observations or follow-up hypotheses, not an
  isolated causal decomposition. Actual-vs-reserved accounting and safe
  backfill have not been experimentally evaluated here.
- Test correctness, method validity, tentative overhead budget and efficiency
  improvement are separate judgments. Native on/off/setup/installation, other
  OSes, actual model pickers and real tasks are still outside this acceptance.

After SPEC passes, use a fresh QUALITY reviewer. No Git commit/push/merge,
publication, current-user client changes or real login registration is authorized
by this handoff.
