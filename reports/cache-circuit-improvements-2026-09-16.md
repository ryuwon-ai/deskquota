# Exact request coalescing and scoped circuit protection

Date: 2026-09-16. Baseline: `d384e0e`. Candidate: the commit containing this report.
These changes target `develop`; the published `v0.1.0-preview.1` predates them.

## What changed

DeskQuota already had an opt-in exact TTL response cache. Previously, queued
duplicates could reuse a completed response, but free execution slots could
start several identical upstream calls. Eligible duplicates now elect one fill
owner before quota admission. Followers consume neither execution slots nor
quota while waiting. A complete valid response wakes them for replay. The exact
key, eligibility rules, TTL, retained-data limit and cache-off default remain.

The leader streams immediately. Followers wait for the complete response, so
coalescing can delay their first token compared with separately started streams.
Incomplete/error responses are never replayed. Cancellation, an uncacheable
response or failed ownership releases a follower to compete for ordinary
admission with its original deadline. Followers do not retain an admission FIFO
position. Replacements may overlap an old draining or uncacheable attempt within
the existing concurrency limit. The 128-key coordination bound fails open when
full; this bypasses coalescing, not quota enforcement.

A small circuit breaker now tracks failures per root, URL, configured model and
effective authentication scope. Three qualifying failures no more than 60 seconds
apart, without an intervening completed nonqualifying HTTP response, open that
scope for five seconds. Longer valid `Retry-After` timing on the triggering
failure extends the wait. One real request probes recovery after that period;
full response completion closes the circuit. Cached hits remain available.

HTTP 500/502/503/504, connection errors and qualifying upstream stream failures
count. HTTP failures count once at headers. A pre-header upstream deadline after
dispatch counts; body-stage timeouts remain neutral because a slow downstream
can cause them. A completed 4xx/429 proves responsiveness and resets the closed
failure streak; it is not evidence of a successful model answer. Cancellation,
shutdown and local queue errors do not trip the circuit. Each explicitly enabled
429 retry acquires a fresh attempt guard; no new retry class was added.

An open scope returns `503 upstream_circuit_open` with `Retry-After` without a
wire attempt or quota charge. Queued calls are rechecked at dispatch but are not
newly awakened by circuit transitions. Scopes are bounded at 128; if all are busy
or protected, new scopes are forwarded untracked. This bound and the five-second
recovery delay are deliberate costs. Custom routing headers outside the effective
auth scope need separate roots when their failure domains differ.

No dependency, Redis instance, daemon, prompt rewrite or response transformation
was added. `llmgw status` exposes coalescing and circuit counters. The detailed
contract is in [runtime-contract.md](../product/docs/runtime-contract.md).

## Established patterns consulted

| Source inspected | Pattern adopted | Boundary |
| --- | --- | --- |
| [Bifrost `lrucache.Fill`, pinned `1b23416`](https://github.com/maximhq/bifrost/blob/1b23416533e0a8b36192f70b175f9babbd6308f1/framework/lrucache/lrucache.go) | Coordinate one fill per key | A resource-cache helper, not evidence that all LLM requests are coalesced |
| [LiteLLM cache coordinator, pinned `9a715df`](https://github.com/BerriAI/litellm/blob/9a715df212d777bbd43f4cab05731978c708ec63/litellm/proxy/common_utils/cache_coordinator.py) | Collapse concurrent resource fills | Does not establish response-cache performance |
| [LiteLLM cooldown handlers, same pin](https://github.com/BerriAI/litellm/blob/9a715df212d777bbd43f4cab05731978c708ec63/litellm/router_utils/cooldown_handlers.py) | Stop dispatching to a failing deployment temporarily | DeskQuota has no alternate-model routing; protection can intentionally return errors |

These are source comparisons, not new competing-product benchmarks. Independent
design, specification and quality reviews are summarized in
[reviews.json](../evidence/cache-circuit-2026-09-16/reviews.json).

## Directly measured: duplicate quota and completion time

Apple M4, 32 GiB RAM, macOS 26.5.1; normal release builds, default features.
Nine loopback cases, five alternating baseline/candidate pairs each: **90 runs,
900/900 validated completions**, zero failed, timed-out or cancelled requests.
Each run submits ten requests. The upstream has a deliberately fixed **100 ms**
service delay; streaming adds a 5 ms chunk interval. Startup hold is disabled.

The mixed case first establishes eight identical requests in the gateway, then
adds two distinct requests, and releases the upstream gate after observing the
expected state. This isolates the consequence of duplicates occupying slots;
it does not represent arbitrary real-world arrival orders. Times below measure
original submission through validated completion, including gate/queue time.
They are the **median of five per-run p95 values**. With ten observations per run,
nearest-rank p95 and p99 both equal the maximum; this is not a reliable production
tail estimate. Each arm completed **50/50** requests in every row.

| Workload | Upstream calls/run, before → after | Completion p95, before → after |
| --- | --- | --- |
| Ten identical JSON, concurrency 1 | 1 → 1 | 113.9 → 115.4 ms |
| Same, RPM 1 | 1 → 1 | 114.2 → 115.2 ms |
| Ten identical SSE, concurrency 1 | 1 → 1 | 121.6 → 121.9 ms |
| Same, RPM 1 | 1 → 1 | 120.8 → 121.1 ms |
| Ten distinct JSON, concurrency 1 | 10 → 10 | 1039.5 → 1040.4 ms |
| Cache disabled, concurrency 1 | 10 → 10 | 1038.6 → 1040.1 ms |
| Ten identical JSON, concurrency 3 | **3 → 1** | 115.4 → 115.1 ms |
| Ten identical SSE, concurrency 3 | **3 → 1** | 121.3 → 121.6 ms |
| Eight identical + two distinct JSON, concurrency 3 | **5 → 3** | **217.2 → 123.2 ms** |

The duplicate-only gain is **66.7% fewer upstream calls and corresponding fixture
token charges**, with essentially unchanged completion time. In the staged mixed
case, freeing two slots reduces calls by **40%** and p95 by about **43.3%**. The
existing concurrency-1 reuse already saved the redundant calls; it does not gain
again. No broad throughput or real-provider speedup follows from these results.

All ingress outcomes, upstream attempts, quota settlement and post-run ownership
gauges are retained in [matrix-final.json](../evidence/cache-circuit-2026-09-16/matrix-final.json).
The harness rejects twelve deliberate evidence corruptions in
[self-check-final.json](../evidence/cache-circuit-2026-09-16/self-check-final.json).

The entire initial matrix was excluded: independent work could reach the fixture
before the duplicate burst, producing four rather than the required five baseline
calls in one mixed run; test traffic also overlapped that experiment. This was a
fixture-ordering defect, not a failed user response. The rejected run, source hashes
and exclusion reason remain in [excluded-initial-matrix.json](../evidence/cache-circuit-2026-09-16/excluded-initial-matrix.json).
All final measurements ran without other tests or benchmarks in parallel.

## Directly measured: ordinary-path cost and footprint

Five alternating pairs of the existing cache-off, unlimited-quota, no-wait
loopback control; 100 sequential measured requests and five warmups per run,
reusing connections. **1,000/1,000 measured completions**. This includes the local
client, gateway and mock server; it is not isolated proxy overhead.

| Measure | Baseline | Candidate |
| --- | --- | --- |
| Median of per-run p95 | 0.418 ms | 0.425 ms |
| Median of per-run p99 | 0.444 ms | 0.512 ms |
| Idle RSS samples | 9.609–9.641 MiB | 9.562–9.578 MiB |
| macOS executable | 10,197,600 bytes | 10,238,208 bytes |

The binary adds **40,608 bytes (0.40%)**, without new dependencies. The no-wait
control does not improve: median p95 increases about 0.007 ms and p99 about
0.068 ms. Five short runs cannot establish stable tail behavior or a memory win;
RSS is an idle snapshot, not peak use under the 128-scope/flight bounds. Raw runs
and exact binary hashes are linked from
[no-wait-summary.json](../evidence/cache-circuit-2026-09-16/no-wait-summary.json).

## Validation and remaining limits

- Full Rust suite: **461 passed, zero failed/ignored**. After the final equivalent
  extraction of the retry predicate into `can_retry`, reran the four affected
  contract suites: **90 passed**. These overlap; do not add their counts.
- Formatting and `cargo clippy --locked --all-targets -- -D warnings` passed;
  normal release build passed. Build outputs stayed outside Desktop/iCloud.
- Circuit regressions cover three-failure opening, queued dispatch, scope
  isolation, cache access during an outage, a single probe, 429 retry recovery,
  truncated bodies, pre-header deadlines, and neutral downstream timeouts.
  A combined test confirms that followers reuse the owner's 429→200 result.
- Pi, Codex and Claude Code native compaction checks plus protocol/cache checks:
  **9/9 scenarios passed against the final measured binary**, using synthetic
  upstream responses. Compaction completion and subsequent conversation were
  observed, not inferred from status codes. See
  [compaction-final.json](../evidence/cache-circuit-2026-09-16/compaction-final.json).
- Validation snapshot: [validation.json](../evidence/cache-circuit-2026-09-16/validation.json).
  The push triggers the repository's macOS and Windows native package workflow;
  those results are pending at this pre-commit snapshot. Consult this commit's
  checks for their final status. No new public release is part of this change.
- **Authenticated provider calls: zero.** NVIDIA/live workload hit rates, outage
  recovery, follower first-token cost, long-running load and low-end hardware
  performance remain unverified. Fast circuit rejection is a failure outcome,
  not a faster successful answer. TTL reuse intentionally returns a previous
  answer and remains opt-in.

## Reproduce

Build baseline `d384e0e` and the candidate with the same Rust toolchain, using
`CARGO_INCREMENTAL=0`, `CARGO_BUILD_JOBS=2`, and `CARGO_TARGET_DIR` outside synced
folders. Preserve separate normal release binaries. From the repository root:

```sh
python3 product/scripts/benchmark_queued_cache.py --self-check --output /tmp/cache-self-check.json
python3 product/scripts/benchmark_queued_cache.py --baseline /path/to/baseline --candidate /path/to/candidate --repeats 5 --output /tmp/cache-matrix.json
python3 evidence/cache-circuit-2026-09-16/measure_no_wait.py --baseline /path/to/baseline --candidate /path/to/candidate --output /tmp/cache-no-wait
python3 product/scripts/probe_compaction.py --binary /path/to/candidate --output /tmp/cache-compaction.json
```

The no-wait output directory must not exist. The compaction probe requires its
documented client installations. Benchmarks use only owned loopback processes.
