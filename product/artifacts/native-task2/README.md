# Native Task 2 FINAL_HOLD

Status: FINAL_HOLD. Product source is frozen for fresh SPEC then QUALITY review.

## Direct verification

- `cargo test --all-targets`: 246 passed, 0 failed (`all-targets.log`).
- `cargo test --test setup_contract`: 30 passed, 0 failed (`setup-contract-green.log`).
- `cargo fmt --all -- --check`, `cargo check --all-targets`, and `cargo clippy --all-targets -- -D warnings`: passed.
- Owned macOS PTY covers cancel, Back then cancel, Input Ctrl-C, save-only, save-and-start/readiness, injected post-start cleanup, and cleanup-failure state preservation (`pty-smoke.json`).
- Isolated `target/native-task2` release build produced Mach-O arm64 `llmgw 0.1.0`.

## TDD evidence

- Initial setup API compile RED: `setup-contract-red.log`.
- HTTP 200 malformed/provider-failed inference RED: `inference-validation-red.log`.
- Valid Responses `error: null` false-failure RED: `inference-null-red.log`.
- Valid inline auth/quota table scalar-edit panic RED: `inline-table-red.log`.

## Identity

- Source files: 68.
- Source manifest SHA-256: `8ffc043b2e1da422e2cbe638443d9d8bd19e4116674379d68b2ecd31f2329254`.
- Source archive SHA-256: `1b705ee53f6a488fe3fb27e6402bcff2db5ef71bee8ed1ae8abad13911a045a5`.
- Current isolated release binary SHA-256: `8b33e86bcc7a612316dfb37a686fa768d3371d8fdcd8da7a88919ed5bcf4d56b` (9,287,392 bytes).
- FINAL_HOLD JSON SHA-256: `e278c0ab62eab9817ed6fe9c6ae203d32ee4b92d2e64218b2603221506f72a0f`.
- Changed-files manifest: 25 paths (16 modified, 9 added).

The ordinary and held benchmark binaries remain SHA-256 `e16ba39c54fcec9bc5ce4fb4039d551dccdd5cdfd3bec9e0a7c412e58bd4364f` at 8,697,040 bytes. The accounting pilot remains SHA-256 `bfe367592eae5953edf0f1bb64dc808c7864f78ce41b48137f47d9dc9901c41c`. The prior source manifest and archive remain `f77ceac838cdf83fb5f7f0d7bd63536a3a9c62ecbecccf932c0a07c911a72304` and `e057f32e5eb37c44eaa489fe15ab0f9846fde283261b7f9c690a4eefb4d5c9c4`.

## Execution incident

While writing this evidence file, an unquoted heredoc executed Markdown backticks and started an owned `cargo test --all-targets` in the default `target/debug`. Its owned process group was terminated before completion. It changed no product source, did not invoke `target/release`, and did not change either protected binary. All accepted verification and the current build use `target/native-task2`.

## Limits

Runtime PTY and network fixtures were exercised on macOS only. Linux and Windows setup runtime are unverified. No real or paid API, model download/start, current-user client config, login registration, installer, commit, release, or benchmark matrix was exercised.
