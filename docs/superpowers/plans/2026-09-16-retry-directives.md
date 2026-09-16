# Upstream Retry Veto Implementation Plan

> **For agentic workers:** Use superpowers:subagent-driven-development. Preserve queued-cache/BPE work; no git mutations or external API calls.

**Goal:** Stop redundant opted-in429 replay when upstream explicitly sends `x-should-retry: false`.

**Architecture:** One small helper reads all values, reused by replay-only body-probe and final replay decisions. Existing timing cooldown and missing-timing classification remain. Global503 cooldown is explicitly not adopted because outage scope is not known.

**Tech Stack:** Existing http headers, Rust, Tokio and gated loopback fixtures.

## Chunk 1: Honor the negative directive

- [ ] Independently review [spec](../specs/2026-09-16-retry-directives-design.md) with A adopted and B deferred; preserve initial11-case RED record.
- [ ] Add minimal failing socket/manual-clock tests in `product/tests/retry_contract.rs`: timed and timingless false, duplicate conflicting values, explicit pause, early head and missing-timing classification control.
- [ ] Change `product/src/admission/retry.rs` and `product/src/transport/stream.rs` only: shared `server_forbids_retry` helper, HTTP OWS trimming, exact lowercase false, both replay guards. `true` must never widen whitelist; original header/cooldown/bytes must survive.
- [ ] Run focused RED/GREEN and retry/stream regression tests, coordinate exclusive Cargo ownership using the same outside-iCloud Rust1.88 target as packaging. No build during timing measurements.
- [ ] Independent spec review then quality review; HOLD hashes after concrete fixes.
- [ ] Parent executes final normal-release directive probe with adopted-contract expectations (503 existing forwarding retained), previous rejection-head checks, full release validation and actual installed-client synthetic compaction.
- [ ] Parent documents the replay reduction separately from successful-task performance and records why blanket503 cooldown was not adopted. Update work items and loop log; no commit/push.
