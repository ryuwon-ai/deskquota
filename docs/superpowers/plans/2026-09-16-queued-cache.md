# Queued Exact-cache Implementation Plan

> **For agentic workers:** Use superpowers:subagent-driven-development; preserve the current dirty BPE work and shared checkout. No commit/push is authorized.

**Goal:** Reuse eligible completed exact responses during initial quota/concurrency waiting.

**Architecture:** Add one completion notification to the existing cache, an idempotent request recheck, and race it against the unchanged admission future in the server. No new dependency or configuration.

**Tech Stack:** Existing Rust, Tokio Notify/select, Axum, existing loopback test fixtures.

## Chunk 1: One bounded behavior change

- [x] Review spec and plan independently against actual ownership/counter behavior.
- [x] Save pre-change normal release binary/source hashes outside iCloud. Main agent owns builds/HTTP benchmarks unless handed off explicitly.
- [x] Implementer adds the failing RPM-exhausted duplicate JSON/SSE regression to `product/tests/cache_contract.rs` using `GatedUpstreamFixture`. Run it before product edits and retain failure output.
- [x] Implementer changes only `product/src/cache.rs`, `product/src/server.rs`, and the cache test file: reuse existing key/capture/lookup, notify only after commit, silent waiting/recheck, unchanged ticket future and unstarted-hold cleanup. Add only missing cancellation/negative/counter tests.
- [x] Run focused regression and existing cache tests; run formatting. Build with `CARGO_TARGET_DIR=/Users/ryuwon/Library/Caches/llmgw-cargo CARGO_INCREMENTAL=0 CARGO_BUILD_JOBS=2 cargo +1.88.0` through RTK.
- [x] Independent spec review, then independent quality review. Fix concrete findings before proceeding.
- [x] Main agent runs relevant shared-path Rust suites, Clippy, normal release, repeated existing-harness baseline/candidate cache cases and no-wait controls. Record failures and latency denominators; no arbitrary universal speed claim.
- [x] Update EN/KR cache documentation, runtime contract, evidence/work items, and a concise report containing agent disagreement, adopted/deferred proposals, measured improvement and ceiling.

Remaining larger quota-contract/group/singleflight/packaging proposals stay research findings until their own concrete evidence supports implementation. Existing user approval covers the bounded cache fix; no new approval loop for that change.

## Completion evidence

2026-09-16: implementation/spec/quality gates passed. Final macOS all-target tests453passed/0failed/1ignored, fmt/Clippy/release passed; Windows focused native tests185passed/0failed/1ignored and six-case429probe passed. See [results and limitations](../../../reports/efficiency-improvements-and-debate-2026-09-16.md) and `evidence/queued-cache-2026-09-16/`. No commit/push/release.
