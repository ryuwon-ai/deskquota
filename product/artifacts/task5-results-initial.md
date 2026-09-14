# Core Task 5 — quota ledger implementation result

Status: **DONE — source/binaries held for parent spec review, then independent quality review.**
Implementation and synthetic acceptance are complete; review acceptance is not claimed.

## Directly verified

All commands ran from `product/`, which has no `.git` directory. No Git mutation,
real LLM/API call, persistent user configuration change, OS service, or reference
clone modification was performed. Network fixtures were synthetic loopback servers
on ephemeral ports. Actual installed Pi used the existing isolated temporary-state
harness; Task 4 artifacts were preserved.

| Check | Command | Observed result |
|---|---|---|
| Format | `cargo fmt --all -- --check` | exit 0 |
| Locked all-target check | `cargo check --locked --all-targets` | exit 0 |
| Clippy | `cargo clippy --locked --all-targets -- -D warnings` | exit 0 |
| Debug regression | `cargo test --locked --all-targets` | 111 passed, 0 failed |
| Release regression | `cargo test --locked --release --all-targets` | same 111 passed, 0 failed |
| Debug binary | `cargo build --locked` | exit 0 |
| Release binary | `cargo build --locked --release` | exit 0 |
| Pi harness regression | `python3 -m unittest discover -s tests -p test_pi_probe.py -v` | 5 passed (4 prior + explicit quota fixture precondition) |
| Actual Pi positive | `python3 scripts/probe_pi.py --binary target/debug/llmgw --output artifacts/task5/pi-e2e.json` | completion 1 attempt; read-tool flow 2 attempts; both passed |
| Actual Pi gateway-off | same command with `--gateway-off --output artifacts/task5/pi-e2e-gateway-off.json` | expected exit 1; both flows assistant failure, 0 attempts, no output marker |

Rust per-profile counts: lib 4, config 19, quota 12, stream 32, wire 44 = **111**.
This preserves the 83-test baseline and adds 28 Rust tests. All Rust source edits
preceded the final Rust checks and builds. Only the Pi Python fixture/test and
runtime/result documentation changed afterward; neither Rust source nor either
binary changed during the final Pi probes.

Binary SHA-256:

- Debug: `a5edaa0c559a043272119e4b21493a2e6cd91889045df25e6f9eea816de47b76`
- Release: `392142c6c5fab00fe3ea3d1299717f03204de8bc5af9717c7a8cb1f60e2d9b80`

`task5/verification.json` contains command results, test counts and these hashes.
`task5/changed-files.json` compares source files with the Task 4 manifest;
`task5/held-source.json` fingerprints the held product inputs. Full check output
is retained in `task5/{fmt,check,clippy,debug-tests,release-tests,debug-build,release-build}.log`.

## RED/GREEN and concrete acceptance evidence

- `task5/ledger-red.log`: all 8 initial shared-library behavior tests failed by
  assertion against a permissive compiling scaffold. `ledger-green.log` passes
  those same tests. The final suite extends this to 12 quota contracts: 60-second
  restart/window boundaries, RPM only at start/no early refund, provisional
  ownership without expiry, joint capacity/no waiting holds, reserved and actual
  accounting, unknown usage, debt, late usage, duplicate identities, resume,
  checked estimated arithmetic, and the fixed retained-entry ceiling.
- `wire-policy-red.log` / `wire-policy-green.log`: real loopback HTTP rejects
  missing output bound and unsupported media/estimate/cap forms before upstream.
  `wire-startup-red.log` / `wire-startup-green.log`: the real production entry
  point holds known RPM at startup; the test waits only 30 ms, never 60 seconds.
- `parser-scope-red.log` / `parser-scope-green.log`: opposite-direction
  counterexamples caught irrelevant Chat `input` being interpreted as media,
  and Responses tool-output media being missed. Both pass after scoped scanning.
- `start-notify-red.log` / `start-notify-green.log`: production-clock coordinator
  waiter would remain asleep when a provisional RPM hold started. Paused Tokio
  reproduces this without a 60-second sleep; the start transition now notifies
  waiters so they schedule its rolling-window expiry. Installed Tokio 1.49
  `test-util` feature/source were inspected before enabling it only as a dev
  dependency. Normal runtime dependencies and Cargo.lock are unchanged.
- `error-guidance-red.log` / `error-guidance-green.log`: quota errors include
  fixed actionable explanations and distinguish estimated cost from actual tokens.
- Additional regression sensitivity was verified by temporarily omitting the
  corresponding cleanup/settlement call, observing assertion failures, and restoring
  identical source bytes. These are deliberately named `*-omission-red.log`
  (prestart cancel, unpolled owner, Actual settlement, long-stream settlement),
  not presented as original discovery failures. The final full suites verify the
  restored behavior.
- Wire tests exercise explicit caps without a model default, default/explicit
  precedence, UTF-8 byte cost and unchanged forwarded bytes, escaped duplicate
  caps, malformed/overflow caps, generation count, text vs media and opaque
  tool/schema fields, and all five endpoints through one admission path.
- Models/count_tokens use RPM 1 and generation TPM 0, accept no output bound,
  and share RPM across root aliases. The same live gateway waits at startup,
  blocks after two attempts, remains blocked at virtual 119 s and proceeds at
  120 s. No HTTP test header carries a fixture cost.
- Prestart gate + actual downstream RST: admitted hold survives virtual time
  60→500, then cancels with **0 upstream attempts and exactly 1 hold cleanup**.
  A subsequent request succeeds. Stop also cancels an admitted unstarted worker;
  queue timeout leaves zero resources held and zero attempts. An unpolled worker
  future owns the RAII hold and releases it when dropped.
- Actual SSE wire usage: Chat cached input is not added twice (20+3=23),
  Responses cached subset is not added twice (13+5=18), Messages includes separate
  cache creation/read once (17+9+3+2=31). Missing/invalid usage retains the estimate.
- Long draining stream: the original reservation expires while the worker still
  owns capacity; terminal marker does not release it. At final HTTP EOF, known
  usage is a new 150-token debit at virtual 130 s, blocking the next attempt at
  189 s and allowing it at 190 s. Separate observed input/output metrics remain
  reported counts. Full existing stream close/drain/abort/backpressure regressions
  pass in both profiles.

## Pi fixture correction and evidence boundaries

The original Task 4 Pi fixture actually configured RPM **known 60**, despite the
handoff description saying unlimited. Its first Task 5 positive run correctly
encountered production startup warmup and both Pi flows hit the harness timeout
with zero upstream attempts. This was a transport fixture precondition omission,
not evidence that the production warmup should be disabled.

The failed artifact/log are retained as
`task5/pi-e2e-warmup-fixture-failure.json` and
`task5/pi-positive-warmup-fixture-failure.log`. The added Python fixture test first
failed (`pi-fixture-red.log`), then passed after only the temporary Pi transport
config changed to RPM unlimited / TPM unknown. Final artifacts record both modes
under `harness.quota_fixture` and configured policy names. The known-60 example
and production policy remain unchanged. Existing `artifacts/pi-e2e*.json` Task 4
files were not overwritten. Final positive/negative runs used the same held debug
binary and installed Pi 0.84.2; all owned children were cleaned up.

## Code/document basis and deliberate limits

`docs/runtime-contract.md` specifies the local 60-second ledger, HTTP-start
reservation origin, one owner across admission/start/terminal/drop, text estimator
forms, cap/error semantics, cache categories, expiry/debt/unknown rules, restart
hold and test seams. `src/admission/quota.rs` owns the explicit-time ledger;
`src/admission/mod.rs` owns joint waiting and RAII; `src/protocol/request.rs`
contains the selective request visitor. Existing HTTP/stream execution uses this
one coordinator instead of a parallel quota engine.

The ledger retains at most 8,192 active/unexpired entries and waits conservatively
when full; it never discards an unexpired debit. IDs do not wrap/reuse, completed
identities are pruned after their debits expire, and new lifecycle counters
saturate. This fixed ceiling is a storage bound, not a throughput result.

Unknown and unlimited remain distinct. The complete JSON UTF-8 byte count plus
output reservation is a proxy, not an exact tokenizer, universal upper bound or
proof of provider quota compliance. Unsupported/malformed content with known TPM
is rejected within the documented initial support boundary. Provider windows,
other PCs' consumption, actual server token timing, billing, OS suspend-clock
behavior, Windows/Linux behavior, latency/throughput/RSS and real LLM quality
remain unverified. No performance claim is made.

Task 6 owns explicit queue/root RR/bypass/64-waiter policy. Task 7 owns retry,
cooldown and broader measurement. Parent will independently run its CLI/stream
probes against the held binaries and perform spec then quality review; those
results are not claimed in this implementation report.
