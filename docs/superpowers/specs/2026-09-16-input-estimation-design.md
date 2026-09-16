# Model-selected input estimation

Status: implementation scope approved by the user's "오케이 개선 진행해" following the tail root-cause analysis. Refine implementation choices within that scope; do not ask again to implement the input estimator. No commit/push authorization is added.

## Problem and choice

Known TPM currently reserves the full JSON UTF-8 byte count plus the unchanged output cap. Saved evidence shows unnecessary admission waits despite available upstream capacity. `actual` settlement happens too late to prevent these waits. Root FIFO remains unchanged in this increment.

Alternatives considered:

1. **Selected BPE encoding for serialized JSON (chosen).** Use maintained `bpe-openai =0.3.1` from GitHub rust-gems; no hand-written tokenizer. Count the original complete JSON with cl100k_base or o200k_base, plus a configurable framing allowance, and retain the existing output reservation. This is a better selectable proxy for some providers, not an exact provider prompt count or upper bound. It includes fields which are not model input and cannot see hidden server templates. The user must match the encoding and allowance to the upstream.
2. Learn a tokens/bytes ratio from usage. Smaller memory, but changing languages, templates, and request shapes can turn prior observations into under-reservations. Do not silently enable adaptive reservations in this increment.
3. Call upstream count_tokens for every generation. Adds network requests and another quota/availability dependency; excluded.

The current dependencies contain no tokenizer. bpe-openai's build script uses bundled compressed vocabularies, builds serialized dictionaries locally, and has no runtime network download. The crate uses Rust dependencies and is MIT licensed. Separate preflight found ~32MiB/~67MiB extra RSS for cl100k/o200k initialization, and a ~50MB standalone binary referencing both; this cost must be reported and checked in the actual gateway, not hidden behind the unconfigured ~10MiB RSS figure. The small probe is research, not a gateway performance result.

## Configuration and behavior

- Add per-model `input_estimator`: `utf8_bytes` (default), `cl100k_base`, or `o200k_base`. No automatic model-name/provider inference and no request header for overriding estimates.
- Add per-model `input_token_overhead`: a u64 framing allowance, default32 for BPE selections and0 for byte mode. Byte mode accepts only0. Validate unknown selectors, negative/invalid numbers, and checked addition overflow. BPE selection is explicit even for familiar model IDs.
- For known TPM, BPE input estimate is `count(original UTF-8 JSON) + overhead`. The output cap or existing configured output fallback is added without modification. Do not clamp the BPE result to byte count, truncate body/context, reduce max_tokens, or change wire usage.
- Unknown/unlimited TPM remains unmetered and does not initialize or execute a tokenizer. Models/count_tokens remain metadata. Existing invalid JSON, duplicate inspected fields, multiple-output, multimodal/opaque-compaction restrictions still apply before token counting.
- Preinitialize only the selected BPE encodings when constructing the gateway for known TPM, before it accepts generation requests. Read-only config parsing, previews, status clients, and doctor do not initialize a vocabulary just to inspect configuration. At most two shared tokenizer instances; no per-model dictionary copies.
- Rename misleading internal Estimated.input_bytes to input_tokens where it now represents an estimate. Keep public fixture costs separate from HTTP input. Ledger rules, usage settlement, queue order, deadlines, retries, caching semantics, forwarded headers/body and cancellation remain intact.
- Status must describe the configured model estimators/allowances and continue showing observed reservation discrepancies. Never label BPE JSON counts as exact upstream tokens. Existing byte-only status labels may remain when all models use byte mode.

## Setup and documentation

The model step offers the three explicit modes with a byte-mode default for a newly selected unknown model. BPE choices explain scope and memory; collect the overhead only for BPE. Re-running setup preserves a model's selected estimator/allowance unless the user changes it. A renamed/replaced model starts with byte defaults unless explicitly selected. Preserve settings for unmodified additional models and existing TOML comment-preservation behavior.

Update config snapshots, runtime/config/setup docs and English/Korean README with a small configuration example and trade-offs. Do not overwrite historical measurements with new results. No claim of Claude/Qwen/Llama tokenizer compatibility just because the JSON API is OpenAI-compatible.

## Verification and acceptance

- Rust 1.88 build, fmt, Clippy and relevant existing quota/wire/usage/setup/client/cache tests. Add a focused estimator contract test with independent known token vectors (English, Korean, special-looking text), invalid config/overflow, unknown-TPM bypass, and unchanged payload/output/usage.
- Use normal release binaries, retain the pre-change b72961c binary/hash, and compare the same synthetic workloads with BPE explicitly selected versus byte mode. Report all outcomes and original ingress IDs; do not treat cached/rejected requests as equivalent to generation completions.
- Re-run the original quarter_actual18-request fixture, plus a TPM-only mixed-input burst, a small repeated multi-turn workload, and a no-wait path. Keep actual fixture costs independent of our estimator; report under-reservation/429 as failures, not as a faster success percentile. Repeat paired runs with rotated order for a claimed effect. First confirm one pair before repeating.
- Check inactive byte-mode RSS separately from cl100k/o200k initialized RSS, package size, initialization/readiness, and warm request p95. Tokenizer preflight has one overlapping exploratory pair; only sequential gateway runs count as comparison evidence.
- Preserve auto-compaction wire/usage invariants and run the existing probe or focused regression where affected. Test Windows GNU natively when the existing SSH host remains reachable; no new runtime, Docker, WSL, MSVC or model download on Windows.
- If a selected estimator fails a quota contract, record it and do not advertise compatibility with that contract. In particular the old quarter_actual fixture is an artificial bytes/4 contract, not a real BPE tokenizer. Its results alone cannot validate a tokenizer.

## Sources

- Existing fixed-source comparison: reports/current-competitor-comparison-2026-09-15.md; reports/latency-tail-root-cause-2026-09-16.md.
- LiteLLM model/custom tokenizer pattern: https://docs.litellm.ai/docs/completion/token_usage . Unknown-model fallback in that product is not adopted here.
- GitHub library: https://github.com/github/rust-gems/tree/main/crates/bpe-openai . Read the downloaded0.3.1 manifest/build script and actual API instead of the stale README example.

## Progress

- [x] Inspect current flow, dependencies, competing pattern, and latency evidence.
- [x] Bound alternatives and approved scope; visual companion not needed for this code task.
- [x] Independent spec review approved; no blockers.
- [x] Implementation plan and review approved; preserve edited multi-model setup and all endpoint output bounds.
- [x] Implement, verify, and report measured trade-offs.

Completed on 2026-09-16. [Measured results and limits](../../../reports/input-estimation-results-2026-09-16.md). Selected-mode acceptance only: default bytes/0 and scheduler semantics stay unchanged. macOS full444 plus final focused93, Windows focused155, native setup8 and compaction9 passed; counts overlap. Three paired runs per primary condition, no-wait controls and one larger-allowance control retain negative findings and resource costs. No commit or push.
