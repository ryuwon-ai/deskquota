# Task 8 native benchmark method

This is a controlled synthetic HTTP pilot, not a real LLM or coding-task result.
The executable manifest, outcomes and tentative-budget observations live in
`artifacts/pilot.json` and its exclusive `pilot-runs/` directory. A result is
complete only when that manifest says `completed_composite_matrix`; partial,
smoke and no-wait-only results are explicitly different states. No competitor
performance result is implied. The research benchmark specification and pinned
reference audit supply the measurement boundaries.

## Accounting ablation mode

The original four-arm baseline above remains a separate experiment. Its no-wait
phase describes added proxy latency and memory, while its quota phase compares
direct, ordinary product RR, and two benchmark-only scheduler arms. The
accounting ablation does not reuse those results as a control and does not launch
the benchmark-only executable. It runs the same ordinary `llmgw` path, SHA256,
and empty feature set twice per seed:

- `production_rr` uses `accounting = "reserved"`.
- `production_actual` uses `accounting = "actual"`.

`--mode accounting` accepts only `--phase quota` or `--smoke`. Quota runs require
exactly five 60-second windows, so their evaluator cutoff is 300 seconds. Smoke
uses the same two product profiles with one compressed fixture schedule and a
three-second cutoff; it validates transport, accounting provenance, usage, and
cleanup without an efficiency conclusion. `--pilot-only` is valid only for the
accounting quota phase. It records the complete requested seed schedule but runs
and validates both arms for the first seed before returning
`accounting_pilot_completed`. A later identical command without `--pilot-only`
reuses that verified pair and continues the remaining schedule. A partial pair
never receives the pilot-completed state.

```sh
python3 scripts/benchmark.py --mode accounting --smoke \
  --binary target/native/release/llmgw --seeds 1 \
  --output artifacts/accounting-ablation/smoke.json

python3 scripts/benchmark.py --mode accounting --phase quota --pilot-only \
  --binary target/native/release/llmgw --seeds 1,2,3,4,5 --windows 5 \
  --output artifacts/accounting-ablation/pilot.json
```

Each product run stores the accounting value together with the launched binary
identity, SHA256 of the exact written TOML, and a sanitized snapshot parsed back
from those on-disk bytes before spawn. Startup also requires the worker's
authenticated status fingerprint and admission accounting to match those bytes;
an inconsistency reaps the owned child and fails the run. The snapshot permits
only loopback, auth `none`, retry off,
close-on-cancel, concurrency two, four registered roots, the fixed model bound,
and the phase quota. Pair comparison normalizes allocated loopback port numbers
and generated temporary paths. Scheme, host, upstream path/query, endpoints,
root/model IDs, model output bound, quota, and every other setting remain exact;
the normalized snapshots may differ only at `accounting`.

These written-file and worker-identity checks apply to the two ordinary product
arms in this accounting experiment. The separate baseline benchmark executable
keeps its existing health-only startup contract and is not labelled as an
ordinary product binary.

Every persisted accounting run must also contain both its initial and final
ordinary-worker status. Resume and paired evaluation recheck each status's
fingerprint and admission accounting against the recorded TOML provenance, so
rehashing a contradictory or relabelled run cannot admit it into a comparison.

Completed generation responses must contain exactly one valid numerical usage
object before `[DONE]`. Both token fields must be integers in the unsigned
64-bit range and must match the linked submitted ratio and actual mock attempt.
Resume repeats the exact integer type and range checks on persisted usage, so a
rehashed equal-valued float remains invalid.
Metadata completion records usage as `not_applicable`; a cancellation, timeout,
rejection, or disconnect may record `not_observed`. Missing, duplicated,
post-terminal, or altered usage invalidates accounting evidence without turning
an otherwise observed completion payload into a more favorable outcome. This
client observation checks the fixture response. It is not by itself evidence of
the gateway's internal refund; source-identity-matched Rust contract evidence
remains a separate basis.

Paired output reports each seed's actual-minus-reserved completions at the fixed
cutoff, complete terminal denominators, root/length results, mock 429s, observed
additional attempts, and scheduled cancellation outcomes. Post-cutoff drain
completions and success-only latency stay separate. Measurement start, duration,
and every terminal timestamp must be finite before cutoff classification. A
first-pair checkpoint is
labelled as one-pair evidence and carries no multi-seed conclusion. Its
`matrix_complete` field remains false even when the requested schedule contains
one seed; the same command without `--pilot-only` can reuse that verified pair
and mark the full one-seed schedule complete. Duplicate planned seeds are
rejected rather than counted twice.

## Arms and build identity

- `direct`: the same loopback mock, without a gateway.
- `production_rr`: ordinary release `llmgw`, no optional features.
- `benchmark_fifo`: the `bench-harness` required-feature example, globally oldest
  root head; a nonfitting head blocks later heads.
- `benchmark_rr`: that same example selecting ordinary production root RR.

The example uses the production HTTP path, ledger, queue ceiling 64, immutable
quota, deadlines and cancellation contract. FIFO selection exists only in the
feature build; production CLI/config has no scheduler selector. No exact-cost
HTTP header or alternative estimator exists. Authenticated status adds the
existing bounded `admission.retained` ledger-entry count. No production request
history, timestamps, histogram, eager logging, framework or dependency is added.

```
cargo build --release --locked
cargo build --release --locked --features bench-harness --example bench_gateway --target-dir target/bench
cargo test --locked --test benchmark_trace
cargo test --locked --features bench-harness --test benchmark_trace
python3 scripts/benchmark.py --self-check
python3 scripts/benchmark.py --binary target/release/llmgw --reference-binary target/bench/release/examples/bench_gateway --seeds 1,2,3,4,5 --windows 5 --output artifacts/pilot.json
```

Exact `ManualClock`/`exact_fixture` traces are separate tests, never HTTP timing
samples. The default build runs RR traces; feature tests run the same expected
RR behavior and add FIFO comparisons. Both heavy-light-heavy and
heavy-heavy-light arrivals retain long timeout outcomes. In FIFO's latter case
light also times out behind the second heavy; no light-success-only claim hides
that loss. Other traces cover continuous small arrivals, root/child sharing,
queued cancellation and cap-one nonpreemption. No deficit policy was introduced.

## Two measurement phases

The full command executes **40 sequential runs**, rotating arm order by seed.
Only one competing arm runs at a time on this development host.

| Phase | Runs and submissions | Resource contract |
|---|---|---|
| No wait | 4 arms × 5 seeds × 100 measured requests = 2,000; 5 extra warmups per run = 100 | Explicit unlimited mock/gateway quota, no synthetic service sleep, sequential reused client connection; gateway upstream pooling left enabled |
| Quota composite | 4 arms × 5 seeds × 5 windows × 20 requests = 2,000 | Real rolling 60-second RPM 16 / TPM 6,000 fixture units; gateway concurrency 2, 4 registered roots; no retries or cache |

The first no-wait warmup includes a new TCP connection. All five warmups and
mock attempts remain separate from the 100 measured reused-connection samples.
A two-second client deadline bounds each no-wait request; any invalid response
invalidates that run. No-wait results are not quota-throughput results.

Each quota run first waits the full production 60-second startup hold (plus
0.1 second margin); direct waits the same interval. Proxy idle sampling adds
one second before that hold. There are no data warmup requests consuming quota.
The measurement interval then lasts exactly five scheduled 60-second windows.
Each window contains these four fixed arrival groups:

| Workload file | Offset within each window | Ingress count |
|---|---:|---:|
| `burst.json` | 0 seconds | 8 |
| `mixed_lengths.json` | 12 seconds | 4 |
| `shared_quota.json` | 25 seconds | 4, including 2 metadata requests |
| `cancellation.json` | 40 seconds | 4, with 3 scheduled 50 ms client cancellations |

All groups share the same queue, ledger and mock budget; group/window transitions
never reset them. Snapshots persist carry-over queue/debit/cooldown/retained state
at approximately one-second intervals. Seed jitter is 0–10 ms; fixture service
sleeps are 10–500 ms. Arrivals are open-loop scheduled; contention can delay the
load generator, and actual send time plus scheduling lag is recorded. Each quota
ingress opens a fresh client connection. Server-side gateway HTTP pooling remains
enabled. The 300 planned cancellations across the matrix may race completion;
actual cancelled outcomes are counted separately.

This is a **five-window composite workload**, not four independent five-window
experiments. No retry/cache comparison, external consumer, provider-specific
window, underestimation, or workload-size sweep is claimed in this pilot.
Metadata consumes RPM and the same gateway concurrency/queue, but zero TPM.
Direct has no gateway concurrency cap; it still obeys the exact same mock quota.

Before the full launch, the candidate TPM 12,000 was reduced to 6,000: actual
short request reservation is 221, long is 3,222, and the first 16 reservations
sum to 9,000. A 12,000 budget would principally constrain RPM; 6,000 deliberately
creates a long head that fits total capacity but not remaining capacity. This
selection precedes quota timing results. Earlier smoke/no-wait preflights had
unlimited quota and do not establish behavior at either known TPM value.

The mock computes matching cost from received JSON byte length plus its actual
synthetic output reservation. One mixed-length request per window deliberately
uses a 0.25 actual/estimated ratio (ceil separately for input/output), labelled
`overestimated`. These are fixture accounting units, not tokenizer tokens or
provider observations. Gateway HTTP cost remains actual request UTF-8 bytes plus
output reservation. Accepted mock requests debit its rolling budget; rejected
mock attempts do not. Gateway's conservative rejected-attempt debt is retained.

## Complete denominators and timing

Each submitted ID must have exactly one terminal outcome: completed, rejected,
error, timeout or cancelled. Every mock-received attempt has an ingress ID and
unique ordinal attempt ID and must end completed, rejected or disconnected.
The gateway's independent started-attempt counter is recorded too: a socket
reset can occur after it starts but before the mock receives an HTTP request.
That gap is reported, not invented into a mapped mock attempt or silently lost.
Retries are explicitly off at every layer; amplification still reports both
observed mock receives and gateway starts per submitted ingress.

HTTP 200 is insufficient: generation must reproduce the exact synthetic content
and `[DONE]`, followed by valid HTTP body termination; metadata must match the
expected JSON. A missing marker, truncated body/trailer or malformed payload is
an error. Self-checks reject missing denominators, duplicate/unknown attempts,
nonterminal attempts, duplicate terminal IDs and success without payload evidence.
They also exercise real loopback quota/rejection and malformed SSE/trailer EOF.

The 300-second end snapshot retains every pending ID. Each quota client has a
125-second deadline from actual send; after measurement the harness waits at most
130 further seconds for all tasks, then validates gateway and mock cleanup.
**Post-measurement drain completions remain in the terminal denominator but do
not count as fixed-time completed requests/workflows.** They may consume later
rolling budget and are never reported as 300-second goodput. Per-workload,
per-length, per-root and per-cost-case summaries retain both outcome sets.
A mock workflow is merely a predetermined pair of independent fixture responses
both validated, not a tool-driven or real-world task.

Python client and mock timestamps share `time.monotonic()` in one process. They
observe send start, mock request receive, first HTTP response byte, first body
bytes, first complete output-delta frame, terminal marker and HTTP body EOF.
Send-to-mock-receive combines client connection/ingress/queue/network time; it is
not pure queue wait. No model TTFT or GPU timing is inferred. Production metrics
named `first_body_byte` etc. remain occurrence counters, not timestamps.

Latency distributions expose sample count, p50/p95/p99/max and failure counts.
Small-sample p99 is descriptive. The paired no-wait comparison subtracts matching
seed/request-index completion times from separate runs and preserves negatives.
It is a paired end-to-end latency difference, not internal gateway CPU duration.
The difference of the two arms' marginal p95 values is separately named; it is
not the p95 of individual differences. Empirical cross-seed ranges describe
variability, not confidence intervals or proof of superiority. The tentative
budgets are paired additional p95 <2 ms and sampled idle RSS <50 MiB. Overlapping
variation, changed failure rates or long-request losses preclude a simple win.

## Resource sampling, provenance and resumability

Owned gateway PID RSS and cumulative CPU are sampled with asynchronous `ps`;
load-generator/mock CPU is not attributed to the gateway. No synchronous `ps`
wait blocks the asyncio loop. Quota sampling/control traffic can still contend
with the fixture, so scheduling lag and host load are retained. No-wait loops
have no periodic status/resource sampler; pre/post snapshots record root count,
queue length and retained entries. In this no-wait phase Unlimited quota retains
zero ledger entries even after warmup: these are explicitly empty-ledger samples.
Pure forwarding overhead with a populated known-quota ledger remains unmeasured;
quota samples include deliberate service/queue delays and cannot fill that gap.
RSS peaks are sampled maxima, not OS high-water marks or whole-system bounds.
CPU delta covers measurement plus drain, excluding startup hold; unused mock
budget samples used in summaries cover only the measurement interval. Control
requests are excluded from ingress denominators even though the gateway's broad
request counter includes control traffic. Final status retains bounded body and
stream-buffer counters as well as usage-known/unknown counters.

The manifest records executable SHA256/features, configuration hash and sanitized
structural settings, workload/source hashes, clock, host architecture/CPU/RAM,
load, seed/order and UTC/monotonic times. Temporary state contains synthetic auth
only; artifacts contain no payload text, real credentials or company data.
Current execution is on the Apple M4 32 GiB development Mac. Linux/Windows,
low-end PCs, native installer/CA/proxy/suspend behavior, actual clients and real
coding-task success remain unverified by this benchmark.

Native Task 6 uses a separate, short observation driver for the exact packaged
binary. It runs two isolated save-only wizards: one direct llmgw process for PID
RSS samples at 20 ms intervals, and a second `/usr/bin/time -l` command for the
macOS `wait4` command-tree maximum and its raw supporting line. It then records
ten idle worker PID RSS samples at 100 ms intervals and every raw monotonic `on`
and `off` duration over five cycles. The command-tree OS peak can include
descendants; each manual-client wizard launches no client version query. A
sampled maximum can miss peaks between observations. These descriptive samples
are not percentiles, low-end-host proof, or a rerun of the quota/accounting
benchmark.

For the source-ready macOS arm64 release binary with SHA-256
`cf7c436c442c426a6e9a1d485a5261fe5e5a05c8b131b130e679e00eec2c646b`, one
pre-freeze pair of wizard observations recorded an 8,880,128-byte command-tree
OS peak and an 8,716,288-byte two-sample direct wizard PID maximum. Ten idle worker PID
samples were each 10,010,624 bytes. Five raw `on` durations ranged from 34.8 to
50.2 ms and five raw `off` durations from 33.8 to 47.4 ms. The final evidence
retains every raw value and repeats the observation only after the packaged
binary identity is fixed; these figures remain development-host observations.

Quota measurement alone is 100 minutes; full startup holds add 20 minutes.
Proxy idle sampling adds 15 seconds. Healthy no-wait runs add roughly seconds,
while quota drain adds zero to at most 43 minutes 20 seconds (20 × 130 seconds).
Thus the expected full healthy-fixture matrix is about **120–164 minutes**, plus
small readiness/resource/cleanup overhead. This is planning arithmetic, not a
measured duration. No production window is shortened. The `--smoke` option is a
separate ~15-second unlimited-quota validation with compressed arrivals, never
labelled a quota run. `--phase no_wait` supports bounded transport preflight.

Each run opens an exclusive `.events.jsonl` journal before submission, preserving
the request manifest, terminal events, end-of-measurement pending IDs and PID.
A validated run becomes its exclusive `.json`, and `pilot.json` is atomically
updated after each run. Failures retain `.failed.json` plus pending IDs and the
original journal. Re-running the identical command skips verified completed runs
only when source/binary/seed/window/schedule identity agrees and every previously
recorded artifact still matches its expected path, SHA256 and content
ID/phase/seed/arm. The entire run directory is checked before manifest writes or
any new arm, including when no output manifest exists. Duplicate or unexpected
IDs, changed/missing files, redirected paths, unregistered outputs and incomplete
journals are rejected. A crash after writing a completed run but before recording
it in the manifest leaves an orphan: it is preserved for inspection and is never
automatically adopted, even if its filename or content looks valid. Use an
exclusive new output after investigating such interrupted state.

This resume preflight and its synthetic-file regressions were added after the
measured matrix. The measured source archive and raw artifacts remain unchanged,
separate from this corrected review source. An incomplete journal refuses
automatic duplicate execution: first inspect the recorded PID
and existing execution session. Live runs are continued through that handle;
interrupted work needs an explicitly new phase/output after preserving the failed
run. Startup now journals the acquired child PID before readiness and retains
cleanup responsibility until successful return transfers the Process handle to
the run. Startup errors and cancellation terminate and reap that owned process
(3 seconds for termination, then 3 seconds after kill), preserve the original
exception, and close the parent log handle. Repeated cancellation cannot interrupt
this cleanup; a cleanup failure is attached to the original exception. This
post-measurement lifecycle fix changes failure handling and PID journal timing,
without changing the HTTP measurement or gateway executable.
Cancellation/stop cleans owned processes only. Full completion is never
inferred merely from an idle process or missing output.
