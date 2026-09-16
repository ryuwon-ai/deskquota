# Conditional 429 Body Probe Implementation Plan

> **For agentic workers:** Use superpowers:subagent-driven-development. Preserve dirty BPE and queued-cache changes. No commit/push.

**Goal:** Stop delaying response headers when reading a 429 body cannot change retry or cooldown decisions.

**Architecture:** Share the existing representation guard, compute whether body classification is useful, and otherwise use the existing streaming response path.

**Tech Stack:** Existing Rust, reqwest, Tokio and gated loopback fixtures. No dependency changes.

## Chunk 1: One transport decision

- [x] Independent spec/plan review against every transient/probe caller.
- [x] Reuse saved binary RED evidence and add failing minimal socket regressions in `product/tests/retry_contract.rs`.
- [x] Change only `product/src/admission/retry.rs`, `product/src/transport/stream.rs`, `product/tests/retry_contract.rs`: share representation guard; preserve timing first; avoid useless body probes; keep original body and bounded ownership.
- [x] Run targeted RED/GREEN and existing retry tests using Rust1.88 with shared cache outside iCloud. Coordinate exclusive Cargo ownership with parent.
- [x] Independent spec review then quality review, correcting concrete issues.
- [x] Parent runs final relevant tests/Clippy/release and six-case independent release probe; check ordinary wire/cancellation/cache/auto-compaction on final artifact.
- [x] Update runtime contract and consolidated results/debate report. No new retry capability, 503 cooldown, or provider policy inferred.

## Completion evidence

2026-09-16: implementation/spec/quality gates passed. Final macOS all-target tests453passed/0failed/1ignored, fmt/Clippy/release passed; Windows focused native tests185passed/0failed/1ignored and six-case429probe passed. See [results and limitations](../../../reports/efficiency-improvements-and-debate-2026-09-16.md) and `evidence/queued-cache-2026-09-16/`. No commit/push/release.
