<p align="center">
  <img src="assets/deskquota-hero.png" alt="DeskQuota — Your LLM quota, managed at your desk. A quiet desk and orderly request lanes." width="100%">
</p>

<h1 align="center">DeskQuota</h1>

<p align="center">English · <a href="README.ko.md">한국어</a></p>

<p align="center">
  <strong>Get more done within your LLM limits.</strong><br>
  A lightweight gateway that coordinates LLM requests around shared limits.
</p>

<p align="center">
  <a href="#get-started">Get started</a> ·
  <a href="#how-it-works">How it works</a> ·
  <a href="#measured-so-far">Measurements</a> ·
  <a href="product/docs/client-compatibility.md">Client compatibility</a> ·
  <a href="RESEARCH.md">Research</a>
</p>

<p align="center"><sub>Rust core · One executable · No Docker, WSL, or database required</sub></p>

---

## A little order for a busy desk

One agent is refactoring. Another is reviewing. A third is waiting for a small answer.
They all share an API allowance or a local model's capacity—whether the endpoint
is paid, free, or provided by your company.

DeskQuota sits on your computer, between your tools, scripts, agents and **one configured
LLM endpoint**. It keeps a shared local quota ledger, gives configured client
queues their turn, and coordinates waiting when the upstream returns `429`.

Keep your tools. Put their shared limits in one place.

| At your desk | DeskQuota handles |
|---|---|
| Several tools, one API allowance | Shared RPM/TPM accounting for requests through this instance |
| Large requests beside small ones | Round-robin turns across configured roots, FIFO within each root, and starvation protection |
| A busy or rate-limited endpoint | Bounded waiting, shared cooldown, and explicit retry limits |
| A task you no longer need | Queued cancellation and controlled upstream connection lifetimes |
| Company proxy or custom CA | Explicit transport settings with TLS verification kept on |
| Starting and stopping your workday | Terminal setup, `on`, `off`, status, and optional user-login startup |

**Early development.** The macOS ARM64 package and selected client flows have
been exercised. On one Windows 10 x64 host, the GNU build, 404 tests, ZIP
installer, setup wizard, and runtime with development tools removed from PATH
passed. Claude Code 2.1.76 also passed managed connection and a native Read tool
flow. See the [Windows follow-up](reports/windows-followup-and-improvements-2026-09-16.md)
for remaining client and platform limits, including an intermittent Windows
client-settings file replacement failure. Linux and real-provider performance comparisons remain unverified.
The command is currently **`llmgw`**; DeskQuota is the project name.

## Get started

### Install a native package

Download the macOS ARM64 or Windows x64 archive, its installer, and their
`.sha256` files from [v0.1.0-preview.1](https://github.com/ryuwon-ai/deskquota/releases/tag/v0.1.0-preview.1).
Follow the [checksum-first installation steps](product/docs/installation.md).
Then run `llmgw setup` and `llmgw on`. No compiler, Docker, or WSL is needed.

This is an unsigned preview (macOS ad-hoc signature only). Follow your OS and
company security policy; signing and clean-machine acceptance remain unverified.

The native preview passed CI on both platforms. Its Windows MSVC binary passed
installation checks on Windows 10, then ran with an empty runtime PATH; no
external VC/GNU runtime DLLs were found. See the [release verification](reports/native-preview-release-2026-09-16.md)
for exact files, checks, and remaining limits. These are installation checks,
not real-provider performance results.

### Build from this checkout

A Rust toolchain is needed to build. The installed gateway itself needs no Rust,
Python, Node, Redis, Docker, or WSL runtime.

From the repository root, on macOS:

```sh
cd product
CARGO_TARGET_DIR="$HOME/.cache/deskquota/cargo" cargo install --locked --path .
llmgw setup
```

The build directory stays outside your checkout. Make sure Cargo's binary
directory is on your `PATH`. The [installation guide](product/docs/installation.md)
also covers offline archive transfer and explicit optional BPE builds.

### Make it yours

The setup wizard walks through your local, company, or hosted endpoint; supported
API format; model IDs; authentication reference; RPM/TPM limits; and concurrency.
It previews changes before applying them. Client connections and login startup
have their own previews.

Choose what the local gateway should enforce:

| Limit choice | Meaning |
|---|---|
| A number | Enforce your own rolling 60-second RPM/TPM cap |
| `unknown` | Let the upstream enforce its allowance; no additional local cap, upstream limits unverified |
| `unlimited` | Explicitly impose no local RPM/TPM cap |

Concurrency and shared `429` cooldown still apply in each case. Copying a
provider's continuously replenished allowance into a local rolling cap can add
unnecessary waiting. Keep a numeric cap when you need that local budget;
`unknown` does not guarantee a separate personal allowance.

```sh
llmgw on        # Start the background gateway
llmgw status    # Inspect its state
llmgw off       # Stop the gateway
```

Run `llmgw setup` to revisit configuration, `llmgw doctor` for an offline check,
and `llmgw restart` to apply a saved configuration to the running worker.

> **Before connecting a tool:** its base URL will point to your local gateway.
> Model discovery and capabilities can differ from the tool's usual provider.
> DeskQuota previews the exact client file and keys it will change. Stopping the
> gateway leaves those client settings in place; use `disconnect` to restore
> managed values. See [what changes](product/docs/client-compatibility.md).

## How it works

```mermaid
flowchart LR
    P[Pi] --> D
    C[Claude Code] --> D
    X[Codex] --> D
    subgraph PC[Your computer]
        D[DeskQuota · localhost]
        D --> Q[Shared quota · fair queue · cooldown]
    end
    Q --> U[Your configured endpoint]
    U --- E[Company API / local model / hosted API]
```

The upstream must support the API format each client uses. DeskQuota passes
requests through their configured routes; it does not translate between
Completions, Messages, and Responses or download and run a model for you.

A setup-default OpenAI-style route looks like
`http://127.0.0.1:4141/r/pi-work/v1`. Messages clients use the corresponding
base without the trailing `/v1`. The wizard shows the exact URL for each client.

Use your client's standard API-key authentication. With `auth.mode = "forward"`,
DeskQuota forwards `Authorization` or `x-api-key`; no `X-LLMGW-Token` is needed.
The worker binds only to loopback and keeps lifecycle controls separately authenticated.
New configurations use `accounting = "actual"` to settle valid final usage
from supported JSON and streaming responses. `startup_hold_secs = 60` is configurable, including 0 to disable the hold.

### Connect the tools you already use

| Client | Exercised version | Exercised protocol | Model-list boundary |
|---|---|---|---|
| Pi | 0.84.2 | Chat Completions | Explicit model list and selection verified |
| Claude Code | 2.1.76 | Messages | Automatic discovery unsupported in this profile |
| Codex | 0.154.0 | Responses over HTTP | Gateway model listing is not a Codex catalog |

These are **earlier isolated macOS ARM64 tests with a synthetic upstream**,
including a file-read tool call and follow-up request. After removing the custom
data token, Pi 0.84.2's completion and read-tool flows passed again with a local
fixture and upstream auth `none`. Claude/Codex's updated profiles have local
contract checks; their full native flows remain earlier evidence. These tests
do not certify every version, model capability, or real-world coding task.
[Exact setup, file changes, and tested flows →](product/docs/client-compatibility.md)

Automatic summarization and the next turn passed isolated native tests for
Pi, Codex and Claude Code, including Claude on Windows. There are still limits:
`/responses/compact` is unsupported, and known-TPM inspection rejects opaque
compaction inputs and input estimates that exceed the configured budget. See the
[automatic compaction audit](reports/auto-compaction-audit-2026-09-16.md) before relying on long sessions.

## Small by design

One executable. In-memory scheduling. Bounded queues and stream buffers.
Connections are reused and response chunks are forwarded as they arrive.

Optional exact caching reuses complete short text responses for repeated calls.
Enable it in setup or add `[cache]` with `ttl_secs = 300` and `max_history = 3`.
It uses a fixed 4 MiB payload budget and spends no upstream quota on a hit.
Reusing a result means you receive the earlier answer rather than a fresh sample.
`llmgw status` shows cache-policy evaluations, hits, eligible misses and named
exclusions alongside entries and retained bytes. SDK `x-stainless-retry-count`
alone does not split cache entries; a response that declares `Vary` on that
header is not stored. Credentials, other effective headers and body bytes still
separate entries.

**On `develop`:** concurrent eligible requests with the same exact key share one
cache fill, even when execution slots are free. The leader streams normally;
followers wait for its complete response without consuming upstream quota or
execution slots. Failure, cancellation or a non-cacheable response releases
the next caller. Followers can therefore wait longer for their first token.
`status` separates waiting duplicates from the admission queue. The published
`v0.1.0-preview.1` predates this coalescing and the circuit protection below.

The gateway also protects a failing root/model/credential scope: three qualifying
failures no more than 60 seconds apart open its circuit for five seconds, extended
by valid retry timing on the triggering failure. Cached responses remain available;
other calls receive `503 upstream_circuit_open` with `Retry-After`. After the wait,
one real request probes recovery. No background model calls or model substitution.
Client errors and `429` do not trip the breaker; ambiguous body-stage timeouts are
excluded because downstream backpressure can cause them. State is bounded to 128
scopes; an untrackable new scope still uses normal quota and concurrency limits.

The default macOS executable is **10.20 MB**, down from the previous **59.92 MB**
release by about **83%**. It uses byte estimates and omits BPE vocabularies. An explicit
`--features bpe` build remains one offline executable with the same commands.
For a model with a known TPM limit, that build's setup can select `cl100k_base` or `o200k_base`
to reduce byte-based over-reservation. Add these fields to that model's existing
`[[models]]` entry only after checking the upstream's encoding:

```toml
input_estimator = "cl100k_base"
input_token_overhead = 32
```

This counts the original serialized JSON plus a framing allowance, **not exact
provider input tokens or a guaranteed upper bound**. An OpenAI-compatible URL
does not establish tokenizer compatibility. Output caps and request bodies stay
unchanged. The default `utf8_bytes` mode requires zero overhead. In the measured
macOS gateway, idle RSS was about **9.9 MiB for bytes, 41.7 MiB for cl100k and
76.7 MiB for o200k**. The current optional BPE build remains **59.92 MB**;
only selected encodings initialize for known TPM. A default build rejects
explicit BPE settings before new startup or saving, rather than changing the
estimator silently. Existing workers can still be inspected and stopped.
These size savings do not reduce the memory used by a selected BPE vocabulary.
[Input estimation measurements and limits →](reports/input-estimation-results-2026-09-16.md)

Status also compares reservations with observed usage for finished upstream
attempts under a known TPM limit. Missing usage is counted separately. These
totals explain reservation differences; they are not an exact tokenizer or a
provider balance. Queue status shows a representative current resource blocker
and any protected root. Counters reset when the worker restarts.

The default policy is round robin with starvation protection. An experimental
backfill policy tests whether smaller requests can use available capacity while
preserving a protected request's conditional admission opportunity. It remains
behind the `bench-harness` feature and is **not the default**.

Some boundaries are deliberate:

- One instance accounts for its own traffic. Other PCs can consume a shared
  allowance outside its view.
- Quota estimates are not an exact copy of every provider's limiter. The current
  default input estimate uses request bytes; selected BPE modes count JSON with an explicit encoding and framing allowance. Valid final usage from supported JSON and streaming responses corrects
  the reservation by default; missing usage retains it. Provider admission
  rules may differ from reported usage.
- Scheduling can reduce avoidable waiting. It does not increase your provider's
  quota or pause and resume a remote model's token generation.
- Semantic caching, automatic model routing, and prompt rewriting are not
  implemented. Exact caching skips tools, stateful requests and incomplete responses.

[Runtime and quota contract →](product/docs/runtime-contract.md)

## Measured so far

Numbers are useful when their boundaries are visible. These are local
observations on an **Apple M4 with 32 GiB RAM**, not low-end hardware guarantees.

In five paired runs of the original 18-request workload against a **continuously
replenished RPM fixture**, choosing the existing provider-managed quota setting
reduced task p95 from **38.13 to 2.51 seconds**. Both settings completed **90/90**
tasks with **90 upstream calls**. Short-input mean fell from **3.76 to 1.21 seconds**;
batch completion fell from **67.59 to 31.96 seconds**, including staggered arrivals.
This removes an unnecessary additional local rolling cap; it is not a new
scheduler or an increase in provider allowance. With a strict rolling upstream
limit, completing every task still took tens of seconds. Keep a local numeric
cap when that is the budget you need to enforce. Both measured settings disabled
startup hold; these times exclude the default 60-second startup hold.

The earlier packaging check recorded **10.20 MB on macOS / 16.38 MB on Windows**, with
**9.67 MiB idle RSS** observed on this Mac. Default/BPE builds passed **456 tests
each on macOS**; Windows passed **409 default / 236 focused BPE tests**. These
overlap rather than adding up to unique checks. Retry-veto, setup, lifecycle and
compaction checks passed within their documented platform scope.
[Workflow results, peer controls, build validation and tradeoffs →](reports/workflow-completion-and-lightweight-builds-2026-09-16.md)

The current `develop` change also coalesces eligible duplicates when execution
slots are free. At concurrency 3, ten identical requests used **3 → 1 upstream
calls**, with essentially unchanged completion time. A staged workload of eight
duplicates followed by two distinct requests used **5 → 3 calls** and reduced
submission-to-completion p95 from **217 → 123 ms**. Both arms completed **50/50**
requests per case across five pairs, with a 100 ms synthetic upstream delay.
These are small mechanism checks, not production tail or model-speed claims.
Cache-off no-wait p95 was **0.418 → 0.425 ms**, p99 **0.444 → 0.512 ms**;
the macOS binary grew by **40,608 bytes**. The new scoped circuit breaker protects
repeatedly failing upstreams; a fast rejection is still a failed request.
[Coalescing, circuit protection, raw outcomes and tradeoffs →](reports/cache-circuit-improvements-2026-09-16.md)

The earlier queued-only cache change reduced **10 upstream calls to 1** for ten identical
JSON or streaming requests at concurrency 1, across five paired runs. All ten
completed. JSON completion p95 after releasing the upstream gate fell from
**1,029 to 104 ms** (median of five runs; 100 ms synthetic generation delay).
With RPM 1, completions within a 750 ms client deadline rose from **1/10 to 10/10**.
Concurrency 3 retained three already-started calls; distinct-request and cache-off
controls retained all ten. This is completed-response reuse, not faster generation.
Cache-off no-wait p95 medians were **0.345 → 0.359 ms**, with about **10 MiB idle RSS**.
Native Windows contract checks passed; real-provider and low-end gains remain unverified.
[Queued-cache results, 429 delivery, tradeoffs and peer review →](reports/efficiency-improvements-and-debate-2026-09-16.md)

With `cl100k_base` explicitly selected, three paired TPM-constrained synthetic
runs completed **10/12 → 12/12** burst requests and **4/6 → 6/6** growing-history
requests within a 1.5-second per-request deadline, with no upstream 429s. The
existing 18-request fixture's p95 fell from **40.63 to 38.13 seconds**, retaining
18/18 completions and 16 in the first minute. A separate, single run with a larger
framing allowance retained that p95 without under-reserving the fixture's input.
Burst success means increased, and a deliberately mismatched encoding contract
produced 429s. These are selected-mode admission results, not universal latency
gains or a new competitor comparison; no-wait large-request p95 increased from
about 0.37 to 1.06 ms.
[Paired results, counterexamples and resource costs →](reports/input-estimation-results-2026-09-16.md)

The earlier diagnostics update also removes a specific cache miss: two otherwise
identical requests with different SDK retry-count metadata used **one upstream
call instead of two**, for both JSON and streaming responses. A matching `Vary`
control still used two. These are synthetic reruns, not a measured SDK retry
workflow or company hit rate.
[Diagnostics update and current release checks →](reports/quota-diagnostics-2026-09-15.md)

Adding JSON usage reconciliation improved completions from **5/20 to 20/20**
in a small-TPM fixture with a 750 ms client deadline, across five paired runs.
Reserved accounting and missing-usage controls stayed at 5/20 on both versions.
No-wait p95 was approximately 0.263 ms on both; the release binary grew by 1,184 bytes,
with no new dependencies. This demonstrates local admission behavior, not a general
throughput multiplier or a provider quota increase.

The pre-diagnostics exact-cache check recorded **0.60 ms JSON / 0.72 ms streaming hit p95**
(30 samples each). It deliberately repeated half of 120 gateway requests,
avoiding 60 upstream calls against a fixture with a 50 ms delay. This verifies
local reuse, not real-world hit rate or model speed.
[Current comparison and cache measurements →](reports/current-competitor-comparison-2026-09-15.md)
[JSON accounting implementation and paired controls →](reports/competitor-round2-2026-09-15.md)
[Earlier field changes and cache measurements →](reports/company-feedback-2026-09-15.md)

A comparison of the pre-diagnostics RR/actual release used Bifrost
transport v2.1.1, HiveMind's pinned HEAD, and LiteLLM v1.100.1. Across five rotated
runs of 100 sequential requests per arm, DeskQuota recorded **0.433–0.556 ms p95**
and **9.56–9.66 MiB idle RSS**, below those three tested configurations. All arms
completed 500/500 requests. This is a cache-off, no-wait local fixture; LiteLLM's
new v1.101.0 was not executed. In a separate quota fixture, DeskQuota completed
18/18 after waiting, with 16 inside the first 60 seconds—the same first-window
count as Direct, Bifrost and HiveMind. Limits, rejected requests and long waits
are included in the [full comparison](reports/current-competitor-comparison-2026-09-15.md).

Earlier measurements remain separate:

| Observation | Recorded result | Scope |
|---|---|---|
| Idle memory | **9.73 MiB RSS** | Accepted macOS package, 10 samples |
| Start / stop | **36–63 ms / 36–45 ms** | 5 local cycles; process lifecycle, not first-request readiness |
| Short-request mean | **14.72 s → 12.32 s** | Experimental backfill vs our RR baseline; 5 paired synthetic runs |
| Short-request p95 | **About 47 s → about 47 s** | Same experiment; no observed improvement |
| Completions inside the measurement windows | **55 → 55** | Five 60-second windows per arm; no throughput increase |

The backfill mean includes successful requests completed during the subsequent
drain. Each arm submitted 100 requests: 85 completed and 15 were deliberately
cancelled. The short-request group includes model-list requests. These repeats
vary arrival jitter in one workload, not five independent workload families.
These seconds include quota waiting and upstream response time; they are not
measurements of the gateway's own processing overhead.

On 2026-09-15, we also ran pinned Bifrost, HiveMind, and LiteLLM versions on this
Mac. Across three repeats of 100 sequential no-wait requests, DeskQuota's
experimental RR binary recorded **0.394–1.229 ms p95** and **8.78–8.84 MiB idle
RSS**. Its observed memory use was smaller than the compared configurations;
its p95 was not lower in every repeat.

A separate paired probe of a blocked queue reduced model-list completion from
**52.02 s to 13.77 ms** in experimental backfill. The protected request still
started at the first quota-expiry opportunity in both runs. This was one
mechanism probe, and the behavior remains experimental.

A new generation-only input completed **16 of 18 requests with RR and 15 with
backfill**. We retain that counterexample and keep RR as the default. Real-API
performance superiority remains unverified.
[Earlier competitor comparison, configurations, and complete outcomes →](reports/competitor-comparison-results-2026-09-15.md)

[Accepted package measurements](product/artifacts/native-final-integration-package/acceptance/README.md) ·
[Backfill results and raw evidence](reports/backfill-experiment-results.md) ·
[Benchmark method](product/docs/benchmark-method.md)

## What comes next

- Reduce avoidable model-list waiting while preserving quota and fairness rules.
- Validate quota estimation against a real endpoint's accounting contract.
- Compare independent workloads, including NVIDIA hosted API compatibility checks.
- Measure real cache eligibility, quota saved by coalescing, and its first-token waiting cost.
- Validate provider-managed completion and local hard-budget tradeoffs against real endpoint contracts.
- Extend Windows checks to a fresh non-admin account, Pi, and actual login startup; exercise Linux and lower-resource computers.

A useful contribution is a small reproducible case: the configured limits,
client version, expected behavior, and observed outcome. Please keep credentials,
company URLs, prompts, and private source code out of shared reports.

## Explore the work

| Start here | For |
|---|---|
| [Installation](product/docs/installation.md) | Native setup, local archives, and lifecycle |
| [Client compatibility](product/docs/client-compatibility.md) | Model visibility and reversible config changes |
| [Runtime contract](product/docs/runtime-contract.md) | Queue, quota, streaming, and cancellation semantics |
| [Research index](RESEARCH.md) | Implementation history, evidence, and open questions |
| [Reference catalog](reports/reference-catalog.md) | Pinned gateway, agent, and inference-engine studies |
| [Recent scheduling research](reports/recent-scheduling-evidence-2026-09-14.md) | Paper findings and what applies to an HTTP gateway |

## License

The original project code is dual-licensed under [MIT](LICENSE-MIT) or
[Apache-2.0](LICENSE-APACHE), as declared in the Rust package. Third-party
references retain their own licenses. See the [snapshot scope](docs/publication-snapshot.md).

---

<p align="center"><sub>A quieter queue for your working day.</sub></p>
