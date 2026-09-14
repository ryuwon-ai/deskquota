# Native Task 1 QUALITY fix — final HOLD

**DONE_WITH_CONCERNS / FINAL HOLD for the same QUALITY reviewer.** Product edits
and builds are stopped. No next task, dependency, platform refactor or registration.

Only `src/lifecycle/mod.rs`, `tests/lifecycle_contract.rs`, and
`docs/runtime-contract.md` changed from the accepted SPEC-fix hold. Exact delta:
`source.diff`. Restart now resolves its canonical path, obtains that instance's
operation lock, then loads and validates current configuration and credentials
before stop/start. A load resolving to a different canonical path is rejected
before either instance is stopped. This extra guard was parent-approved within
the SAME-instance requirement after a directly reproduced symlink counterexample.

On's authenticated existing-state-before-parse order and stopped token validation
are unchanged. The existing child fingerprint guard is unchanged. There is no
new resolver framework, snapshot IPC, supervisor, migration, or assertion that
external editor changes are atomic throughout drain/spawn.

## Direct evidence on this macOS host

| Evidence | Result |
| --- | --- |
| `red-restart-race.log` | both new held-operation-lock cases failed under old ordering; 2 existing cases passed |
| `green-restart-race.log` | all 4 focused cases passed after ordering fix |
| `red-restart-path.log` | canonical path retarget returned success instead of rejecting a different instance |
| `green-restart-path.log` | path retarget rejected; both complete worker identities preserved |
| `lifecycle-debug-final.log` | 25 passed, 0 failed |
| `lifecycle-release-final.log` | 25 passed, 0 failed |
| `fmt-final.log` | formatting check PASS |
| `check-final.log` | all targets/features PASS |
| `clippy-final.log` | all targets/features, warnings denied PASS |
| `cli-release.json` | current release CLI loopback fixture PASS; 1 upstream attempt |

Each new restart test explicitly holds the existing protected operation lock,
spawns the restart command, gives it 500 ms to reach the gate, checks that the
child remains pending, writes the edit, releases the lock and reaps the command.
The lock controls the disruptive stop/start ordering; the scheduling interval
is not a latency claim or a new production timing seam. Malformed bytes must
return validation failure with the full old identity unchanged. A valid comment
save must restart successfully with a new nonce, the latest bytes' fingerprint,
and no pending restart. Both cases use a nonblocking owned loopback upstream
listener and verify zero upstream connections. The additional Unix-only test
retargets to a second owned worker config while waiting, restores the original
path before inspection/cleanup, and verifies both identities remain unchanged.

The final 25-case runs include prior on/SPEC, credential/ACL, queue hold/cooldown,
concurrent lifecycle, drain/socket cleanup and unrelated helper tests. Earlier
24-case logs are intermediate evidence before the path guard/test; use the
`*-final.log` files for this held source. No unrelated full suites, Pi, external
SSE/fairness/retry probes or benchmark matrix were repeated for SHA refresh.

Commands (product cwd, redirected to these logs):

```sh
CARGO_TARGET_DIR=target/native cargo test --test lifecycle_contract restart_
CARGO_TARGET_DIR=target/native cargo test --test lifecycle_contract restart_path_retarget
CARGO_TARGET_DIR=target/native cargo test --test lifecycle_contract
CARGO_TARGET_DIR=target/native cargo test --release --test lifecycle_contract
cargo fmt --all -- --check
CARGO_TARGET_DIR=target/native cargo check --all-targets --all-features
CARGO_TARGET_DIR=target/native cargo clippy --all-targets --all-features -- -D warnings
python3 ../scripts/probe-product-cli.py --binary target/native/release/llmgw --output artifacts/native-task1/quality-fix/cli-release.json
```

## Held source, preservation and limits

`source-manifest.json` freezes 57 product files and matches the current release
CLI evidence. `source-hold.tar.gz` contains those plus four unchanged research
helpers: **61 files, 171,335 bytes**, SHA256
`d84f5b1ee94fc2a57a697b92007e2b2715168735a97ef33c2209f4e01f6d5c33`.
Manifest SHA256:
`4f89a05514a8a662409c89d2945394c19a0c318fd24076e0741de521cd7780a2`.

Current release: **8,697,040 bytes**, SHA256
`e16ba39c54fcec9bc5ce4fb4039d551dccdd5cdfd3bec9e0a7c412e58bd4364f`.
`summary.json` contains debug identity, exact source delta, prior/reviewer
artifact hashes, checked original binary/pilot hashes and empty final owned
native-process/temp-state inventory. Only owned fixtures/handles were cleaned.

Prior SPEC-fix source/archive/reports and 22-test logs are preserved under
`../spec-fix/`; their release SHA is
`af6b5b405cd29183482229cb42d217470a4004266c8f109e580ffaf5af83a033`.
Earlier full 211-test logs, Pi/stream/fairness/retry, smoke, and parent footprint
remain their exact earlier-source evidence. They were neither overwritten nor
relabeled as current execution. Reviewer `probe-restart-race.py` and its failing
`restart-race-results.json` remain unchanged. Original Task 8 measured binaries,
source archive, pilot manifest and all 40 pilot JSON hashes match expectations.
No benchmark or efficiency claim follows from these changes.

Windows/Linux runtime and whole Windows SDK build remain unverified. No new
cross-platform check is claimed; the unchanged platform module retains prior
isolated type/clippy evidence. The new symlink test runs only on Unix. No Git,
real provider/model requests, user client settings, autostart or OS registration
was performed. No complete defense against hostile same-user filesystem edits
is claimed.

Self-review checked canonical instance ownership before/after lock wait,
configuration and credential validation before stop, unchanged child guard and
on contract, no new upstream requests in race fixtures, and owned resource
cleanup. Same QUALITY re-review is required before Task 1 acceptance.
