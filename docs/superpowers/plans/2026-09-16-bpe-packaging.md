# Optional BPE Packaging Implementation Plan

> **For agentic workers:** Use superpowers:subagent-driven-development. Preserve the current dirty checkout and prior accepted changes. No commit/push/release is authorized.

**Goal:** Remove unused BPE vocabularies from the default native executable while keeping an explicit offline BPE build.

**Architecture:** One Cargo optional dependency/feature, one shared estimator capability predicate, existing parse/start/save validation, and a wizard showing compiled choices only. No new runtime layer or automatic fallback.

**Tech Stack:** Existing Rust1.88, Cargo features, serde, dialoguer, installed bpe-openai0.3.1.

## Chunk 1: Compile capability and existing boundaries

- [ ] Independently review [spec](../specs/2026-09-16-bpe-packaging-design.md) and this plan; baseline is `evidence/workflow-completion-2026-09-16/baseline.json`.
- [ ] Add default-build regression for explicit BPE config/draft/typed startup rejection; run RED under the old default. Include preservation of existing files and no bound listener.
- [ ] In `product/Cargo.toml`, set `bpe = ["dep:bpe-openai"]` and make the pinned dependency optional. Preserve bench-harness independence.
- [ ] Update `product/src/input_estimate.rs`, `product/src/config/validate.rs`, `product/src/server.rs`, `product/src/setup/prompts.rs` following the spec. Keep enum serialization names; no missing-feature byte estimate.
- [ ] Keep all bytes/setup tests active. Gate only BPE-specific checks in `product/tests/input_estimation_contract.rs`, `product/tests/setup_contract.rs`, and the `product/src/protocol/request.rs` test module; add default unsupported-capability controls.
- [ ] Coordinate exclusive Cargo ownership with parent. Run focused tests for default and `--features bpe`, plus fmt. Prefix commands with `rtk proxy env CARGO_TARGET_DIR=/Users/ryuwon/Library/Caches/llmgw-cargo CARGO_INCREMENTAL=0 CARGO_BUILD_JOBS=2 cargo +1.88.0`.
- [ ] Independent spec compliance then quality review; fix concrete findings. HOLD source hashes before final builds.
- [ ] Parent runs all-target tests/Clippy for `bench-harness` and `bench-harness,bpe`, normal release default/BPE builds, dependency graphs, sizes and bounded no-wait/lifecycle/compaction checks. Windows agent uses existing GNU tools and isolated owned paths for both capabilities.
- [ ] Parent updates EN/KR README, installation/runtime contract, evidence and consolidated result report. Do not claim reduced BPE-selected memory or faster quota-bound generation.

One feature is the deliberate maintenance cost. Local compile validation is not hosted CI, and no release is published by this task.
