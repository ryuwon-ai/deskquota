# Queued exact-cache reuse

Status: user authorized implementation after the quota-policy deep dive; bounded refinement of its first recommended change. No further product-scope approval is needed. Commit/push is excluded.

## Goal and motto

Get more done within your LLM limits. Reuse an already completed, eligible exact response while a duplicate waits for admission. Keep one native executable, current tools and wire bodies, bounded memory, and no new dependency/configuration/service.

## Existing pattern and alternatives

LiteLLM 1.100.1 `proxy/common_utils/cache_coordinator.py:41-158` uses completion notification and a second cache read for shared resource loads. Bifrost v2.1.1 `framework/lrucache/lrucache.go:150-210` shares in-flight resource fills. These are source patterns, not proof of response-cache performance.

1. A lookup only after admission is smallest but still waits for RPM expiry; insufficient.
2. Notify waiting requests when an eligible response is committed and race cache availability against the existing admission future. Adopt this bounded change.
3. Full singleflight elects one leader even when slots are free, but adds leader failure/cancellation and stream follower ownership. Defer until the queued-reuse ceiling is measured.

## Contract

- Initial cache eligibility/key/security boundary stays unchanged: root, URL, credential-bearing forwarded headers, body bytes, policy, tools/state/history/size bypasses. Validations still precede cache lookup.
- Only a successfully committed complete JSON/SSE response wakes cache waiters. No partial/error/tool response is reused. Replay body and usage bytes stay unchanged; no usage is charged a second time.
- Register notification before checking the cache; a fill between the original lookup and subscription cannot be lost. Recheck silently after unrelated fills.
- Race cache availability against one retained admission future; never re-enqueue on cache notifications. Stop/disconnect/deadline remain higher priority. On a hit, dropping the acquire future removes its ticket or returns any unstarted reservation through existing ownership.
- Recheck once after obtaining admission before handing the request to the worker. A hit drops the unstarted hold and returns the replay. Already started requests keep existing behavior; no preemption.
- Keep request-based hit/miss accounting: a successful queued recheck reclassifies its original miss as a hit exactly once; silent rechecks do not increment counters. Cancellation or non-cacheable completion remains a miss. The implementation should make this state explicit if needed to prevent duplicate counting.
- No change to retry ownership, queue capacity, FIFO/RR, quota contract, estimator defaults, cache TTL/budget, or protocol contents. Initial admission is in scope; internal retry waits are reported separately if not covered.

## Verification

Reproduce before editing production code with existing gated loopback fixtures. At RPM=1, cap=1, hold the first response before completion, enqueue duplicates, then complete the response. Duplicates must finish within a short client deadline with one upstream attempt and one RPM/actual TPM debit. Repeat for JSON and SSE; include a concurrency-only case. Preserve body/usage, hit/miss denominator, queue/held cleanup, negative cache-control/key separation/incomplete response behavior, queued cancellation and deadlines. Existing tests supply negative coverage; add only missing relevant cases.

Run cache, fairness, retry, stream-lifetime, usage, wire and compaction-relevant checks, formatter and Clippy. Use the existing release HTTP harness for repeated baseline/candidate tests with all successes/errors/timeouts/cancellations and upstream attempts, plus a no-wait control. Build output and saved binaries stay outside Desktop/iCloud. No real API calls. Claims apply to repeated synthetic requests, not different prompts or general competitor superiority.

## Ceiling and tradeoff

A single cache notification may wake up to the existing bounded set of waiting requests. Prefer this over a new per-key registry until profiling demonstrates a problem. Multiple duplicates admitted before the first fill may still go upstream. No claim that arbitrary 38-second quota-bound work can finish in 2 seconds.
