# Native Task 1 final review handoff

Status: **DONE_WITH_CONCERNS / HOLD for fresh SPEC, then QUALITY review**.
Product source edits and builds stopped at this handoff. This is implementation
handoff, not acceptance of Windows/Linux runtime or the complete native roadmap.

## Review target and scope

Use `source-manifest-final.json`, `source-hold-final.tar.gz`, and
`summary-final.json`. The archive contains 57 product source files plus the four
authorized research fixture helpers. Compared with accepted Task 8: 15 product
files changed and 7 added; the JSON lists all paths. No Git operations occurred.

The earlier `source-manifest.json`, `source-hold.tar.gz`, and `summary.json` are a
preserved pre-fix candidate. They are superseded for review: self-review found
that a stale `worker.log` could misclassify a new child failure. The final source
uses the spawned child's exit status instead. `red-stale-startup.log` captures
that concrete invalid-config-before-worker-lock counterexample; its GREEN is
`green-stale-startup.log`. Hidden worker exit 10 means typed port bind conflict;
public command failures remain 1. No new supervisor, result file or secret IPC.

Implemented: explicit global config path, on/off/restart/status JSON, foreground
run with the same lifetime ownership, canonical path-specific state, immutable
content fingerprint, nonce/token-authenticated identity and recorded bound
address, stable lifetime lock, command operation lock, native detach, 5-second
readiness, 10-second drain plus 2-second confirmation, quota hold/cooldown status,
protected files, and diagnostic state. `doctor --json` provides the typed
`state_directory` boundary for all affected fixture helpers. No old state-path
fallback or migration. Default config resolution/setup, adapters, autostart and
installers are outside Task 1; status reports them as not implemented.

Server delta: authenticated control stays observable during the fixed drain
window, while new data ingress returns 503. It shares the existing 128 socket
budget; control connections close after one response and cannot extend the
drain deadline. The previous core test expecting immediate physical listener
closure was changed to check data rejection and authenticated draining status.
Stream/request/body/upstream cleanup and the 10-second bound remain tested.

## Direct verification on this macOS host

| Evidence | Result |
| --- | --- |
| `red.log` | Initial behavior: 0 passed, 6 failed because lifecycle commands/state were absent |
| `final-debug-211.log` | 211 passed, 0 failed: prior 191 plus 20 lifecycle tests |
| `final-release-211.log` | 211 passed, 0 failed: same suite in release |
| `fmt-diagnostic-fixed.log` | cargo fmt check PASS |
| `check-diagnostic-fixed.log` | cargo check, all targets/features PASS |
| `clippy-diagnostic-fixed.log` | all targets/features, warnings denied PASS |
| `python-final.log` | 5 existing Python harness tests PASS |
| `benchmark-final-selfcheck.log` | regression harness self-check PASS |
| `cli-diagnostic-fixed.json` | PASS, 1 loopback upstream attempt |
| `stream-diagnostic-fixed.json` | 4 cases PASS, 7 attempts, forced drain near 10 seconds |
| `fairness-diagnostic-fixed.json` | 3 cases PASS, 77 ingress, 59 attempts |
| `retry-diagnostic-fixed.json` | 6 cases PASS, 21 ingress, 24 attempts |
| `pi-diagnostic-fixed.json` | installed Pi, isolated config/cwd; completion and read-tool flows PASS, 1/2 attempts, authenticated stop cleanup |
| `pi-negative-diagnostic-fixed.json` | expected failure observed, process exit 1; 0/0 attempts; not counted as successful gateway flow |
| `benchmark-smoke-final.json` | four short arms, 20 submitted each; each 17 completed/3 cancelled, all owned processes cleaned; no performance conclusion |
| `unix-session-final.json` | owned worker SID=PGID=PID; distinct parent session; all three standard descriptors `/dev/null`; no token in output/record; log 8 bytes after off; temp directory removed |

The 20 lifecycle tests cover concurrent on, repeated off, occupied port without
disrupting its listener, changed fingerprint/restart, stale PID/nonce, separate
same-directory configs, conditional stop, edited old address, no_proxy,
permissions, live stream drain and authenticated draining status, lock-held
identity failure JSON, unrelated owned helper survival and stable lock inode,
empty-queue quota guard/cooldown, inherited/existing macOS ACL rejection,
deleted config identity refusal, read-only private tokens and stale-log startup
classification. Additional focused RED logs retain the failed behavior before
those fixes. `red-draining.log` was an inadequate before-headers fixture; the
corrected live-stream RED is `red-draining-stream.log`.

Each loopback probe retains its own case outcomes and teardown. Rust fixtures
restore their original synthetic config, authenticated-off and remove their
owned directory; the unrelated sleep helper is explicitly killed/reaped by its
owned handle only. Final read-only process/temp inventory in `summary-final.json`
is empty. No indiscriminate process kill, real provider/model call, user client
configuration change, registration or performance matrix was used.

## Dependencies and native protection limits

Pinned fs4 1.1.0 (sync only) owns lock handles; Unix uses existing rustix 1.1.4
process API for pre-runtime setsid. macOS-only exacl 0.13.0 rejects nonempty
extended ACLs before token writes; it does not add Linux libacl linkage. Linux
POSIX ACL mode-mask behavior supports the private mode approach by source/manual
reasoning, but Linux runtime and non-POSIX filesystems were not exercised.
macOS no-follow ACL path lookup also compares the opened device/inode. This is
not an atomic defense against hostile same-user edits between permission checks.

Windows uses existing locked windows-sys 0.61.2 with only required features.
Creation supplies protected current-user DACLs, then validates owner, ordinary
user-only allow ACEs, disk/non-reparse type and descriptor bounds on the opened
handle before a writable file returns. Existing ACLs are rejected rather than
modified. Parent explicitly approved Cargo unsafe lint `deny` and a single
narrow Windows protection module exception with SAFETY comments/RAII.

`windows-check-attempt1.log` is a **failed full product cross-check**, blocked in
aws-lc-sys by missing Windows C/SDK headers (`stdlib.h`, `windows.h`). No SDK was
installed. `windows-api-final.log` is a separate successful isolated compile and
clippy of the actual Windows module under Rust 1.88/x86_64-pc-windows-msvc. It
proves API/type/lint compatibility, not full product compilation, linking,
Windows ACL behavior, lifecycle execution, suspend or login registration.
Actual Windows acceptance remains required; no Windows runtime PASS is claimed.

## Reproduction commands and provenance

Working directory is the product child. All Cargo builds used
`CARGO_TARGET_DIR=target/native`, except the isolated module used its own nested
target. Initial RED: `cargo test --test lifecycle_contract` (commands absent).
Final checks:

```sh
CARGO_TARGET_DIR=target/native cargo test
CARGO_TARGET_DIR=target/native cargo test --release
cargo fmt --all -- --check
CARGO_TARGET_DIR=target/native cargo check --all-targets --all-features
CARGO_TARGET_DIR=target/native cargo clippy --all-targets --all-features -- -D warnings
CARGO_TARGET_DIR=target/native/windows-api-check cargo clippy --manifest-path artifacts/native-task1/windows-api-check/Cargo.toml --target x86_64-pc-windows-msvc -- -D warnings
python3 scripts/benchmark.py --self-check
CARGO_TARGET_DIR=target/native cargo build --release --features bench-harness --example bench_gateway
python3 scripts/benchmark.py --smoke --binary target/native/release/llmgw --reference-binary target/native/release/examples/bench_gateway --seeds 1 --output artifacts/native-task1/benchmark-smoke-final.json
python3 scripts/probe_pi.py --binary target/native/release/llmgw --output artifacts/native-task1/pi-diagnostic-fixed.json
python3 scripts/probe_pi.py --binary target/native/release/llmgw --gateway-off --output artifacts/native-task1/pi-negative-diagnostic-fixed.json
python3 artifacts/native-task1/unix-session-final-probe.py
```

Each research CLI/stream/fairness/retry script used
`--binary target/native/release/llmgw --output artifacts/native-task1/<name>-diagnostic-fixed.json`.
Do not re-run evidence assembly over a held archive: `assemble-final-evidence.py`
records how it was captured, not permission to replace it during review.
`evidence-final-assembly.log` records a harmless evidence-script filename error;
`evidence-final-assembly-pass.log` records the corrected assembly. No product
change or test rerun resulted from that artifact-only typo.

Final release: **8,679,872 bytes**, SHA256
`09dff35ac5311ac499f4980d2572c013b72cd2f7bbcce4a9fb9a81921134a1ec`.
Final 61-file source archive: **170,144 bytes**, SHA256
`d8c05e4191aed5326e2ecdec7887bde199806796dfb7ba017b15f7adc8b9e70f`.
Final source manifest SHA256:
`8df2ec417d0465da061b28f0b51c2ce01de97cdf6afff51c2cc73c13081d8cdf`.
This is filesystem/build evidence, not Git/release provenance.

The original measured production and benchmark executables, pilot.json, all 40
pilot JSON hashes, and accepted Task 8 source archive retain their expected
hashes (`summary-final.json`). Old measured journals were not written by task
commands; no unrecorded prior journal hash comparison is claimed. Native work
makes no new efficiency claim; the earlier fixed-window pilot did not prove a
completion advantage.

Self-review checked ownership, stable lock inode, stale-record/nonce races,
pre-lock failures, state permission rejection before secret writes, restart
validation before disruption, old-address stop, control drain bounds, and owned
resource teardown. Remaining concerns are the explicit native OS/ACL acceptance
limits above. Fresh SPEC then QUALITY reviews have not yet run on this hold.
