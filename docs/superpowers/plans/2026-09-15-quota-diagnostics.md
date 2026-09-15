# Quota diagnostics implementation plan

> **For agentic workers:** REQUIRED: Use superpowers:subagent-driven-development. Steps use checkbox syntax. User authorization covers implementing the approved scope; commit/push/release remain separately authorized actions.

**Goal:** Explain cache exclusions and reservation differences through existing status, show the actual queue blocker, and eliminate cache misses caused only by SDK retry-count metadata.

**Architecture:** Keep ownership in the existing cache mutex and admission ledger. Reuse the existing control JSON and CLI with fixed counters; change no admission scheduling or provider accounting policy. Preserve the dirty working tree and the prior release binary.

**Tech Stack:** Existing Rust 1.88, serde, axum/reqwest, tokio and SHA-256 dependencies; Python stdlib only for existing/local probes.

---

## Chunk 1: bounded product change

Spec: `docs/superpowers/specs/2026-09-15-quota-diagnostics-design.md`.

### Task 1: implementation and targeted regressions

Files: `product/src/cache.rs` (eligibility reason counters, key and Vary), `product/src/server.rs` (pass original request reuse prohibition into cache evaluation), `product/src/admission/quota.rs` (finish-time diagnostics), `product/src/admission/mod.rs` (status), `product/src/admission/queue.rs` (representative actual blocker), `product/src/cli.rs` (display), existing `product/tests/cache_contract.rs`, `product/tests/quota_contract.rs` and module tests. Keep tests in their existing suites; no new framework.

- [x] Read every affected function and caller, preserving preexisting dirty work. Parent freezes the previous release and records source hashes first.
- [x] Add regressions that fail on the original code: changed `x-stainless-retry-count` must not create a second cache miss; original Vary on that header must still prevent storage; missing diagnostics/queue blocker behavior must fail for the intended reason.
- [x] Run targeted regressions with `rtk env CARGO_TARGET_DIR=/Users/ryuwon/Library/Caches/llmgw-cargo CARGO_BUILD_JOBS=2 cargo +1.88.0 test --test cache_contract` from `product`, plus filtered module/ledger tests. Record the actual failure before changing logic.
- [x] Add fixed bypass counters and deterministic classification without broadening eligibility. In the header-name collection use `headers.keys().filter(|name| name.as_str() != "x-stainless-retry-count")`; forward headers unchanged. Treat matching Vary tokens as uncacheable.
- [x] Add saturating reservation totals only at the idempotent known-TPM generation finish transition, before debit replacement. Serialize wide totals as decimal strings. Keep metadata, unknown usage, unstarted cancellation and cache hits out of known samples.
- [x] Resolve protected-head blocking through the existing ledger check; preserve scheduling state and empty-queue semantics. Print concise diagnostics using existing CLI helpers.
- [x] Run targeted checks and self-review. No commit. Hand the stable diff back for independent review.

### Task 2: validation, documentation and acceptance

Parent owns `README.md`, `README.ko.md`, `product/docs/runtime-contract.md`, the result report and evidence records; do not race an implementer editing these files.

- [x] Run independent spec review, fix findings, then independent quality review and fixes. Reviewer reads actual diff/callers, not only implementer claims.
- [x] Run `cargo +1.88.0 fmt --all -- --check`, `cargo +1.88.0 test --all-targets --features bench-harness`, `cargo +1.88.0 clippy --all-targets --features bench-harness -- -D warnings`, and `cargo +1.88.0 build --release`, sequentially with the external target/two-job environment prefix above.
- [x] Run a small sequential baseline/candidate loopback check for retry-count-only JSON/SSE reruns and a matching-Vary control, saving only synthetic counters/body hashes/status. Reuse existing harness helpers; no new gateway or real account configuration.
- [x] Run the existing `product/scripts/benchmark_cache.py --help` to confirm options, then execute it against the held normal release into a new evidence path. It checks direct/miss/hit JSON/SSE body semantics, attempts, usage, latency distributions and descriptive process resources; report its limits.
- [x] Update English and Korean README and runtime contract, including considered-vs-global-request denominators, fixed bypass meanings, known-TPM reservation samples and retry-count/Vary contract.
- [x] Verify research links/artifact integrity and `git diff --check`, record final hashes and review status. Mark this bounded implementation complete only with passing checks; keep Windows runtime pending and later provider/advanced scheduling work explicit. No commit, push or release.

Acceptance: completed 2026-09-15; see `reports/quota-diagnostics-2026-09-15.md`. The initial cache latency observation prompted three alternating paired rechecks under a recorded plan. No consistent latency improvement established. Windows runtime, commit/push and public release remain separate.
