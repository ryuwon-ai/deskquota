# Task 3 results — stream lifetime, cancellation, and observation

Date: 2026-09-12

## Scope implemented

- Added one supervised upstream worker per admitted request. The worker owns the
  HTTP attempt from before response headers, the response body, one execution
  permit, bounded downstream delivery, and exactly one terminal outcome.
- Enforced configured concurrency `1..=16` for all five data endpoints. This is
  the minimum Task 3 capacity seam; RPM/TPM admission, the 64-item scheduler,
  fairness, cooldown, and retry remain later tasks.
- Added `cancel_policy = "drain"` by default and explicit `"close"`. Drain
  discards post-disconnect wire bytes while observing bounded metadata and holds
  capacity through real body EOF/error/deadline. Close drops the local HTTP
  attempt. It does not claim provider compute termination.
- Added an eight-item downstream channel plus a 64 KiB per-response byte budget.
  Payload is copied into chunks of at most 16 KiB, avoiding retention of a
  larger upstream backing allocation through a small `Bytes` slice. Currently
  executing workers contribute at most 1 MiB at the maximum concurrency of 16,
  but finished upstream workers can release their slots while outstanding
  downstream responses still retain their queued bytes. The implementation
  therefore does not claim a 1 MiB process-wide bound. Aggregate outstanding
  response memory, Reqwest, Hyper, socket buffers, and exact RSS remain Task 7
  measurements.
- Separated payload delivery from one terminal body signal. Only successful
  upstream body EOF produces successful downstream EOF; deadline, body error,
  and forced interruption abort HTTP framing even when the payload queue is
  full.
- Added a 256 KiB maximum retained SSE metadata contract without changing wire
  bytes. One fixed aggregate storage design replaces separately growing line,
  data, event, and BOM vectors. The observer handles split UTF-8 JSON, BOM,
  CR/LF/CRLF, multiline data, comments, terminal markers, incomplete EOF, and
  endpoint-specific usage.
- Added stop ordering: stop wins ready admission branches, the listener is
  dropped before drain, keepalive/capacity waiters cannot begin new attempts,
  active work gets the production 10-second grace period, and the force signal
  makes the supervisor abort and join every worker before shutdown returns.

## TDD evidence

The config RED is `task3-config-red.log`: the new test failed to compile because
`CancelPolicy` and `Config.cancel_policy` did not exist. The targeted GREEN is
`task3-config-green.log`.

The first stream RED in `task3-stream-red.log` is retained but is not accepted
as authoritative behavioral evidence. Its fixture raw byte strings encoded
literal backslash-n sequences instead of SSE newlines, and a zero-delay status
connection loop exhausted ephemeral ports. Only its early real failures — the
drain request releasing capacity and close not terminating the upstream socket
— were meaningful.

`task3-stream-red-corrected.log` fixed the newlines and used a bounded 20 ms
status interval. It contains 13 actual loopback fixture cases: 3 existing raw
stream/stop behaviors passed and 10 new lifecycle/observer assertions failed.
The failures included concurrency admitting attempt 2 before EOF, queued
cancellation starting attempt 2, close not closing the fixture HTTP socket,
and absent terminal/usage/queue observations. It contains no
ConnectionRefused- or ephemeral-port-only substitute RED.

Independent specification review then reproduced four additional concrete
failures. Product regressions in `task3-spec-fix-red.log` recorded: 331,072
bytes of retained decoder vector capacity without overflow; constructed
concurrency 0 accepted by public spawn; Chat cached=7 reported as zero while
negative cache and output 10→2 remained known; and a saturated 300 ms deadline
ending a partial HTTP 200 with a successful chunked terminator. The same RED
also showed the new upstream-body-error regression already passed because the
old payload channel happened to have room for its error item; the fix removes
that queue-capacity dependency.

The pre-review targeted GREEN is `task3-stream-green-final.log` (24 tests).
`task3-spec-fix-green.log` contains the focused correction GREEN, and the full
final `task3-verification.log` contains the complete final source. The final
stream suite has 31 actual TCP tests:

- required first-event-before-EOF, split UTF-8/SSE byte preservation, drain
  capacity retention, close socket termination, huge-event unknown usage, and
  exactly-once terminal cleanup;
- pre-header RST drain/close, silent valid request half-close, queued-before-start
  cancellation, metadata capacity sharing, slow-reader byte saturation, and
  graceful stop with listener/keepalive admission blocked;
- short internal fixture deadlines while upstream headers are silent, body is
  silent, and the downstream 64 KiB delivery budget is saturated;
- forced shutdown with the upstream EOF gate held, asserting post-join
  `active=0`, one terminal outcome, and `terminal_shutdown=1` before the test
  helper returns;
- Chat empty-choices usage, Responses nested completed usage, Messages
  cumulative output, provisional Messages usage remaining unknown, split BOM /
  CRLF / multiline data, huge comments, and incomplete trailing events.
- saturated deadline and upstream-body error framing failure, constructed
  concurrency rejection with valid 1/16 boundaries, Chat cache subset and
  explicit-zero accounting without input double-add, malformed/decreasing usage
  becoming unknown, and analogous Responses/Messages invalid usage cases.

The decoder allocation regression is a library unit test because retained
buffer storage is intentionally private. It recreates the reviewer multiline
data, 60,000-byte unknown event name, and unfinished 131,071-byte comment and
checks a 256 KiB aggregate retained allocation without poisoning later usage.

The short deadline and shutdown values are available only through a doc-hidden
library fixture seam which is compiled in every optimization profile so the
same integration suite runs in debug and release. The user configuration schema
and production 30-minute request, 120-second capacity-wait, and 10-second
shutdown values were not extended with test knobs.

## Quality review correction

The first independent quality review found that the doc-hidden fixture seam was
conditioned on `debug_assertions`. Its release all-target check therefore failed
while compiling two unconditional integration-test references. The seam is now
optimization-profile independent, so debug and release compile and execute the
same 83-test suite. `task3-quality-fix-green.log` is the focused successful rerun
of the reviewer's failing release all-target command.

The same review found that the earlier 1 MiB wording conflated execution permit
lifetime with downstream response lifetime. This document and the runtime
contract now claim 64 KiB per outstanding response and at most 1 MiB contributed
by the currently executing workers. They explicitly leave aggregate memory
across finished-upstream, slow-downstream responses to Task 7 measurement.

## Full verification

The original Task 3 commands ran in order and exited successfully in
`task3-verification.log`. After the quality corrections, the following commands
ran against the final files and exited successfully. Raw output is in
`task3-quality-fix-verification.log`.

```text
cargo +1.88.0 fmt --check
cargo +1.88.0 check --locked --all-targets
cargo +1.88.0 clippy --locked --all-targets -- -D warnings
cargo +1.88.0 test --locked
cargo +1.88.0 check --locked --release --all-targets
cargo +1.88.0 clippy --locked --release --all-targets -- -D warnings
cargo +1.88.0 test --locked --release
cargo +1.88.0 build --locked --release
cargo +1.88.0 run --locked -- --help
```

The final suite has 83 passing tests in both debug and release: 2 library unit
tests, 19 configuration tests, 31 stream-lifetime tests, and 31 Task 2
wire/startup regressions. CLI help still lists only `run`, `doctor`, and
generated `help`.

## Directly verified boundaries

Directly verified with synthetic loopback TCP: pre/post-header RST, valid
request write-half-close, body EOF distinct from protocol terminal markers,
capacity held through drain, close-policy local HTTP socket termination,
pre-start cancellation, metadata capacity sharing, a 64 KiB queued response
payload maximum at concurrency one, deadline behavior, graceful and forced
shutdown cleanup, byte-identical streaming, endpoint usage rules, observer
overflow and aggregate retained capacity, failed downstream framing on
non-EOF terminal outcomes, public-start concurrency bounds, and all previous
request/query/auth/gzip/body-budget regressions.

The forced library test uses a 100 ms internal grace period and checks worker
metrics after the supervisor join. It does not sleep through the production
10-second constant. The separate foreground CLI probe is research evidence
owned outside this product directory.

## Remaining scope and uncertainty

- A local HTTP socket close does not prove a provider stopped GPU, KV-cache,
  quota, or billing work. Backend abort requires separate provider-specific
  verification.
- FIN-only application closure is ambiguous with a valid HTTP request
  write-half-close. Immediate silent-upstream disconnect is proven for RST and
  connection error, not every TCP close form.
- Exact RSS, Reqwest/Hyper transport-buffer peaks, many concurrent large
  chunks, production 30-minute timeout passage, production 10-second forced
  stop passage, Windows runtime behavior, sleep/restart behavior, and real LLM
  behavior were not measured here.
- No real API/model/paid call, user client configuration, OS service, wildcard
  listener, cache/replay, protocol translation, quota ledger, fair scheduler,
  cooldown, or gateway retry was added or exercised.
- This directory has no Git repository. The logs and local source are reviewable
  filesystem evidence, not a commit or release provenance claim.
