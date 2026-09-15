# Competitor adoption round 2: complete usage accounting

Status: design approved; implementation passed independent specification and quality reviews. Full-suite, release and paired measurement checks passed with an independent raw-result audit; see the report for limits.

## Problem and selection

Current DeskQuota reconciles final SSE usage, but leaves nonstreaming JSON requests at their initial reservation. Short classification responses can therefore occupy a large local TPM budget after completion. Bifrost, LiteLLM and HiveMind already consume reported JSON usage; adopt this established pattern using the existing ledger.

Three alternatives were considered:

1. **Adopt:** bounded JSON usage observation and a text cache status line. Directly closes the accounting gap without another service, dependency, persistent store or setting.
2. **Defer:** concurrent exact-request coalescing. A leader/follower lifetime, cancellation, timeout and SSE delivery contract is substantial. A post-admission cache recheck alone cannot wake a request blocked by exhausted RPM, so it does not solve the strongest case.
3. **Defer:** provider routing, automatic 5xx replay and dashboards. These add scope without evidence of benefit for this one-upstream local gateway.

## Response contract

- Observe identity-encoded `application/json` responses on supported generation endpoints with HTTP 200, in addition to the existing SSE path. Inspect all original upstream Content-Type and Content-Encoding values before hop-by-hop stripping. Duplicate, conflicting or non-UTF8 representation values stay usage-unknown; HTTP 206 is not a complete response. Unsupported or ambiguous representation stays usage-unknown.
- Forward each body chunk immediately through the existing delivery path. Observation never waits for a full JSON document before starting delivery and never rewrites payloads.
- Retain at most the existing 256 KiB raw observer storage allowance per worker; JSON and SSE storage are mutually exclusive. This excludes temporary serde_json allocations, existing cache capture and delivery buffers: it is not a total worker memory bound. Stop observing on overflow, keep forwarding, record the existing overflow metric and retain the reservation. A bounded JSON response may be parsed once at clean EOF; reuse that parse for cache completion where practical and drop the parsed value immediately afterward.
- Only clean HTTP EOF may supply final JSON usage. Transport failure, deadline, close-policy cancellation, malformed/truncated/oversized/encoded JSON, non-success status, error objects, unsupported or missing usage never refund. Drain policy can settle a complete observed response after client disconnect, following the existing SSE contract; disconnected responses still cannot populate cache.
- Require nonnegative integer input and output counts; optional cache counts must satisfy existing protocol semantics. Reuse `ObservedUsage`, `token`, `nested_token`, existing hold cleanup and ledger settlement. Do not infer counts from text, `total_tokens` alone, or missing fields; do not introduce a tokenizer or provider accounting mode.
- Chat Completions: completed nonempty choices with terminal finish reasons and a final usage object. Normal tool-call and length-limited responses may carry valid usage and settle even though exact-cache eligibility rejects them. Incomplete choices or an error object remain unknown.
- Responses: a completed response without an error/incomplete marker and valid input/output usage. Queued, in-progress, incomplete and failed responses remain unknown. Tool output does not by itself disqualify usage.
- Messages: a completed message with a nonempty terminal stop reason and valid input/output usage, with the existing separate cache-creation/cache-read fields. Tool and max-token stops may settle; they remain excluded from text-cache reuse.
- Keep usage validity separate from answer cacheability. Cache hits produce no upstream usage or quota debit. Missing usage may still permit caching a complete eligible text response.
- `Actual` remains the default; explicit `Reserved` keeps its reservation while its window remains live and still reports observed usage. Preserve existing positive late-usage charging after reservation expiry in both modes. Providers that enforce admission estimates or independent input/output limits need their real contract checked; combined local actual accounting is not proof that upstream 429s are impossible.

## Cache status

Render the existing `runtime.exact_cache` snapshot in ordinary `llmgw status`: enabled/disabled, hits, eligible misses, entries and retained/budget bytes. Do not add counters, response headers, ratios, polling, storage or a dashboard. Missing status data is unavailable, not a fabricated zero.

## Verification and acceptance

- Freeze the already verified release binary outside Desktop/iCloud before edits, with source and binary hashes. Preserve all prior dirty work and raw results.
- Add focused protocol/loopback regressions for three endpoint families, tool-vs-cache separation, unknown usage, valid zero, malformed/count/cache details, individual and summed count overflow, EOF/overflow/encoding/error, Reserved and cache hit accounting. Actual usage exceeding the reservation must create debt. Reuse existing expired-window/debt ledger checks instead of duplicating that suite. Verify bytes and first-chunk delivery as well as ledger cleanup.
- Use a paired local fixture: identical bounded requests, small TPM, cache disabled, actual response tokens below reservation. Compare baseline/candidate request outcomes and debits within a bounded window; also include a no-refund/Reserved control. This establishes the accounting mechanism, not a competitor performance ranking or actual company quota behavior.
- Re-run the existing direct/miss/hit cache probe with updated JSON usage expectations; report latency distributions, all attempts/failures/cancellations, descriptive resource observations and binary size. No universal p99, low-end PC or model-quality claim.
- Run current Rust tests, formatting and clippy using Rust 1.88, two build jobs and `/Users/ryuwon/Library/Caches/llmgw-cargo`. Independent spec review then quality review must pass before final acceptance.
- Update runtime/client-facing documentation and a source-pinned comparison report. No reference mutations, global installation, external provider call, commit, push or release in this scope.
