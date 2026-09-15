# Competitor Adoption Implementation Plan

> **For agentic workers:** REQUIRED: Use superpowers:subagent-driven-development. Steps use checkbox syntax for tracking. Do not commit or publish without the separate repository authorization.

**Goal:** Apply the approved narrow status/metadata improvements and compare preserved/current DeskQuota with pinned competitor runtimes.

**Architecture:** Reuse the existing admission ledger, bounded backfill, CLI JSON status and HTTP benchmark. Preserve passthrough and quota/cancellation ownership. No new runtime dependency or provider framework.

**Tech Stack:** Rust/Tokio/reqwest; Python stdlib/available benchmark dependencies for isolated measurement; native or isolated-venv competitors.

## Chunk 1: Product change and independent correctness review

Files: modify `product/src/cli.rs`, `product/src/admission/quota.rs`, `product/src/protocol/request.rs`; touch `product/src/admission/mod.rs` only if needed to enforce existing settlement; tests in existing quota/backfill/protocol/CLI suites. Update runtime/user documentation after behavior is confirmed. Do not refactor unrelated files.

- [x] Freeze existing executable/example hashes and preserve copies outside iCloud before builds.
- [x] Add a failing focused trace for true metadata backfill while keeping general zero-cost late-positive usage unsafe. Trace candidate, active and unstarted metadata; check protected-head admission and cap/RPM/FIFO negatives.
- [x] Introduce an unmetered generation cost distinct from true metadata; store the smallest necessary kind/flag in Entry. Metadata is always zero TPM on settlement; generation/fixture semantics remain unchanged.
- [x] Exempt only true metadata from zero-cost/late-usage ambiguity checks. Do not bypass slot/RPM/FIFO/cooldown or add a second queue.
- [x] Print existing status fields with readable representative-reason labels and preserve unknown/unlimited. Add one focused output check with multiple meaningful states.
- [x] Run relevant native regression suites with `--locked --features bench-harness` (221 passed), default-feature subset (64 overlapping), final profiler/library compile (29 passed,1 ignored), fmt and all-target/all-feature clippy. Commands and exits are in `evidence/adoption-product-2026-09-15/`. No Desktop target output.
- [x] Independent spec review, then independent quality review. Fix actual findings and rerun affected tests.

## Chunk 2: Runtime comparison

Files: preserve existing harness; create a small experiment wrapper under `scripts/` only if existing entrypoints cannot express pinned external proxy routes or holdout input. Evidence under `evidence/competitor-comparison-2026-09-15/`; reports under `reports/`. Reference prep agents own their separately named reports/evidence/cache paths.

- [x] Fetch the four named references into isolated cache checkouts as needed; record source/release hashes, license, runtime/dependency and config facts. Preserve prior pins.
- [x] Reuse existing HTTP/SSE mock and complete ingress accounting. Self-check external gateway adapter with direct local mock before blaming a competitor startup/protocol failure.
- [x] Run independent generation-only/mixed metadata cases with cap1/2, skewed arrivals and long work. Use current RR and backfill in the same transport; report metadata separately and maintain identical request schedules.
- [x] On the same metadata workload also compare the preserved pre-change backfill executable to the new backfill executable, so the metadata change is not credited with the existing RR→backfill benefit.
- [x] Preserve the independent mixed-input null result; add a three-request metadata-barrier mechanism probe with old/new backfill, actual barrier/idle-slot precondition, identical first-expiry opportunity and full terminal accounting. This is not a general performance holdout.
- [x] Evaluate the previously pending120-second5-pair promotion repeat. **Not run in this task:** the independent generation-only counterexample already rejects promotion (RR16/BF15 complete,2/3 timeouts). Repeating the old favorable mix cannot overturn that gate. Preserve the pending historical experiment separately; do not claim it passed. Spend the remaining measurement on the existing actual-accounting setting instead.
- [x] Build/freeze one ordinary release binary after the active matrix finishes, then compare its reserved/actual settings under both cost contracts with the same seed101 input (4 additional runs). This is an adaptive setting-sensitivity follow-up, not a new holdout or default-accounting promotion.
- [x] Measure bounded full-ledger admission work separately. Include8190 retained entries leaving room for head+candidate with a future-expiry scan, as well as8192 saturation rejection. Record roots/entries/queue occupancy, build mode, iterations and baseline; no empty-ledger-only overhead claim.
- [x] Run at least one functioning pinned competitor against the same local mock; prefer both Bifrost and HiveMind if they install/start cleanly. Separate no-quota forwarding from quota-limited generation. Bifrost, HiveMind and LiteLLM ran; metadata mixed inputs stay internal because external model catalog contracts differ. PromptForge source-only status is explicit.
- [x] Compare fixed-window completions, short/long generation and metadata latency, errors/cancellations/timeouts, upstream attempts, CPU/RSS. Report per-run counts and spread; never pool successes without outcome counts.
- [x] Decide whether default promotion meets gates. Keep RR if evidence is mixed or incomplete; report this decision as a completed evaluation, not a promised gain.

## Chunk 3: Evidence and delivery

- [x] Update runtime docs and a concise comparison report with exact commands, versions, raw artifacts and limits.
- [x] Verify diff, focused tests and artifacts; independent final interpretation review.
- [x] Append the evidence loop and keep unproven real-provider/Windows/demand claims incomplete.
- [x] Report applied changes, measured benefits/regressions and remaining external limitations. No git commit/push/release in this task.
