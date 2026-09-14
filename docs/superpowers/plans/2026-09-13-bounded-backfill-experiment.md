# Reserved-slot Backfill Experiment Implementation Plan

> **For agentic workers:** REQUIRED: Use superpowers:subagent-driven-development. One product writer/runtime owner; review is read-only until ownership is transferred. No Git actions or releases are authorized.

**Goal:** Test one lightweight scheduling improvement without changing the accepted default.

**Architecture:** One conservative projection helper on the existing ledger, a narrow barrier branch, and benchmark-only selection. Existing admission and HTTP ownership remain authoritative.

**Tech Stack:** Current Rust/Tokio and Python standard-library benchmark helpers; no added packages.

## Chunk 1: Implement and check one candidate

Read the [spec](../specs/2026-09-13-bounded-backfill-experiment.md), `product/src/admission/{quota,queue,mod}.rs`, all callers in `product/src/server.rs`, `product/examples/bench_gateway.rs`, and current quota/fairness/benchmark trace tests.

- [x] Add a failing focused trace in `product/tests/benchmark_trace.rs` (or one focused sibling test if clearer): seed70/100 at60s, protected80 at60s, age barrier at65s, light20 admitted early only by candidate, head still admitted at120s even if light remains active. A further light request must not spend protected quota/slot.
- [x] Implement benchmark-only selection and conservative projection in the existing files. Use the existing next expiry; no mutation of ledger time for a projection, no duration guesses. Guard Actual accounting and uncertain late usage. Avoid replacing production policy or adding a public config knob.
- [x] Extend the focused trace with the spec's negative cases, preserving existing tests. Run `CARGO_TARGET_DIR=target/backfill-experiment cargo test --locked --features bench-harness --test benchmark_trace` and relevant quota/fairness tests; keep the initial failing output.
- [x] Run fmt check, locked all-target feature check, clippy with warnings denied, and the full existing Rust suite on the new target only. Build the release benchmark example. Capture source, binary, commands, and failures in a new `product/artifacts/backfill-experiment/` directory. Never overwrite held native artifacts.
- [x] Transfer ownership for spec then quality review, resolve concrete findings, and recapture only new evidence paths.

## Chunk 2: Decide from measurements

- [x] Reuse `product/scripts/benchmark_http.py` and suitable functions of `benchmark.py`. Keep accounting validation and historical arm identities intact. Add the smallest separate experiment runner only if existing entry points cannot express policy selection; no generic framework.
- [ ] Run one equal-transport loopback pilot including positive-slack and cap1/no-slack cases. Check all ingress IDs terminate and attempts are not hidden. If useful, five seeded alternating pairs across more than one quota window within the 45-minute runtime cap, including an untouched held-out mix.
- [x] Save actual counts and short/long latency plus limitations in `reports/backfill-experiment-results.md`; update the existing V01 evidence and loop log. Preserve the baseline package. Report acceptance for further study or rejection, never paper-readiness from this experiment alone.

No commit step: these directories are not initialized repositories and user approval is required for version-control publication actions. The evidence/runbook is the durable local record.

## September 14 resumed run

The restored 102-file source matches the held snapshot. New builds use
`CARGO_TARGET_DIR=/Users/ryuwon/Library/Caches/llmgw-cargo`, `CARGO_INCREMENTAL=0`,
and two jobs to keep generated files outside Desktop. This is a per-command
target override; no old target is removed and no global Cargo configuration is changed.
Read-only spec/quality reviews passed. Fresh release checks ran 14 tests successfully;
the prior complete 370/381-test logs remain historical evidence on matching source.

The two-window seed1 pilot completed: both arms have 26 fixed-window completions,
34 final completions, six cancellations, and no errors/timeouts. Per-request inspection
then found several short requests finishing 12–71 seconds earlier; identical aggregate
completion counts do not imply identical latency. The early throughput-only rejection
was therefore corrected before deciding the candidate.

Within the original 45-minute round wall budget, the next observations are a one-window
cap1 negative control, followed by five alternating one-window pairs (seeds2–6).
These repeat the existing fixture; they are exploratory replications, **not** five
replications of the 120-second pilot, and must not be pooled with it. Keep startup,
fixed-window completion, and post-window drain distinct. Compare every terminal outcome
and paired short/long latency, including means and per-ID differences; do not claim an
overall efficiency gain from the aggregate p95 alone. Stop if outcomes regress or the
round deadline cannot be met. An untouched holdout and five full two-window repetitions
remain outside the evidence unless actually run; no default promotion follows.

The benchmark example reports `identity: null` by design. Audit the generated TOML/SHA,
binary/CLI source, and observed RPM/TPM/accounting/root values separately. Do not weaken
the ordinary-product identity validator or present this as full runtime attestation.

Final round: pilot + cap1 control + five one-window pairs completed and audited. The original multi-window/untouched-holdout acceptance checkbox remains open. See the result report and final-verification.json; no production promotion.
