# Competitor Round 2 Implementation Plan

> **For agentic workers:** REQUIRED: Use superpowers:subagent-driven-development. Steps use checkbox syntax for tracking. No commit/push: workspace approval rules override skill defaults.

**Goal:** Reconcile supported JSON usage without heavyweight dependencies and expose existing exact-cache status in the terminal.

**Architecture:** Extend the existing per-worker response observer with a bounded JSON alternative. Reuse protocol usage helpers, existing ledger cleanup and the existing cache snapshot; keep body forwarding and cache eligibility independent.

**Tech Stack:** Rust 1.88, existing serde_json, Tokio, Axum and test fixtures; stdlib Python for the local paired probe.

---

## Chunk 1: Bounded JSON accounting and cache status

One implementation/build owner. Other agents are read-only reviewers. Work on the current authorized dirty tree: moving only committed HEAD to a new worktree would omit the implementation being extended.

Files:
- Modify `product/src/transport/stream.rs`: response classification, mutually exclusive bounded observation, clean EOF result and cache parse sharing.
- Modify `product/src/protocol/mod.rs` and endpoint modules only where shared count validation belongs; a small `product/src/protocol/json.rs` is acceptable if it keeps transport focused.
- Modify `product/src/cache.rs` only to reuse an already parsed completion result; preserve all cache guards/budgets.
- Modify `product/src/cli.rs`: render existing snapshot in ordinary status.
- Extend current usage/cache/resource/CLI tests, or add one cohesive `product/tests/json_usage_contract.rs` using existing fixture helpers.
- Update `product/scripts/benchmark_cache.py` JSON observed-usage expectations, runtime contract and current README accounting/status descriptions. Historical report results remain historical.

- [x] Read the full current affected flow and all callers; inspect the selected design.
- [x] Add focused failing checks for JSON usage/unknown/Reserved/cache separation and terminal cache display. Record pre-change failures.
- [x] Implement the minimum bounded JSON observer and reuse existing protocol count handling. Preserve delivery and fail-closed accounting.
- [x] Run focused checks and inspect any changed legacy fixture expectations; never relax an unrelated safety assertion to pass.
- [x] Update the cache probe expectations and current documentation, preserving prior results and explicit provider-limit caveats.
- [x] Record changed-file hashes and focused check results for independent spec review, then quality review. Fix findings and repeat those reviews as needed.

## Chunk 2: Parent verification and paired evidence

Files:
- Create `product/scripts/probe_json_accounting.py` only if the existing probe cannot express a default-binary paired accounting check simply. Reuse existing HTTP/control/lifecycle helpers; no dependency or installed service.
- Create `reports/competitor-round2-2026-09-15.md` with pinned source evidence, selection/rejections, measured results and limits.
- Record raw artifacts under `evidence/competitor-round2-2026-09-15/` (ignored evidence, never secrets).
- Update work-items/loop-log and document entry points after results exist.

- [x] Freeze baseline release/source manifest outside iCloud (`baseline.json`).
- [x] After reviews pass, run `rtk env CARGO_TARGET_DIR=/Users/ryuwon/Library/Caches/llmgw-cargo CARGO_BUILD_JOBS=2 cargo +1.88.0 fmt --check` from `product`.
- [x] Run the equivalent `cargo +1.88.0 clippy --all-targets --all-features -- -D warnings` and `cargo +1.88.0 test --all-targets --all-features -- --test-threads=2`, retaining full output/counts.
- [x] Build the default release (`cargo +1.88.0 build --release --bin llmgw`) and record hashes/size.
- [x] Run paired baseline/candidate low-TPM and no-refund controls, plus the current direct/miss/hit probe. Capture all outcomes, body checks, raw times and local resource snapshots; clean up only owned processes/temp state.
- [x] Have an independent agent recalculate the raw results and verify claim boundaries. Update documentation with only measured conclusions.
- [x] Record final integrity, work item and loop result. Report implementation/measurement limits; no VCS mutation without separate approval.
