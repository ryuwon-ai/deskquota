# Native Task 1 SPEC fix — final HOLD

**DONE_WITH_CONCERNS / FINAL HOLD for the same SPEC reviewer.** No QUALITY review
or next task was started. Product edits and builds are now stopped.

## Changes since prior HOLD

Only three product files changed: `src/lifecycle/mod.rs`,
`tests/lifecycle_contract.rs`, and `docs/runtime-contract.md`. No dependencies,
platform protection code, helper scripts, server/admission logic or CLI schema
changed. `source.diff` compares these files with the prior 61-file archive.

- `on` now resolves the canonical instance and acquires its operation lock, then
  authenticates the existing worker and compares current bytes before parsing a
  fresh launch. Malformed edited TOML therefore returns `restart_required` / 1
  while preserving the entire old identity. NEW/stopped configuration is still
  validated before token creation. `restart` validation and credential checks
  before stopping the old worker are unchanged.
- The common stale-PID fixture provisions its protected directory and runtime
  file through public `on`/`off`, then edits the existing valid JSON record's PID
  and state. It neither creates a Windows file with inherited DACLs nor changes
  permissions. The stale-state assertion now checks `stale_runtime=true`.
- Adjacent fixture review found the remaining direct private-state creation and
  chmod cases restricted to Unix/macOS rejection tests. Common record/token/log
  edits operate on files already provisioned by the public CLI. Windows runtime
  portability remains source-supported, not directly executed here.

The previous stale-log test's held operation-lock gate now occurs before the
parent parses stopped configuration. Its expected outcome is accordingly a
fresh parse failure and never stale `port_in_use`; no child is spawned in this
branch. Prior `red-stale-startup.log` / `green-stale-startup.log` remain unchanged
as evidence of the former child-before-lock failure and exit-status correction.
The production child exit-status protocol is unchanged by this SPEC fix.

## Direct checks on macOS for this revision

| Evidence | Result |
| --- | --- |
| `red-malformed.log` | behavioral RED: running malformed-config diagnostic fails; stopped-token guard passes (1 passed / 1 failed) |
| `green-malformed.log` | both focused cases PASS after production fix |
| `lifecycle-debug-final.log` | 22 lifecycle tests PASS / 0 failed |
| `lifecycle-release-final.log` | 22 lifecycle tests PASS / 0 failed |
| `fmt-final.log` | formatting check PASS |
| `check-final.log` | all targets/features PASS |
| `clippy-final.log` | all targets/features, warnings denied PASS |
| `cli-release.json` | current release CLI loopback fixture PASS, 1 upstream attempt |

The new running-config regression verifies the entire authenticated identity is
unchanged, pending restart is true, malformed restart is rejected, and a
nonblocking loopback upstream listener receives no connection. Off then succeeds.
The stopped-config regression verifies two failed on calls create neither token.
The 22-case suite includes the existing live-request 10-second drain, socket and
port cleanup, concurrency, identity rejection, unrelated helper preservation,
quota status and permission cases. Initial 22-case logs were followed by final
22-case runs after strengthening the stale-record setup to retain valid JSON;
there was no further product-source change.

Commands (product cwd; output redirected to the named logs):

```sh
CARGO_TARGET_DIR=target/native cargo test --test lifecycle_contract malformed_
CARGO_TARGET_DIR=target/native cargo test --test lifecycle_contract
CARGO_TARGET_DIR=target/native cargo test --release --test lifecycle_contract
cargo fmt --all -- --check
CARGO_TARGET_DIR=target/native cargo check --all-targets --all-features
CARGO_TARGET_DIR=target/native cargo clippy --all-targets --all-features -- -D warnings
python3 ../scripts/probe-product-cli.py --binary target/native/release/llmgw --output artifacts/native-task1/spec-fix/cli-release.json
```

## Source freeze and limitations

`source-manifest.json` freezes 57 product files, verifies the current CLI probe's
binary SHA, and checks stability during capture. `source-hold.tar.gz` contains
those files plus four unchanged authorized research helpers: **61 files,
170,535 bytes**, SHA256
`38c1d079f973a43e18bb49bee046a3216619eba96184ad71547053ce5d15a68a`.

Current release: **8,695,680 bytes**, SHA256
`af6b5b405cd29183482229cb42d217470a4004266c8f109e580ffaf5af83a033`.
Manifest SHA256:
`588a3d41e7b7831eb4502e7297c4953b50bbaf35bd7d014866290bdec011b1cd`.
`summary.json` records debug identity, prior artifact hashes, checks, source delta
and an empty final native-process/temp-state inventory. Fixtures clean only
owned handles/directories; no unrelated process is killed.

Prior `../README.md`, `../summary-final.json`, `../source-manifest-final.json`,
`../source-hold-final.tar.gz`, the 211-test logs, parent footprint and smoke remain
preserved prior-HOLD evidence. Their release SHA is
`09dff35ac5311ac499f4980d2572c013b72cd2f7bbcce4a9fb9a81921134a1ec`.
Full suites, Pi, external SSE/fairness/retry probes, smoke and footprint were
**not rerun or relabeled** for this revision. No current efficiency claim follows.
Original Task 8 binary/archive/pilot hashes and all 40 pilot JSON hashes still
match their preserved expectations.

No Windows/Linux runtime was performed. Prior Windows SDK-blocked full build
and isolated API/type/clippy results remain prior evidence; the platform source
is unchanged. The Windows fixture correction does not establish actual Windows
acceptance. No new dependencies, Git work, user configuration, provider/model
calls, registration or autostart actions occurred.

Self-review confirmed existing-identity-before-parse ordering, operation-lock
serialization, launch validation before token creation, unchanged restart
validation-before-disruption, structurally valid stale-record handling, no
permission relaxation, no new upstream connection in the counterexample, and
owned teardown. Same SPEC re-review is still required before acceptance.
