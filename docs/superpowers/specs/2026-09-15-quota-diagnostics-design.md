# Quota diagnostics and one exact-cache miss fix

Status: user approved the positioning and staged implementation direction; Windows runtime verification will follow when the user connects their PC. This specification makes the first bounded change concrete. Preserve prior dirty research work; no commit, push, release, global installation or external provider call.

## Choice and scope

Reuse the existing control status, cache mutex, admission ledger and CLI. Existing `blocked_reason`, cache hit/miss and worker metrics already answer part of the question. Add bounded missing information and remove one reproducible source of cache misses. No new dependency, database, settings, background sampler, request log, scheduler policy, tokenizer or response header. Native Windows runtime remains explicitly unverified; no need to wait for it to implement platform-neutral logic.

The existing pinned competitor analysis supports completed-usage settlement and bounded cache lookup. The installed OpenAI Python SDK attaches `x-stainless-retry-count` as attempt metadata. Our cache currently hashes all effective headers. Narrowly excluding this one field is a cache reuse contract change, not general header normalization or an inference about all custom upstreams.

## Cache diagnostics

- Count cache-policy evaluations after normal route/body validation when caching is enabled. Invalid ingress and disabled-cache traffic are outside this denominator. Use `considered` for this count; preserve the global `requests` counter's existing meaning.
- Give every bypass exactly one fixed reason, with deterministic precedence: request cache-control/pragma prohibition; request byte limit; unsupported endpoint; tools/stateful request; history limit; remaining unsupported request shape. An empty tools field still follows the existing exclusion policy. No caller-controlled labels, body fragments, URLs, model strings or headers are retained in counters.
- On quiescent completed lookups, `considered = hits + misses + sum(bypasses)`. Misses still mean eligible cache lookups that did not hit, not every request. Hits do not imply new upstream usage or a newly completed worker.
- Keep current eligibility unchanged except the explicitly described retry header key change. Classifying an exclusion must not accidentally enable previously excluded request shapes. Unknown fields, malformed options, media and unsupported content remain excluded.
- Print the policy-evaluation denominator and named bypass counts in `llmgw status`, using unavailable for absent runtime data. Explain that counters reset with the worker and that store rejection/budget bypass is distinct from request eligibility.

## Reservation diagnostics

- Reuse the existing single-owner `Ledger::finish` for exactly-once recording of started, non-metadata generation with known TPM. Record known-usage sample count, unknown-usage count, and aggregate original reserved amount, observed usage, reservation excess and reservation shortfall for known samples only.
- Compare the original reservation with `ObservedUsage::total(endpoint)` before existing settlement changes the debit. Include genuine zero usage. Unknown/malformed/overflowed usage remains unknown; canceled unstarted holds and cache hits are excluded. Reserved accounting and expired reservations are still observable without changing their accounting behavior.
- Use saturating wide integers for cumulative token sums and decimal strings in control JSON, matching the existing wide debit representation. These are local reservation-vs-usage diagnostics, not tokenizer error, provider remaining capacity, refundable quota or proven saved tokens.
- Print counts and totals in existing admission details. No estimator or ledger admission behavior changes.

## Current queue cause

- Keep the existing representative instantaneous reason and protected-root display. When the starvation barrier is active, show the protected head's actual ledger blocking reason (RPM/TPM/concurrency/startup/capacity) instead of replacing it with the generic barrier label. If no requests are queued, report no queue blocking cause even if a cooldown is still active; the existing cooldown duration remains visible.
- This is a snapshot, not historical time attribution, an exact completion ETA or a proof that every queued request has the same blocker. Do not change queue selection, aging, reservation or wake-up policy. Reuse existing cooldown/startup duration fields rather than adding timers or per-request history.

## Exact-cache key correction

- Ignore only `x-stainless-retry-count` when framing the effective-header portion of the cache key. Continue forwarding the original header unchanged on misses. Its absence, value changes or duplicate values cannot create different cache keys.
- Preserve root, full composed URL/query, credentials, other effective headers and raw body bytes in the key. Do not ignore all `x-stainless-*` fields or normalize JSON.
- Do not cache a response whose original `Vary` declares `x-stainless-retry-count`, case-insensitively and across comma-separated/duplicate Vary fields. Preserve the existing `Vary: *`/malformed exclusion. This respects an upstream that explicitly makes the response vary on this metadata.
- Existing opt-in, TTL, byte/count limits, text-only/short-history policy, complete JSON/SSE requirement, no-store/no-cache, cache hit admission bypass and cancellation behavior stay intact. No in-flight coalescing.

## Acceptance

1. Preserve the previous verified release outside Desktop/iCloud before source edits and record its hash. Add meaningful regressions before implementation: retry-count change currently causes an extra upstream call; demonstrate the failure then correction.
2. Exercise JSON and SSE identical reruns: baseline two requests produce two upstream calls, candidate produces one. Compare body bytes, usage/debits and counters. Vary on the excluded header, changed auth/root/query/body/another effective header must remain misses. This is a synthetic mechanism check, not a real SDK retry run, company hit rate or competitor speed benchmark.
3. Table-test bypass reasons and unchanged eligibility; use a loopback mixture to verify cache denominator closure, no-store provenance and disabled-cache behavior. Do not sum live independent counters as if they were an atomic whole-runtime snapshot.
4. Use the existing deterministic admission/ledger seams to check exact-once samples, zero/missing usage, over/under reservation, reserved/actual modes, expired windows, metadata exclusion and wide totals. Protecting an older request must still show its actual resource blocker without altering existing fairness outcomes.
5. Run targeted tests, then formatting, the full existing Rust suite including benchmark feature, clippy and a normal release build with Rust 1.88, two jobs and the external Cargo cache. Run no concurrent builds or measurements.
6. Run the existing standalone cache probe with the current release for byte-preserving replay, complete outcomes and descriptive footprint/latency evidence; do not claim diagnostic instrumentation is free or that a single fixed-order probe proves a speed improvement.
7. Independent specification review then quality review. Update EN/KO README and runtime documentation with precise counters and key semantics. Record evidence and Windows pending status.
