# Exact-request coalescing and upstream circuit protection

Status: implementation and commit/push authorized by the user's “오케이 개선해두고 커밋 푸시해”. Refines the three recommendations in the preceding code/source comparison. No new provider, background model probe, service, dependency, or client configuration change.

## Existing patterns and choice

Bifrost `1b23416533e0a8b36192f70b175f9babbd6308f1`, `framework/lrucache/lrucache.go:165` and LiteLLM `9a715df212d777bbd43f4cab05731978c708ec63`, `litellm/proxy/common_utils/cache_coordinator.py:49` coordinate one resource fill per key. These helpers support the pattern, not a claim that all their response caches coalesce streams. LiteLLM's cooldown handler scopes failure protection to deployments and is cautious about excluding a lone target. Bifrost's documented header-based circuit breaker is Enterprise, not an assumed OSS feature.

Keep the existing exact eligibility and digest unchanged; loosening arbitrary headers or widening tool caching without workload evidence risks incorrect reuse. Replace the measured three-start ceiling with one active fill per exact key. Add a small bounded circuit state machine, rather than model fallback or a generic policy framework.

## Cache contract

- Opt-in TTL, root/URL/auth/header/body isolation, input/history exclusions, valid complete HTTP EOF requirement, byte budgets and original response/usage remain unchanged.
- Atomically elect one owner per eligible key before quota/capacity admission. Followers wait without a worker, quota debit or execution slot. They retain bounded ingress/body ownership and the original capacity/overall deadline. They are reported separately from the admission queue; the connection cap remains 128. Distinct leaders still enter the existing fair queue normally.
- A complete cached response wakes followers. Register notification before checking state. One waiting request may become the next owner if the leader fails, is cancelled, cannot store, or is evicted before replay. Errors and partial streams are never shared as cache results. Dropping an owner always releases ownership and wakes waiters.
- The leader streams normally. Followers receive a completed response, so their first token can wait for leader EOF. No stream fan-out or autonomous leader detached from existing cancellation policy.
- Ownership covers a possible cache fill, not every wire request: once response headers forbid caching, capture exceeds its budget, or a disconnected leader drains without capture, release followers immediately. Such non-cacheable/draining requests may overlap a new owner within the existing concurrency cap. Test this exception explicitly.
- A potentially eligible, explicitly enabled internal 429 retry may retain logical fill ownership until its final response; its successful response can still populate the cache. The failed wire attempt's circuit guard is nevertheless released before retry quota waiting. Clearly non-retryable non-200 responses release fill ownership at headers.
- Bounded flight state and waiter gauges augment existing hit/miss/exclusion status. No prompt/header digests or secret labels in status. Cache-off requests bypass coordination.

## Circuit contract

- Enabled in the core with conservative fixed defaults: three qualifying failures without an intervening completed healthy response, failures no more than 60 seconds apart; open for five seconds, extended by a valid Retry-After / Retry-After-Ms signal on the qualifying failure. Reuse the current parser. The cap is bounded by its existing representable-delay contract, not a new timer.
- Scope: root + final upstream URL/query + configured model index (metadata separate) + effective authorization/API-key and OpenAI organization/project headers, including a custom env-auth target. Hash credentials; never retain or report the raw scope. Return model identity from the existing body inspection instead of parsing prompts again.
- Count upstream HTTP 500/502/503/504 once at headers and unambiguous connection/response-stream transport failures. Count deadline/timeout before response headers only after a wire attempt has started. All body-stage timeouts/deadlines are unclassified because reqwest's total timeout and the gateway deadline include downstream backpressure. Do not count client errors, 429, caller cancellation, shutdown or local admission errors. Completed non-qualifying HTTP responses reset a closed circuit's streak; 429 still follows the existing shared quota cooldown.
- Check after a cache miss and again immediately before each real wire attempt, including an optional 429 retry. An open circuit returns explicit local 503 `upstream_circuit_open` with rounded-up Retry-After; no quota charge, no automatic model substitution. Cache hits remain available while open. Queued requests are rechecked at dispatch; no new queue notification layer.
- After the cooldown one real pending request owns the half-open probe. A completed healthy HTTP response closes it; qualifying failure reopens it. Cancellation/unclassified outcomes release a probe without claiming recovery. Generation/ownership guards prevent older in-flight completions from closing a newer open circuit. No background billable health requests.
- An outcome guard belongs to one wire attempt. Complete/release it on fully consumed 429 before waiting for retry quota, and acquire a fresh guard for the retry before Hold.start(). A counted 5xx cannot be counted again for a truncated body. A neutral cancelled probe returns to expired-open for the next real request without claiming recovery.
- At most 128 scope states; evict only inactive closed states. If all entries are protected/in-flight, admit an untracked new scope and record a capacity bypass, while quota/concurrency still apply. This bounded fail-open ceiling is explicit.
- Status exposes aggregate opens/rejections/probes/recoveries/failures/capacity bypasses and state counts, never credentials, URLs or prompts. No new always-on dependency.

## Verification and limits

Use existing loopback fixtures to prove cap=3 duplicate JSON/SSE calls produce one upstream attempt with intact body/usage and exact quota debits. Include distinct keys, cache disabled, no-store/Vary, incomplete/oversize bodies, cancellation of owner/follower, failed-owner takeover and deadline/slot cleanup. Existing unsafe-cache exclusions remain regression checks.

Use virtual time for circuit state transitions, stale completions and bounded table pressure; real HTTP fixtures for repeated 503/connection failure, excluded 4xx/429, per-model/auth/root isolation, response-stream failure, recovery with one probe, cache hits while open and prestart quota accounting. No synthetic fast error is counted as successful task completion.

Run relevant Rust contracts, full default-feature checks, fmt/clippy, locked release build and native CI after push. Reuse the cache benchmark with five alternating before/after pairs, cap1/cap3/unique/cache-off controls, all completions/errors/timeouts, upstream attempts, latency, RSS and binary size. Build output stays outside Desktop/iCloud. Real-provider, low-end hardware and broad speed superiority remain unverified.
