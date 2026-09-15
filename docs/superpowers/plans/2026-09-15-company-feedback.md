# Company Feedback Implementation Plan

> **For agentic workers:** REQUIRED: Use superpowers:subagent-driven-development. No commits or publishing without separate repository authorization.

**Goal:** Apply the five user-authorized field corrections while retaining a small native gateway.

**Architecture:** Reuse Win32 handle checks, standard auth forwarding, existing SSE settlement and bounded streaming. Introduce only a bounded optional exact cache and one startup-hold setting.

**Tech Stack:** Existing Rust/Tokio/bytes/SHA-256/windows-sys; local Python benchmark utilities.

## Chunk 1: Windows portability

Files: product/src/config_patch/storage.rs, product/src/lifecycle/platform/windows.rs, existing config-patch tests, product/docs/installation.md.

- [x] Review all read_regular/same_snapshot callers and existing Win32 helper.
- [x] Add stable handle identity and remove every windows_by_handle call; preserve non-reparse checks and changed-file rejection.
- [x] Add one Windows identity replacement regression; cross-check available target and run local unaffected config-patch suite. Record absent native Windows execution explicitly.
- [x] Document executable versus source-build prerequisites, including compiler/assembler/dlltool and no MSVC runtime-build requirement for the GNU route.
- [x] Independent spec/correctness review before accepting.

## Chunk 2: Standard auth, actual default and configurable hold

Files: config.rs/config/validate.rs, server.rs, protocol/mod.rs, transport/headers.rs, lifecycle identity/state, clients/cli/setup/config_patch token-specific paths, admission/mod.rs/quota.rs; corresponding existing tests and examples.

- [x] Trace data-token creation through runtime and Pi/Claude/Codex profiles, then delete the obsolete requirement and fields.
- [x] Keep control authentication; reject Origin data requests and invalid POST content type; test Authorization/x-api-key passthrough and env/none modes with actual upstream fixtures.
- [x] Remove secret-header injection from client edits while preserving credentials, unrelated settings, previews and restore ownership.
- [x] Change omitted/new setup accounting to actual; retain explicit reserved and unknown-usage safety. Add focused default/EOF regression.
- [x] Thread startup_hold_secs through validated config, setup, preview/snapshot and ledger constructor; tests for0, short, default60, invalid/overflow values and unmetered behavior.
- [x] Run config/setup/client/wire/quota/retry/lifecycle suites and independent review.

## Chunk 3: Exact cache

Files: one cache module, config/setup additions, server/stream integration and metrics; one focused cache contract suite and one local timing driver using existing HTTP fixture.

- [x] Finalize configurable TTL/history eligibility using user feedback or stated provisional defaults; review bounded ownership and full-key isolation before integration.
- [x] Implement memory-only byte-exact lookup with effective auth/route/model/header scope; explicit no-store/bypass and bounded TTL eviction.
- [x] Capture complete successful JSON/SSE misses without delaying first output; avoid retaining original delivery-budget permits; store nothing for errors/partial/oversized responses.
- [x] Serve hits before quota admission, keeping upstream/usage counters honest and cache counters separate.
- [x] Check hit/miss/expiry, changed input/auth/root/options, history/tool/state restrictions, aborted capture, bounded memory and unchanged quota with actual wire-attempt counts.
- [x] Run a sequential synthetic miss/hit comparison with counts and tail latency; no inherited field-performance claim.


## Chunk 4: Review and delivery

- [x] Run fmt, appropriate regression tests and clippy using CARGO_TARGET_DIR outside iCloud, jobs2; check available Windows target without presenting it as native execution.
- [x] Update English/Korean README and runtime/client/install docs so old mandatory-token/default-reserved/fixed-hold/no-cache claims are removed from current contracts. Preserve historical benchmark records.
- [x] Independent final review, fix material findings, update bounded evidence loop/work items and report remaining Windows/real-provider validation.
- [x] Report results and concrete Windows rerun commands; no commit/push/release.
