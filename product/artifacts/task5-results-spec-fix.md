# Core Task 5 — spec-review correction round

Status: **DONE / HOLD — corrected source and binaries held for the same spec
reviewer to rerun the counterexamples, followed by fresh quality review.**
Implementation verification is complete; independent review acceptance is pending.

The original Task 5 result is preserved unchanged in `task5-results-initial.md`.
All initial `task5/` artifacts, Task 4 Pi artifacts and reviewer artifacts remain
preserved. This round writes evidence only under `task5-spec-fix/` and updates
this summary plus the runtime contract.

## Changes and directly reproduced failures

The independent reviewer correctly identified that Task 5's Actual accounting
made two inherited observer mistakes capable of refunding unsupported usage:
Chat accepted nonfinal usage shapes, and Responses retained stale success usage
through conflicting terminal outcomes.

| Corrected behavior | Scope and evidence |
|---|---|
| Chat final qualification | Only numeric usage from `choices: []` before `[DONE]` supplies final accounting usage. Nonempty, absent and malformed choices cannot supply it. Null deltas and numeric interim observations may precede a valid final report. Cache-read remains an inclusive input subset. |
| Responses terminal conflicts | Failed/incomplete/error events invalidate usage in either order relative to completed. Contradictory nested completion status/error and a later completed report lacking usage cannot reuse stale successful usage. Repeated consistent completed reports remain supported. |
| Adjacent explicit errors | Chat error objects and Messages standard errors make usage unknown. Messages before/after final usage/stop was independently reproduced in this round before the correction; the parent explicitly included this adjacent Actual-accounting input in scope. |
| Named SSE error | The fixed-size name classifier preserves `event: error`; it invalidates final usage even when the JSON omits its own type/error key. Other unknown events remain transparent. No buffer size, dependency, mode or retry policy changed. |

`tests/task5_usage_contract.rs` traverses the actual HTTP gateway with synthetic
loopback SSE, a manual monotonic startup clock and a known TPM budget. Each case
checks unchanged status/body bytes, exactly one attempt/cleanup, zero active
slots after EOF, the final debit, and known/unknown counters. Debit and accounting-usage
assertions run after controlled shutdown. Default Reserved and valid Chat/Responses/Messages cache
semantics are positive controls.

- `usage-red.log`: initial 19 tests, **15 assertion failures / 4 controls passed**.
  These are behavioral failures against the original source, not compile errors.
- `usage-partial-fix-red.log`: first narrow observer correction, **18 passed /
  1 failed**. The remaining named Messages error had been changed to `unknown` by
  the bounded SSE name classifier; this intermediate failure was not overwritten.
- `named-error-red.log`: additional named-error coverage, **0 passed / 3 failed**
  (Messages plus new Chat/Responses cases).
- `usage-green.log`: corrected final source, **21 passed / 0 failed**.

The focused command is `cargo test --locked --test task5_usage_contract`.
The named-error command adds the `named_error_event` filter. All commands run
from `/Users/ryuwon/Desktop/ryuwon-project/llm-gateway-research/product`.

These changes are local observation corrections. Worker ownership, HTTP EOF,
byte forwarding, slot release, the single ledger and estimator are unchanged.
Successful final usage still settles Actual; unsupported usage remains unknown
and retains its original reservation, while Reserved still keeps its estimate.

## Final verification after the last Rust edit

| Check | Exact command | Observed result |
|---|---|---|
| Format | `cargo fmt --all -- --check` | exit 0 |
| Locked check | `cargo check --locked --all-targets` | exit 0 |
| Clippy | `cargo clippy --locked --all-targets -- -D warnings` | exit 0 |
| Debug | `cargo test --locked --all-targets` | 132 passed, 0 failed |
| Release | `cargo test --locked --release --all-targets` | same 132 passed, 0 failed |
| Debug build | `cargo build --locked` | exit 0 |
| Release build | `cargo build --locked --release` | exit 0 |
| Pi harness | `python3 -m unittest discover -s tests -p test_pi_probe.py -v` | 5 passed |
| Actual installed Pi | `python3 scripts/probe_pi.py --binary target/debug/llmgw --output artifacts/task5-spec-fix/pi-e2e.json` | both flows passed, attempts 1 and 2 |
| Gateway-off control | `python3 scripts/probe_pi.py --binary target/debug/llmgw --gateway-off --output artifacts/task5-spec-fix/pi-e2e-gateway-off.json` | expected exit 1, assistant failures, attempts 0 and 0 |

Per-profile Rust counts: lib 4, config 19, quota 12, stream 32, wire 44,
spec-review usage contracts 21 = **132**. The initial 111 tests remain passing.
Raw logs use the corresponding check names in `task5-spec-fix/`; exact commands,
exits, counts and hashes are also in `task5-spec-fix/verification.json`.

Held binary SHA-256:

- Debug: `5fe45aca9910a31fb08e1d4dea4cadf7f1a53ba4a4f7b04410401d02e7b71713`
- Release: `3cfd0e778e2dab57d96b6fa2aeb0c05ea1ef71616e3ab7383514f98bcd98f562`

No Rust source changed after these checks/builds. Both final Pi artifacts record
the same unchanged debug binary hash and temporary RPM unlimited / TPM unknown
fixture. The production known-quota restart hold is unchanged. All persistent
user files, original Task 4 and initial Task 5 evidence were preserved. No real
LLM/API, OS service, wildcard listener, Git mutation or reviewer-file edit ran.

## Code/document basis and limits

Changed runtime files in this correction: `src/protocol/completions.rs`,
`responses.rs`, `messages.rs`, `mod.rs`, and the bounded SSE name enum in
`src/transport/stream.rs`. `docs/runtime-contract.md` now spells out supported
final reports and sticky explicit-error handling. A new integration test file
contains the 21 regressions. `task5-spec-fix/changed-files.json` compares this
round to the original held Task 5 inputs, and `held-source.json` fingerprints the
36 final source inputs and both binaries.

The initial Task 5 estimator and accounting limitations still apply: local
60-second windows, byte proxy rather than exact tokenizer, unknown provider
windows/other-PC consumption/billing and server token timing, unverified OS
suspend semantics and Windows/Linux behavior. No performance or real-provider
compliance claim is made. Queue fairness/bypass belongs to Task 6; retry/cooldown
and wider measurement belong to Task 7. Independent matching-binary CLI/stream
probes and spec re-review/quality review remain parent/reviewer work.
