# Core Task 5 fresh quality review — CHANGES REQUESTED

Reviewed 2026-09-12 after the separate spec re-review passed. The review covers the complete Task 5 change from Task 4, including request inspection, joint admission, wait/start ownership, transport cleanup and settlement, the observer corrections, tests, runtime documentation and the Pi quota-fixture change. One new Important accounting defect is independently reproduced. **Not ready for Task 6; HOLD for a narrow correction.** Product source, tests, documentation and held binaries were read-only throughout this review.

## Strengths

- The ledger and execution capacity have one owner. An admitted RAII hold owns both; waiting candidates hold neither. Explicit start commitment and drop cleanup keep cancellation before the first HTTP poll distinct from a provider attempt. Notification registration precedes the locked decision, consistent with the installed Tokio 1.49.0 `Notify` documentation/source; no speculative scheduler layer was added.
- Explicit monotonic timestamps, bounded retained entries, nonwrapping reservation identities, checked cost addition and wide aggregate sums keep expiry and late debt separate from a new request's ownership. A new mixed saturation/late-debt probe passes in both profiles, including debt greater than `u64::MAX` in aggregate.
- The selective request visitor keeps policy inspection separate from raw forwarding, uses the original UTF-8 body byte count, rejects ambiguous inspected caps, and treats tool schemas/arguments as opaque. Fixed actionable errors distinguish estimation from actual provider usage. Models/count-tokens retain their explicit metadata treatment. These are source-review conclusions; no tokenizer or provider-compliance claim follows.
- Tests use meaningful boundary observations: actual upstream attempt counts, admitted-but-unsent gates, response EOF, body-byte preservation and debit/cleanup assertions. The independent 20-contender control confirms provisional holds survive manual-clock expiry, eight prestart RST cancellations release once without attempts, and the twelve surviving requests complete with exactly twelve committed attempts. The separate debug and release failures below show why passing broad suites alone cannot establish supported final usage.

## Important findings

### 1. [P2] Do not qualify final Messages usage received after `message_stop`

**Directly reproduced in debug and release.** With known TPM and explicit Actual accounting, both of these synthetic SSE sequences reduce a 352-token reservation to 2 after successful HTTP EOF:

1. `message_start` (input 1, output 0) → `message_stop` → `message_delta` (output 1).
2. `message_stop` → `message_start` (input 1, output 0) → `message_delta` (output 1).

Neither has the required final delta usage before `message_stop`, so final accounting usage must remain unknown and retain 352 until its original expiry. The ordered `message_start` → `message_delta` → `message_stop` control correctly settles to 2. All three wire cases preserve status 200 and exact response-body bytes, observe one upstream attempt and one cleanup, and shut the gateway down before the result assertion.

At [messages.rs:44](../../product/src/protocol/messages.rs#L44), `message_delta` still assigns usage and sets `final_output_usage_seen` after the terminal flag has been set at line 64. The adjacent `message_start` branch at line 39 also keeps populating usage after stop. [finish at line 108](../../product/src/protocol/messages.rs#L108) tests `terminal` and `final_output_usage_seen` independently, losing their ordering. This contradicts the documented [supported Messages final-usage ordering](../../product/docs/runtime-contract.md#L328) and feeds an unsupported value through [Hold cleanup](../../product/src/admission/mod.rs#L150) into Actual settlement. The concrete consequence is 350 units of extra admission budget, while wire bytes remain unchanged.

**Minimal direction:** prevent accounting-bearing `message_start`/`message_delta` events after stop from establishing or changing supported final usage; keep this sequence conservatively unknown. Add both ordering regressions and retain the ordered positive control, valid cumulative usage, ordinary non-accounting events and sticky error behavior. A broad protocol state-machine rewrite is unnecessary. No fix was implemented by this reviewer. This is a new Messages ordering defect, not a reopening of the resolved Chat final-shape or Responses terminal-conflict findings.

## Critical / Minor findings

No Critical or separate actionable Minor issue found in this bounded review. No styling requests or deferred Task 6–8 requirements are included.

## Independent verification

[tests/review.rs](tests/review.rs) links the held product library. Its new wire assertions reuse the read-only product synthetic socket helper; the fixture is not substituted for the production ledger, parser, server or forwarding worker. All listeners bind ephemeral IPv4 loopback. Deterministic manual time affects the production ledger seam only; transport and Tokio task scheduling run normally. The retained-entry probe calls the real ledger directly.

| Independent check | Debug | Release |
|---|---|---|
| Normal Messages final ordering | Pass: debit 2 | Pass: debit 2 |
| Messages start → stop → delta | **Fail: debit 2, expected 352** | **Fail: debit 2, expected 352** |
| Messages stop → start → delta | **Fail: debit 2, expected 352** | **Fail: debit 2, expected 352** |
| 20 contenders, 16 provisional slots, clock advance, 8 prestart RST cancellations | Pass: 20 cleanups / 12 attempts / 0 active / 0 held TPM; 12 reservations retained | Same pass |
| 8,192 retained entries, expiry with one old active identity, fresh request and late `u64::MAX` debt | Pass: no dropped debit, wrap, duplicate cleanup or cross-settlement | Same pass |

Final locked suites: **3 passed / 2 failed per profile, exit 101**. Each profile includes 23 wire ingress requests (15 upstream attempts, eight prestart cancellations) plus one ledger scenario. Failures are the expected accounting assertions after fixture cleanup, not compilation or transport setup failures. These checks are not load, latency or RSS benchmarks.

Exact commands, from `/Users/ryuwon/Desktop/ryuwon-project/llm-gateway-research`:

```sh
cargo test --offline --manifest-path evidence/task5-quality-review/Cargo.toml --target-dir evidence/task5-quality-review/target -- --nocapture
cargo test --offline --locked --manifest-path evidence/task5-quality-review/Cargo.toml --target-dir evidence/task5-quality-review/target -- --nocapture
cargo test --offline --locked --release --manifest-path evidence/task5-quality-review/Cargo.toml --target-dir evidence/task5-quality-review/target -- --nocapture
```

The first run created only the review Cargo.lock offline and covered the initial four tests (2 pass / 2 fail); its log is preserved in [debug-probes.log](debug-probes.log). The final five-test runs are [debug-locked-probes.log](debug-locked-probes.log) and [release-probes.log](release-probes.log). All **205 shared Cargo package records**, including versions, sources, checksums and dependency edges, match the product lock exactly. The independent compilation target was removed after completion; manifest, lockfile, tests and logs remain reproducible.

## Source review and limits

[source-and-lock-comparison.json](source-and-lock-comparison.json) identifies the 18 changed/added inputs from the Task 4 baseline and the seven-input observer correction from initial Task 5. The complete held input set contains 36 files. There is no Git repository or Git diff; the review compares recorded file sets/hashes and reads the source directly.

Beyond the finding above, source review found no additional actionable issue in atomic quota/capacity ownership, initial known-quota hold, unsent reservation lifetime, cancel/stop/deadline cleanup, first-poll attempt commitment, expiry/start notification, retained-entry bounds, monotonic time clamp, cache category totals, checked arithmetic, fixed observer storage, unchanged wire forwarding, output-cap/default precedence or selective input-path policy. The installed dependency's documented notification pattern and the existing product research on reservation receipts/conditional budget guards informed this assessment. The Pi fixture explicitly selects unlimited RPM / unknown TPM in temporary config and identifies that mode in artifacts; its regression checks that exact parsed config. No runtime dependency was added by Task 5; `test-util` is dev-only.

The 132-test/profile product regressions, Python five-test suite, actual Pi flows and held-CLI lifecycle probes are implementer/parent evidence inspected or referenced for context, not independent runs claimed here. The prior spec review and re-review findings remain preserved and resolved. This quality review makes no claim about root fairness or the 64-request queue (Task 6), retries/cooldown/global resource bounds (Task 7), performance (Task 8), real providers/models, Windows/Linux, OS suspend or other-PC consumption. It does not demand native setup/on/off/wizard features in this task.

## Held identities and assessment

[identity-before.json](identity-before.json) and [identity-after.json](identity-after.json) compare all **36 held inputs: 36/36 unchanged before and after review**. Both held CLI binaries also remain unchanged:

- Debug SHA-256: `5fe45aca9910a31fb08e1d4dea4cadf7f1a53ba4a4f7b04410401d02e7b71713`
- Release SHA-256: `3cfd0e778e2dab57d96b6fa2aeb0c05ea1ef71616e3ab7383514f98bcd98f562`

Independent probes compiled the held library in a dedicated target directory. They did not execute or replace the held CLI binaries and do not establish binary build provenance. No product edit, real API/model call, download, client configuration, OS service, wildcard/non-loopback listener, external message or Git operation occurred.

**CHANGES REQUESTED; not READY FOR TASK 6.** Joint admission and lifecycle controls held in the independent checks, but the reproduced post-stop Messages usage creates an incorrect Actual refund. Correct this narrow ordering boundary, retain the counterexamples, and rerun the same independent quality checks before proceeding. **Reviewer HOLD after this verdict.**
