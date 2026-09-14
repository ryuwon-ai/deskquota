# Core Task 5 — quality-review correction round

Status: **DONE / HOLD — source and binaries fixed for the same quality reviewer
to rerun the original ordering counterexamples. Quality acceptance is pending.**

The original Task 5 result remains in `task5-results-initial.md`; the preceding
spec-review correction result is preserved in `task5-results-spec-fix.md`.
Initial Task 5, spec-fix, Task 4 and independent reviewer logs/artifacts were not
overwritten. This round's evidence is under `task5-quality-fix/`.

## Narrow correction and behavioral evidence

The quality review identified a Messages ordering defect: `message_start` and
`message_delta` could still assign usage after `message_stop`, establishing or
changing accounting usage that was required to qualify before stop.

The runtime correction adds an early terminal check inside each of the two
usage-bearing branches in `src/protocol/messages.rs` (eight lines). A post-stop
usage-bearing start/delta marks accounting usage invalid and returns before
assigning counters or qualifying final output. Existing sticky errors remain;
non-accounting events do not trigger the guard. No broad state machine,
dependency, feature, mode, retry or forwarding change was introduced.

Six real-wire regressions were added to `tests/task5_usage_contract.rs`:

| Sequence/control | Before fix | After fix |
|---|---|---|
| START(input 1/output 0) → STOP → DELTA(output 1) | Incorrect debit 2 | Unknown, original 335 reservation retained |
| STOP → START(input 1/output 0) → DELTA(output 1) | Incorrect debit 2 | Unknown, 335 retained |
| Previously qualified cached usage → STOP → usage-bearing DELTA | Incorrectly revises debit to 32 | Unknown, 335 retained; identical post-stop update also rejected |
| Previously qualified cached usage → STOP → usage-bearing START | Incorrectly revises debit to 32 | Unknown, 335 retained; identical post-stop update also rejected |
| Ordered START → DELTA → STOP | Correct debit 2 | Correct debit 2 |
| Qualified cached usage → STOP → ordinary non-accounting events/repeated STOP | Correct debit 31 | Correct debit 31 |

The local regression fixture uses the configured output default 300 and a
35-byte request, giving 335 estimated units. The independent reviewer used an
explicit-cap request whose byte count gave 352. The ordering defect and required
reservation retention are the same; the two fixture estimates are not conflated.
All new cases traverse real ephemeral loopback HTTP, check status and exact SSE
body-byte preservation, one attempt/cleanup and zero active slots at EOF. Debit
and known/unknown assertions occur after controlled gateway shutdown.

- `usage-red.log`: **4 assertion failures / 2 passing controls** against the
  unmodified reviewed source. No compile or fixture setup error.
- `usage-green.log`: the same focused **6 passed / 0 failed** after the guard.
- `usage-suite-green.log`: all **27 usage contracts** passed, including prior
  cumulative/cache, Chat/Responses, explicit-error, repeat and Reserved controls.

Exact focused commands, run from the product directory:

```sh
cargo test --locked --test task5_usage_contract quality_messages
cargo test --locked --test task5_usage_contract
```

`docs/runtime-contract.md` clarifies the existing accepted requirement that final
Messages usage must precede stop. The ledger, other observers, HTTP EOF/drop
ownership, window/reservation semantics and wire forwarding remain unchanged.

## Fresh verification after the last Rust source edit

| Check | Exact command | Observed result |
|---|---|---|
| Format | `cargo fmt --all -- --check` | exit 0 |
| Locked check | `cargo check --locked --all-targets` | exit 0 |
| Clippy | `cargo clippy --locked --all-targets -- -D warnings` | exit 0 |
| Debug regression | `cargo test --locked --all-targets` | 138 passed, 0 failed |
| Release regression | `cargo test --locked --release --all-targets` | same 138 passed, 0 failed |
| Debug binary | `cargo build --locked` | exit 0 |
| Release binary | `cargo build --locked --release` | exit 0 |
| Final-binary Pi positive | `python3 scripts/probe_pi.py --binary target/debug/llmgw --output artifacts/task5-quality-fix/pi-e2e.json` | completion/read-tool flows both passed, attempts 1 and 2 |
| Final-binary gateway-off | `python3 scripts/probe_pi.py --binary target/debug/llmgw --gateway-off --output artifacts/task5-quality-fix/pi-e2e-gateway-off.json` | expected exit 1; assistant failures and attempts 0 and 0 |

Per-profile Rust counts: lib 4, config 19, quota 12, stream 32, wire 44,
usage contracts 27 = **138**. All preceding 132 tests remain passing.
Raw outputs are `task5-quality-fix/{fmt,check,clippy,debug-tests,release-tests,debug-build,release-build,pi-positive,pi-gateway-off}.log`.
`verification.json` records exact commands/exits/counts/hashes.

The Python unit suite was **not rerun this round**: both Python files are
hash-verified unchanged from the preceding held source. The prior **5 passed**
evidence remains `task5-spec-fix/pi-python-tests.log`; the comparison hashes are
recorded in this round's verification JSON. This is distinct from the fresh
actual Pi positive and gateway-off executions above.

Held SHA-256:

- Debug: `4578b4492177512d653b53bcd2a85a6fd71f35be686900d76d6706f1058b36da`
- Release: `8011efa55a45c3223353c484aba43d7889c8f18257656bbe606ce98226ae1c1d`

Both Pi artifacts record the unchanged final debug binary hash. No Rust source
changed after final verification/build. `task5-quality-fix/held-source.json`
fingerprints 36 final product inputs and both binaries. `changed-files.json`
compares against the prior spec-fix HOLD: only Messages observer, existing usage
tests and runtime documentation changed.

## Scope and remaining acceptance

Only owned synthetic loopback fixtures and the existing isolated temporary Pi
harness ran. No Git operation, real provider/model call, current user-client
configuration change, OS service, wildcard listener or reviewer-file edit ran.
Existing artifacts remain preserved. The production startup hold and the
explicit Unlimited/Unknown Pi transport fixture remain unchanged.

No performance, tokenizer accuracy, provider-compliance or OS portability claim
is made. The initial Task 5 limits concerning local windows, estimated input,
external consumption and unknown server token timing remain. Parent will run
matching-binary CLI/stream checks and the same quality reviewer will rerun the
original counterexamples. Task 6 starts only after quality acceptance.
