# Native Task5 QUALITY fix evidence

Status: **DONE_WITH_CONCERNS**. The Q1–Q3 corrections and the adjacent predictable-backup collision correction are implemented. Execution is finished and ownership is released for the same Native Task5 QUALITY rereview. Task6 has not started.

## Changes

- `src/autostart/mod.rs`: registration candidates are created through the existing protected exclusive-open primitive and written through that handle. A pre-existing regular file or dangling symlink at the predictable temporary path is refused and preserved. The predictable backup path is checked with `symlink_metadata`; any existing entry is refused and preserved, while only `NotFound` authorizes the rename.
- `scripts/verify_native.py`: the macOS template fixture now owns its temporary directory explicitly. Once manual start is attempted, every exceptional path attempts authenticated `off` with the exact config and confirms `stopped` before deleting state. Failed confirmation retains the recovery directory and reports both the original and cleanup failures. Executed human-attested login observations return exit 1 when their existing acceptance predicate is false; preview and intentionally incomplete stages retain their prior exit semantics.
- `tests/autostart_templates.rs`: adds dangling temporary and backup symlink regressions, with regular temporary collision and changed-preview preservation controls.
- `tests/test_verify_native.py`: adds post-start cleanup, failed-cleanup retention, and failed/successful attested-observation exit controls.

`Cargo.lock` remains unchanged at `b216737fbfe2dc9946ffba69e34e26ce9902c6c87c90e974ab58ee850337e3e8`.

## TDD and final verification

- Q1 dangling temporary RED: 0 passed, 1 failed because `apply` returned `Ok`; GREEN: 1 passed.
- Q2/Q3 corrected Python RED: four expected failures across cleanup, retention, and two failed observation cases; GREEN: three methods passed, including four observation subcases. The first Python RED attempt had a test-fixture recursion error and is preserved separately rather than counted as the behavioral RED.
- Adjacent dangling backup RED: 0 passed, 1 failed because `apply` returned `Ok`; GREEN: 1 passed.
- `cargo fmt -- --check`: exit 0.
- `cargo check --locked`: exit 0.
- `cargo clippy --locked --all-targets -- -D warnings`: exit 0.
- `cargo test --locked --test autostart_templates`: 15 passed, 0 failed.
- `PYTHONDONTWRITEBYTECODE=1 python3 -m unittest tests.test_verify_native`: 13 passed, 0 failed.
- `cargo build --locked --release`: exit 0.
- Final release `verify_native.py --templates-only`: exit 0; macOS `plutil` passed; the manual worker was authenticated and stopped before fixture removal; `actual_user_registration_changed=false`; `actual_login_verified=false`.

The earlier full 362-test run is held prior evidence for a different release binary and is not relabeled as current. No full project, installed-client, quota, accounting, performance, Windows SDK, or actual user-service matrix was rerun in this bounded quality correction.

## Current identities

Source-manifest rows are relative to `/Users/ryuwon/Desktop/ryuwon-project/llm-gateway-research`. Artifact, target, and lock rows below are relative to `/Users/ryuwon/Desktop/ryuwon-project/llm-gateway-research/product`.

- Release binary: `target/native-task5-quality-fix-v2/release/llmgw` — 10,011,264 bytes — `88c21502025b0bb0b77db2833845d64bb2c562e345e0c48bc5042651a4ee25a3`
- Debug binary: `target/native-task5-quality-fix-v2/debug/llmgw` — 30,643,560 bytes — `d01801d02719848a9d5e161a3d8bbc5b78c052a0b351bf6bcfc0ac48ab7d49cb`
- Default debug rlib: `target/native-task5-quality-fix-v2/debug/deps/libllmgw-1750bcafbc9dfb14.rlib` — 50,712,616 bytes — `0c0bfaf27091f5bc10896015f6d99eaacce21efb11f98ed7c5a51950f409fea7`
- Release rlib: `target/native-task5-quality-fix-v2/release/deps/libllmgw-fc099ad98d7d2eb1.rlib` — 7,864,760 bytes — `c6f1aef7f8592bb03527944ffc5d4ed25ff9e94a4b5b1869af9142c68ea6655c`
- Source manifest: `artifacts/native-task5/quality-fix-v2/source-manifest-final.json` — 16,254 bytes — `8b281a41f48313a2d664d9f94737cbcba3794b10688904ec1e969d675296e304`
- Source archive: `artifacts/native-task5/quality-fix-v2/source-final.tar.gz` — 310,515 bytes — `12f6c81a44d95879383ffb0883b2b7681dcc203c18c20dcca6cf2621cfe5c875` (93 members)

The manifest contains 92 source rows with zero drift. Its release hash, the fixture hash, and the current release bytes all equal `88c21502025b0bb0b77db2833845d64bb2c562e345e0c48bc5042651a4ee25a3`. The archive matches the manifest and live frozen source byte for byte. These are filesystem and command records, not Git provenance.

## Provisional capture and preservation

The first Q1–Q3 capture was superseded after the adjacent dangling-backup finding. Its target was not rebuilt. Its captured release remains `a61417ee53c5d8ae91a67d90a4768c8d183c9efbc9eada7010da1730471cce8f`; its source archive remains `be4452f6e8175d8d44cb912d8501e00609af94af92e867294b73464aa40bc586`. All seven recorded provisional identities rechecked without drift.

The v2 preservation baseline contains 1,521 retained non-live files with zero drift. This is a bounded baseline claim, not a blanket claim that every historical artifact is available or unchanged.

The first cleanup metadata check incorrectly treated eight unrelated pre-existing Python cache files as current residue and is preserved with `passed=false`. The corrected check scopes bytecode to the two current verifier modules and passes. The final fixture home and LaunchAgent path are absent, its recorded manual cleanup state is `stopped`, and no current verifier bytecode remains. The unrelated cache files were not removed.

## Remaining limits

- Actual Windows and Linux managers were not executed.
- No actual login or temporary OS account flow was executed; `actual_login_verified=false` remains explicit.
- Two historical diagnostic logs and two overwritten provisional debug originals remain unavailable with recovery unknown. No reconstruction claim is made.
- The earlier macOS ACL-path EINVAL remains unresolved at its exact syscall, phase, and post-error state; this phase did not retry it or claim it resolved.
- Fixed synthetic source literals remain legitimately present in source and archives. No generated credential value was recorded or copied into registration.
