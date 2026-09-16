# Matched Workflow Completion Implementation Plan

> **For agentic workers:** Use superpowers:subagent-driven-development. Work in the existing dirty checkout with owned files. Preserve old benchmarks and source evidence. No commit/push/real API charges.

**Goal:** Compare original task completion through final success or deadline, including client retries, then adopt the smallest evidenced way to remove unnecessary waits.

**Architecture:** Reuse the existing pinned gateway launchers and HTTP fixture/client. A bounded driver owns logical workflows and retries while the independent synthetic upstream enforces the same contract for every arm. Evaluate existing provider-managed settings before adding another rate limiter.

**Tech Stack:** Python stdlib asyncio, existing native DeskQuota/Bifrost and isolated LiteLLM runtime; no product dependencies.

## Chunk 1: Instrument the complete task

- [x] Independent review of [spec](../specs/2026-09-16-workflow-completion-design.md) and this plan before implementation.
- [x] Clarify existing known/unknown/unlimited labels and setup summary using their verified current meaning, preserving all schema/defaults/values. Keep unknown upstream limits unverified and concurrency/shared cooldown unchanged. Use the existing summary test rather than new configuration machinery.
- [x] Add only necessary response header observation to `product/scripts/benchmark_http.py` after inspecting its callers. Preserve existing fields and timing semantics.
- [x] Implement a focused new `scripts/benchmark-workflow-completion.py` using existing launch/fixture helpers. Keep original18 tasks/arrival/service/TPM6000, strict rolling and explicit token-bucket mock contracts separate, cache off, gateway retry off in primary comparison.
- [x] Give each logical request a125s original-ingress deadline and four client HTTP attempts. Honor valid Retry-After without resetting the deadline. Record separate client HTTP attempts, gateway replay and actual provider attempts; local429 consumes client budget only. Retries of one task never create extra successes.
- [x] Add a runnable self-check for fixture quota/refill boundaries, client retry/deadline/cancellation, workflow dependencies, response validity and malformed/corrupted result rejection. No full duplicate framework.
- [x] Review runnable harness/configs independently; reject comparisons that hide changed caps, response meaning, time origins or unmatched terminal sets.
- [x] Run explicit requested-usage preflight on fresh gateway/fixture instances, isolated from measured quota state. Original18 does not request usage: allow legitimate omission, validate any observed usage, and report both counts beside completion.

## Chunk 2: Run, explain, and apply

- [x] Parent owns final binaries and sequential timing runs. Original strict18 comparison is a one-run diagnostic, not a stable product ranking; include direct, DeskQuota known/provider-managed and peer nativequota plus retries.
- [x] Prioritize five paired original18 token-bucket runs of the same DeskQuota binary under known rolling and provider-managed configurations. Upstream RPM capacity16/refill16 per60s and strict TPM6000 remain identical. Verify capacity is fixture-specific, not a claimed Claude burst allowance.
- [x] Run peer-managed/direct controls with three rotated repeats (five if useful and feasible), plus short dependency/recovery/cancellation cases. Record reduced coverage honestly at the20minute measurement checkpoint; do not change workload timing to obtain desired results.
- [x] Recalculate all logical outcomes, p50/p95/p99, task/batch makespan, within-deadline completion, failures/cancels, attempted calls and sampled resources from raw records. Report latency and deadline success together; expected output is verified for each success.
- [x] Use the measured result to improve setup/documentation for existing provider-managed behavior if appropriate. Preserve locally configured hard budgets as an explicit choice; no silent quota relaxation, new default or invented provider detector.
- [x] Independent result/report review, EN/KR README and report update, work-item/loop evidence. A failed hypothesis remains a reported counterexample rather than a feature promotion.

The earlier user approval covers this bounded implementation/evaluation direction. Real endpoint credentials and confirmed free account limits remain separate from local causal experiments.
