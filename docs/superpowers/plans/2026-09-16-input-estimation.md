# Model-selected Input Estimation Implementation Plan

> **For agentic workers:** REQUIRED: Use superpowers:subagent-driven-development or superpowers:executing-plans. User approval for this scoped implementation is already recorded; no commits/pushes without new applicable authorization.

**Goal:** Reduce avoidable TPM admission waits through explicit native input estimators while preserving request/response semantics and exposing the memory/accuracy trade-off.

**Architecture:** A small estimator enum owns UTF-8 or BPE JSON counting, and each configured model selects it and a framing allowance. Initialize only selected encodings for known TPM at gateway construction; reuse the existing protocol inspection and quota ledger. The estimator never translates protocols or alters payloads.

**Tech Stack:** Existing Rust1.88/Axum/Tokio/serde stack plus pinned bpe-openai0.3.1; existing stdlib Python benchmark/mock drivers. Build outputs stay in `/Users/ryuwon/Library/Caches/llmgw-cargo`, incremental disabled. Work in the current child repository with one product writer; retain existing uncommitted research documents.

## Chunk 1: implementation and verification

### Task 1: estimator, model configuration, setup and documentation (one implementer)

Files: create `product/src/input_estimate.rs`; modify `product/src/lib.rs`, `product/Cargo.toml`/`Cargo.lock`, `product/src/config.rs`/`config/validate.rs`, `product/src/protocol/request.rs`, gateway construction, admission diagnostics, CLI display, `product/src/setup/{mod,prompts,document,summary}.rs`; minimal fixture constructor updates in existing tests. Add `product/tests/input_estimation_contract.rs`. Update `product/docs/runtime-contract.md`, setup/config documentation entrypoints, English and Korean README.

- [x] Add failing tests for parsed model selector/default overhead, unknown selector, byte-mode nonzero overhead rejection and BPE overflow. Reuse config and wire fixtures; avoid duplicate mock infrastructure.
- [x] Implement `InputEstimator::{Utf8Bytes,Cl100kBase,O200kBase}` with names, checked count-plus-allowance and preparation. Default bytes/0; explicit BPE default32; selected overhead accepts0. BPE counts original full JSON with normal literal special-token-looking strings, preserving UTF-8 content and no normalization. Pin bpe-openai0.3.1. Do not make provider-name inference or unsupported encoding fallback.
- [x] Integrate only after existing request validation. For known TPM calculate the model's input estimate then add the unchanged output reservation. Keep errors explicit. Unknown/unlimited and metadata bypass all tokenizer work.
- [x] Prewarm selected encodings during gateway construction before listening, keeping config loading/preview side-effect free. Reuse the same shared instances in requests.
- [x] Rename internal estimated byte labels to input estimate where appropriate. Preserve public fixture cost paths and all existing ledger/queue semantics. Surface configured model selectors and allowances in status and snapshots, with explicit estimated semantics.
- [x] Wire model choices through setup, preview, TOML preservation and reruns. Retained model IDs preserve existing choices; newly chosen IDs start conservative unless explicitly selected. Only BPE selection requests an allowance.
- [x] Verify independent fixed text vectors, body/output/SSE usage byte preservation, token count versus byte rejection, unknown-TPM bypass, config/setup round trips and relevant regression suites. Keep diagnostics honest about unknown server templates and provider admission contracts.
- [x] Update EN/KR user docs with config example, selected-mode memory cost, provider matching and unchanged output/body semantics. Historical performance stays historical; parent adds measured results later.
- [x] Run fmt, targeted tests and report exact changed files and commands. Do not commit. Parent runs final full tests/Clippy/release after reviews.

### Task 2: repeatable release comparison (parent, harness files only)

Files: new `product/scripts/benchmark_input_estimation.py` reusing existing benchmark_http Client/Mock/request_once and config writer; evidence under `evidence/input-estimation-2026-09-16/`; report `reports/input-estimation-results-2026-09-16.md`.

- [x] Build pre-change b72961c normal release and preserve binary/hash outside iCloud.
- [x] Read all reused harness functions. Implement a bounded standalone driver for explicit byte/BPE selection, configured costs independent of gateway count, all terminal outcomes, reservations/observed usage, RSS and payload assertions. No real LLM/API/key access.
- [x] First run one original18-request quarter_actual pair using normal release and empty fresh quota state (startup hold0 in fixture only). If usable, run at least3rotated pairs; compare request IDs, p95/mean, complete/deadline outcomes and attempts. Do not attribute old bytes/4 contract to BPE correctness.
- [x] Run3short paired repetitions of TPM-only mixed-input burst, multi-turn interaction, and no-wait known/unknown TPM. Include English/Korean and long/repeated input. Run a deliberate mismatched byte-cost contract to expose under-reservation, without treating failures as latency wins.
- [x] Keep builds and measurement processes sequential. Initialize tokenizers before measurement; report readiness separately and selected/unselected RSS separately. Report paired tails/throughput/failures together, not just average wins.
- [x] Where Windows SSH remains reachable, build/test GNU using the existing configured environment and run a focused selector/wire probe. Preserve the user's host setup.

### Task 3: independent reviews and completion

- [x] Review spec compliance, then code quality; resolve findings before final validation.
- [x] Run Rust1.88 full tests with benchmark fixtures, fmt, Clippy `-D warnings`, normal release and selected auto-compaction/usage regression. Re-run performance only if reviewed fixes change the measured code.
- [x] Record results, limitations and next action in report/work-items/loop-log. Existing saved tail analysis remains unchanged as historical evidence.
- [x] Report actual gains and memory cost; do not claim all-provider precision, universal speed advantage, or Windows proof unless executed. No commit/push in this task.

Completed on 2026-09-16. [Measured results and limits](../../../reports/input-estimation-results-2026-09-16.md). Selected-mode acceptance only: default bytes/0 and scheduler semantics stay unchanged. macOS full444 plus final focused93, Windows focused155, native setup8 and compaction9 passed; counts overlap. Three paired runs per primary condition, no-wait controls and one larger-allowance control retain negative findings and resource costs. No commit or push.
