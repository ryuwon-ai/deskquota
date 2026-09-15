# Competitor-adoption product implementation — 2026-09-15

Status: independent spec and quality reviews approved; frozen implementation retained for measurement. Native macOS deterministic/socket/CLI checks passed; no new performance or real-provider claim.

## Changes

- Preserve true Models/count_tokens metadata through RequestCost and a single Entry fact. Settlement cannot recharge positive generation TPM for this cost. Unknown/unlimited generation uses UnmeteredGeneration. General zero fixtures/estimates retain late positive usage debt.
- Experimental backfill exempts only proven metadata from the zero/absent-TPM-expiry ambiguity checks. Candidate, active and unstarted metadata retain RPM/slot/future-head constraints. Default RR is unchanged.
- Ordinary CLI output renders existing admission status: representative reason, protected root, quota mode/capacity/debit/hold, accounting and estimate. JSON shape and engine policy are unchanged; unknown/unlimited capacity is n/a, not remaining zero.
- Runtime and installation notes explain these boundaries. Dependencies added: 0.
- Added an ignored, bench-harness-only full-ledger profiler. Parent subsequently timed it; see `../competitor-comparison-2026-09-15/full-ledger-profile.json`. Cases are 0/1/8190/8192 retained entries; 1/16 sequential root-head checks; check vs can_backfill. At 8190 a successful future-expiry projection is asserted; 8192 is saturation rejection. Reports Entry bytes, 25 batches x16 iterations, 32 warmups, successful calls, mean and batch p95. It excludes full queue selection, locks and HTTP; 16 sequential identical heads are not an end-to-end queue benchmark.

## Test-first evidence

- `red-metadata-behavior-fixed-test.log`: valid tests failed because genuine metadata was refused by backfill and could recharge positive TPM. 2 failures.
- `red-metadata-settlement.log`: true metadata produced debt31 when expected0. 1 failure.
- `red-status.log`: ordinary native CLI omitted quota modes/details. 1 failure.
- Earlier `red-metadata.log` and `red-metadata-behavior.log` are test-harness compilation mistakes (private module access, missing Debug), not product defects. They are retained separately. `clippy.log` initially found a test module before later items; it was moved to the end without changing runtime behavior.

## Passing verification

- `green-focused.log`: 221 passed across library, backfill, quota, fairness, lifecycle, retry, stream, usage and wire contracts. Seven new regression tests; existing checks remain.
- `green-default.log`: 64 passed without bench-harness (library, quota, lifecycle); overlaps the focused set and is not added to a unique-test total.
- `green-profiler-compile-final.log`: final feature library 29 passed, profiler1 ignored. Test-only profiler additions followed the broad functional checks.
- `format-check-hold-final.log`: cargo fmt --all --check passed.
- `clippy-hold-final.log`: cargo clippy --locked --all-targets --all-features -- -D warnings passed.
- git diff --check passed. All raw commands, exit codes and environment overrides are preserved in the adjacent JSON/log files; source SHA256 values are in `source-hold.json`.

Metadata-specific checks cover cumulative RPM, the protected execution slot, both active and unstarted entries, zero-cost late debt, same-root FIFO, concurrency1, unchanged upstream catalog/403 bytes and authorization forwarding with exactly one HTTP attempt. Existing deadline/cooldown/queued and started lifetime checks passed.

All builds used CARGO_TARGET_DIR=/Users/ryuwon/Library/Caches/llmgw-cargo and CARGO_BUILD_JOBS=2. No commits, pushes, real API calls, global installs or new runtime dependencies. No profiler timing before parent scheduling approval. Source review does not establish Windows, low-end hardware, real LLM quality or competitor performance.
