# Workflow completion under matched limits

2026-09-16. Design for review; no new benchmark result is claimed here.

## Question and scope

Does a user finish the same useful work earlier through DeskQuota than through a
direct connection, Bifrost, or LiteLLM after retries and waiting are included?
The acceptance criterion is the motto: **Get more done within your LLM limits.**
Quick rejection is not task completion. A gateway queue is not useful merely
because it turns a client-visible failure into a long wait.

Use only native, pinned, already prepared runtimes and loopback synthetic LLM
responses. No Docker, WSL, new Redis service, paid API, model substitution,
fallback endpoint, real prompt, or credentials. These are fixture workflows,
not coding-task quality or real-provider performance evidence.

## Reuse and minimal harness change

- Reuse `scripts/compare-native-gateways.py`: `workload`, `payload`,
  `CanonicalMock` admission/service semantics, `RoutedClient`, pinned runtime
  paths and provenance.
- Reuse `product/scripts/benchmark_http.py`: TCP client, bounded body/SSE parser,
  RST close, fixture lifecycle, and exact original usage observation.
- Reuse `product/scripts/benchmark.py`: distributions, owned-process resources,
  startup/stop/cleanup helpers. Do not copy the complete existing benchmark.
- The shared `Client.exchange` currently discards response headers. Expose the
  parsed header collection in its result and check its callers before changing
  it. Preserve duplicate timing fields if the retry fixture exercises them.
  An empty/missing-header control must continue to work. Do not introduce a new
  HTTP dependency just to retain these values.
- Add one workflow driver and its `--self-check`. Existing no-retry historical
  artifacts and launchers remain unchanged; new effective configs are written
  in each run's temporary directory and their exact hashes recorded.

## Arms and the quota contract

Primary arms are direct, current DeskQuota with explicit local rolling limits,
the same DeskQuota binary in provider-managed configuration, Bifrost, and
LiteLLM. Add a before/after DeskQuota binary only when this turn changes runtime
logic relevant to the fixture. Do not add a sixth arm merely to rename the same
binary/configuration.

1. **DeskQuota local rolling** keeps the recorded original configuration:
   RPM16, TPM6000, actual settlement, concurrency2, selected `cl100k_base` with
   the existing +256 input overhead, startup hold0, cache off. The result must
   store the complete effective config rather than assume this is the default.
2. **DeskQuota provider-managed** uses existing RPM/TPM `unknown`, actual
   settlement, concurrency2, shared upstream429 cooldown, startup hold0, cache
   off. The upstream independently enforces the same fixture limits. This
   configuration does not enforce an additional personal spending/rate budget.
3. **Bifrost/LiteLLM provider-managed** also let the independent fixture enforce
   quota. Retain concurrency2, one model/endpoint, requested response usage, cache off,
   and no fallback. Bifrost omits local governance rate limits; LiteLLM removes
   model RPM/TPM prechecks and uses its supported simple routing configuration.
   Validate the exact pinned version rather than assume current documentation
   applies unchanged. Do not retain their old incompatible local limiters and
   call the comparison contract-matched.
4. Keep each peer's earlier configured-local-limiter behavior as a separately
   labelled one-seed reproduction if needed. It is not the primary fair result.

All performance arms use the same upstream RPM/TPM ledger and service semaphore. Let the
provider contract differ *between named profiles*, never between arms of a
profile. Local rolling and provider-managed configs have different guarantees;
faster provider-managed behavior is a configuration/contract effect, not a new
scheduler algorithm or proof that user-specified budgets can be removed.

Clarify this existing choice in setup before measurement, without changing the
schema, defaults or saved values: known means a local rolling 60-second cap;
unknown defers enforcement to the upstream with no additional local cap and an
unverified upstream limit; unlimited explicitly removes only the local quota
cap. Concurrency and shared 429 cooldown still apply. Update the existing summary
assertions. This wording describes verified current code, not a performance claim.

## Retry ownership and budgets

The primary comparison uses the same useful client retry policy for every arm,
with gateway/provider SDK retries explicitly zero. This avoids leaving peer
failures unretried, as the old 2.5-second table did.

- At most four client HTTP attempts per logical node: initial plus three retries.
  At most four actual upstream attempts per node. The independent provider
  ledger records every attempt, including rejected attempts; any unexpected
  internal retry that exceeds the ceiling invalidates the run.
- Retry complete pre-output408/429/500/502/503/504 responses. Honor explicit
  `x-should-retry: false`. Permanent auth/validation errors are terminal. A
  partial successful stream is not silently replayed.
- Use valid provider timing as a lower bound. For this initial harness support
  seconds, HTTP date, and milliseconds using standard-library parsing; retain
  both observed fields and computed monotonic retry time. If multiple valid
  timing values conflict, use their maximum and record that harness policy.
- When timing is absent, use 0.5s,1s,2s exponential backoff with deterministic
  per-node/per-retry jitter in [0.75,1.0], seeded identically across arms. A hint
  beyond the remaining deadline ends in a deadline outcome, not an early retry.
- Retry delay is outside that node's active HTTP exchange. Other independent
  nodes continue. There is no benchmark-wide client cooldown, which would
  implement DeskQuota's sharing function on behalf of competitors.
- Store retry-reason, selected delay, actual wake time, attempt index, any
  visible status and request/response boundary times. Never reset the logical
  node or workflow timer on retry.

**Three distinct denominators are mandatory.** A local gateway429 consumes one
client HTTP attempt but zero upstream attempts. A gateway internal retry would
consume another upstream attempt without another client attempt. Logical work
is counted once regardless of either. Do not add client and upstream attempts
into one number: that double-counts forwarded requests. In the primary arm,
the per-node ceilings are equal but actual counts may differ legitimately.

A small secondary retry-ownership sensitivity run can enable each product's
documented native retry policy (Bifrost `max_retries=2`, LiteLLM router
`num_retries=2` with nested SDK retries0, DeskQuota existing opt-in maximum1)
and set client retries0. The common ceiling remains four upstream attempts;
products need not use it all. Label configured policies and actual counts,
because their retry coverage is not equivalent. Validate a recoverable429 and
503 with one required retry before selecting any native-policy result for a
comparison. Changing models or adding alternate accounts is outside this task.

## Logical deadlines and outcome accounting

Each workflow has `released_at` and one absolute `deadline_at`. A node's first
eligible time is max(workflow release, dependencies' successful completion plus
fixed tool delay). Its absolute deadline is inherited from the workflow. Its
latency starts at first eligibility, not the latest attempt. Client scheduling
lag from the planned release is reported separately.

For the original18 reproduction each row is one workflow; retain its original
arrival offset. Keep the earlier125s from its original ingress as the client deadline, and
keep the original100ms cancellation on g7 visibly labelled. That row's20ms
service historically finished before cancellation; it is not cancellation proof.
Batch makespan starts at the first planned workload release. Even an original18
per-request p95 near2.5s has an approximately34s batch duration because arrivals
span roughly32s. Never call these two metrics the same thing.

Every planned workflow ends in exactly one of completed, permanent failure,
retry budget exhausted, deadline exceeded, cancelled, or dependency failed.
Every planned node ends in a corresponding outcome or not-started dependency
failure. Pending work cannot disappear from the denominator. A final HTTP429
is an intermediate attempt outcome until retry/deadline policy makes it terminal.

Record request first HTTP byte, first body, first model content, terminal marker
and EOF separately. The unchanged original18 body does not request
`stream_options.include_usage`. Complete such requests after validated output,
`finish_reason=stop`, DONE and EOF; if usage is observed it must match the exact
fixture counts, while missing unrequested usage is counted separately and does
not turn a valid completed workflow into failure. Malformed, duplicate or
incorrect observed usage is a contract failure, even when it was not requested.
The existing client records content and DONE but does not retain finish reasons,
so the new driver must validate that field from its retained SSE body. Peer
reserialization is permitted; compare semantic request/output and any observed
usage. Never require peers to preserve JSON whitespace or generated SSE chunk
IDs. DeskQuota's byte preservation and unchanged usage remain separately covered
by its existing cache/compaction regression probes.

Preflight and every measured run use fresh, separate gateway and fixture
instances, so neither local nor provider quota state carries into measurement.
Before performance runs, a separate short `usage_contract` preflight sends the
same `stream_options: {include_usage: true}` request through every arm. Exact
fixture usage is then required as well as content, finish reason, DONE and EOF.
That preflight's requests and upstream attempts are recorded separately from
the original18 performance denominator; its extra body field never changes the
original performance payload or canonical cost. This distinction follows the
recorded LiteLLM behavior, not a post-run relaxation of a failing product result.
See [the source/raw-record audit](../../../evidence/workflow-completion-2026-09-16/usage-contract-design-review.json).
Successful-workflow p50/p95/p99 is secondary and always beside its denominator.
Primary output is workflows completed by2/5/10s from their own release and by
their fixed deadline, plus all-work completion time when all complete. Show
short/long and root distributions, and all-attempt resource/cost totals.

## Profiles

| Profile | Fixed fixture | Purpose / limit |
|---|---|---|
| `original18_rolling` | Existing `workload(101,"generation","quarter_actual")`; rolling60s RPM16/TPM6000; upstream cap2; original service/arrival/body/usage; deadline125s per row | Reproduce the38s tail with client retries included. Independent limiter admits only allowed generation attempts. All18 within2.5s is not a plausible target under this contract. |
| `original18_bucket` | Same rows/costs/service/deadlines; RPM bucket capacity16, continuous refill16/60s; TPM remains strict rolling6000 | Detect unnecessary waiting caused by assuming every RPM16 limit is a60s rolling window. Seed101 actual total4924 stays below TPM6000. The bucket implementation is fixture-only; do not add a product token-bucket scheduler yet. |
| `not_exhausted` | Eight unique one-node workflows, four roots,20/100ms services, cap2, high provider RPM/TPM;3s deadlines | Forwarding/scheduling control; no expected benefit from hiding quota failures. Include pooled warmup outside measured denominators. |
| `recoverable429` / `recoverable503` | Eight unique workflows with staggered0–0.7s arrivals; a globally shared1s provider outage, complete JSON error with explicit timing, then100ms success;5s deadline | Same endpoint recovery window for all arms. Count attempted calls during the unavailable window; compare shared coordination with independent client retries. This is a controlled outage, not a claim that every503 should block unrelated providers. |
| `chains` | Four independent three-node workflows; each next node uses the validated predecessor output plus50ms fixed tool delay; mix20/400ms generation; one root per workflow; high provider quota, cap2;5s deadline | Actual dependency ordering and critical-path completion. Healthy workflows must keep progressing while one is waiting/retrying. Output equality validates that later nodes did not run early or consume a different predecessor. |
| `cancel_control` | DeskQuota local/provider-managed only, separate cap1; gate one upstream long response, then queue a second same-root node and another independent root; observe the queued node through control status before a real socket RST | Functional control outside the performance denominator. After RST confirm the cancelled node never reaches upstream; release the gate and confirm the other root completes. Record in-flight residual service and final queue/process cleanup. No framework to infer opaque peer queues is needed; all performance profiles retain cap2. |
| `usage_contract` | One short unique SSE request per arm with explicit `stream_options.include_usage=true`; high provider limits, no failure injection | Required requested-usage/content/finish/DONE/EOF preflight, outside the performance denominator. The original18 payload remains unchanged. |

Use the original seed101 unchanged for direct historical comparison. Prioritize
five paired repeats of the unchanged bucket workload for DeskQuota local versus
provider-managed configurations, alternating arm order. This establishes
repeatability for that fixture, not generality across workload seeds. Peer and
direct controls use three rotated runs, or five when feasible; preserve their
smaller sample count. Short controls/recovery/chains use five runs if their speed
difference becomes a conclusion. Broad workload claims require additional fixed
seeds101–105, and remain unverified in this bounded iteration.

## Bounded execution sequence

1. Run harness self-check without a product. Run the strict original18 once each
   through direct, DeskQuota local, DeskQuota provider-managed, and the earlier
   Bifrost/LiteLLM native quota configurations with the common client retries.
   This diagnoses retry and local-policy effects; it is not a fresh ranking.
   Do not build concurrently.
2. Review payload validity, quota/deadline and actual attempt accounting before
   repetitions. A failed harness setup is not a product result.
3. Run the token-bucket original18 with five DeskQuota local/provider-managed
   pairs first. Its expected batch durations are approximately67/34s before
   measurement, so this core is roughly9minutes plus startup, not a promised
   runtime. Then run direct and provider-managed peer controls in three rotated
   blocks, extending to five if feasible. A run hard-stops by the last release
   plus125s deadline and bounded cleanup. If the total passes20minutes, preserve
   the completed core and reduce peer repetition; report the missing coverage.
   Do not shorten the32s arrival span or2.5s long service to manufacture a win.
4. Short controls run only after startup validation, each with at most10s hard
   timeout and at most32 client/provider attempts for the eight-node profiles.
   The12-node chains profile has at most48. Native retry sensitivity is
   a separate small matrix, not multiplied into every original18 run.
5. Stop, drain or explicitly cancel every owned client and process; require no
   residual fixture task. Capture source/binary/config/script hashes before
   and after. Never touch unrelated gateway or user processes.

## Fixture self-check and review gates

One `--self-check` must exercise actual loopback HTTP and independent expected
records before product runs:

- Retry429→200 and503→200 once, while a permanent400 and explicit retry-false
  response cause one attempt. Verify absent-header backoff too.
- Without requested usage, a valid content/finish/DONE/EOF response lacking usage
  is completed with a missing-usage count. The identical response fails the
  separate requested-usage contract. Any present incorrect usage fails both.
- Delay cannot be shorter than a valid hint, including an HTTP-date boundary;
  a hint exceeding the deadline never starts a second attempt.
- Retry success latency includes the first failed attempt and sleep. A new
  attempt does not create another successful logical workflow.
- A local-like429 responder produces one client attempt and zero calls to the
  separate upstream recorder. Four local rejections consume the client budget.
- Rolling and bucket ledgers independently reject overspend and differ on a
  known refill trace. Rejected calls do not become successful-generation debits
  in these named fixture contracts; every rejection is still an upstream attempt.
- Chain node2 is not sent before node1 completion/tool delay. Dependency failure
  leaves descendants explicitly not started. Independent workflows proceed.
- Reject corrupted summaries: missing/duplicate workflow outcome, unrecognized
  node/attempt, changed body/output/usage, success after deadline, retry before
  hint, attempt ceiling exceeded, unaccounted cancellation or pending tail.
- A partial SSE, missing finish reason or missing terminal is never counted as completed and is not
  retried by the primary client. A missing header field does not break previous
  no-retry callers of the shared client.

Independent review should recompute results from attempt/node/workflow records,
not trust the printed summary. Performance and contract checks are separate.

## Primary-source context

OpenAI's Python SDK documents automatic retry support; this is why disabling
all client retries is not a useful stand-in for normal users. Our bounded
deterministic retry loop is a stated benchmark policy, not an assertion that
all SDK versions behave identically. [Official SDK retries](https://github.com/openai/openai-python#retries).

LiteLLM documents configurable retry, timeout and cooldown behavior, including
fallback to other model groups. We use one model and explicit retry ownership
to preserve the workload. [Official reliability documentation](https://docs.litellm.ai/docs/proxy/reliability).

Pinned local Bifrost source `core/schemas/provider.go` declares
`DefaultMaxRetries = 0`; its normal loop is in `core/bifrost.go`. Zero is not
itself a misconfiguration, but presenting unretried errors as fast completed
tasks would be misleading. Record the source SHA and executed binary hash.
