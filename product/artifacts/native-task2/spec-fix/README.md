# Native Task 2 SPEC Fix FINAL_HOLD

Status: FINAL_HOLD. `execution_finished=true`; product source and the isolated binaries are frozen for the same fresh SPEC reviewer.

## F1-F7 result

- F1: Existing macOS/Linux files are fully written and synced before publication. The actual SIGXFSZ repro now preserves all 417 original bytes. Windows keeps protected original and replacement recovery copies across ambiguous `ReplaceFileW` or post-write verification failure; the host-side 1176 model proves both contents remain recoverable.
- F2: Final apply holds a persistent setup writer lock and compares the exact draft snapshot again immediately before publication. The concurrent-edit repro exits 1 and keeps the external edit.
- F3: The dialog keeps the loaded Env header/name and Models/CountTokens endpoints. A Models-only root completes without a fabricated generation protocol.
- F4: Loaded login and client pending intents remain selected and persist through re-setup.
- F5: A model bounded by each request is valid without a configured fallback. The summary calls any configured fallback a reservation estimate, not an upstream limit.
- F6: Unchanged SaveAndStart uses authenticated `on`; PID, nonce, fingerprint, address, and start time remain identical.
- F7: Invalid configuration and missing inference arguments exit 2. The lifecycle suite preserves the distinct live-worker restart-required exit 1 and usable off/status identity path.

## Direct verification

- Final `cargo test --locked --all-targets`: 255 passed, 0 failed across 13 result rows (`full-tests-final.log`). This supersedes the pre-Windows-safety 252 count and the earlier intentionally retained failed `integration-green.log` run with three stale exit expectations.
- Setup contract: 36/36; lifecycle contract: 25/25.
- Windows recovery decision model: 3/3 after `windows-recovery-red.log` failed before the helper existed.
- `cargo fmt --all -- --check`, `cargo check --locked --all-targets`, `cargo clippy --locked --all-targets -- -D warnings`, and `cargo build --locked --release`: passed.
- Final macOS PTY: 7/7 against the exact held release path, including cancel, Back, Input Ctrl-C, Unicode/space save-only, authenticated save-and-start, injected post-start cleanup, and cleanup-failure preservation (`pty-final-exact.json`).
- Combined F1-F7 RED/GREEN observations are in `probe-results.json` / `green-probe-results.json`; running identity in `running-noop-results.json` / `green-running-noop-results.json`; metadata-only prompt behavior in `metadata-only-red-result.json` / `metadata-only-result.json`.

## Identity

- Source files: 70.
- Source manifest SHA-256: `1ad4bd171021f1249b3e5a6fbdfd4377b55c4f87c055e2cfe6fe3594d8d1b55f`.
- Source archive SHA-256: `7183f33d9c4eff60a31f61aa44b5b0cddb860788c385149c60ccbb2f97eb1955` (210671 bytes).
- Changed-files manifest SHA-256: `b4c7de930f4f7eb5e39e14e7534a6a7c015db902937040c19384b883352bb18a` (12 paths relative to initial Native Task 2 HOLD).
- Final isolated release: `821973c23b30b62270a08b19db3b73399275c06d44f2d0d81526c7426fb91530` (9298000 bytes).
- Final isolated debug: `b3dfda27393f301fb1246a58c9a336591ea6fb731f410a70161eb08bed8f321c` (28207288 bytes).
- PTY script: `0bcd9750d78eeba44702b1965564c58fba83d4a44b149c60291b74867e89daba`; exact PTY result: `377b36df227273142d1955f4693615733f2bbdfbe6faff8db5c7a8db14753bf9`.

Initial Native Task 2 source/archive/release remain `8ffc043b2e1da422e2cbe638443d9d8bd19e4116674379d68b2ecd31f2329254`, `1b705ee53f6a488fe3fb27e6402bcff2db5ef71bee8ed1ae8abad13911a045a5`, and `8b33e86bcc7a612316dfb37a686fa768d3371d8fdcd8da7a88919ed5bcf4d56b`. Protected ordinary/held binaries remain `e16ba39c54fcec9bc5ce4fb4039d551dccdd5cdfd3bec9e0a7c412e58bd4364f` at 8697040 bytes. Accounting baseline manifest/archive and pilot remain `f77ceac838cdf83fb5f7f0d7bd63536a3a9c62ecbecccf932c0a07c911a72304`, `e057f32e5eb37c44eaa489fe15ab0f9846fde283261b7f9c690a4eefb4d5c9c4`, and `bfe367592eae5953edf0f1bb64dc808c7864f78ce41b48137f47d9dc9901c41c`.

## Cleanup and limits

All owned `llmgw`, Cargo, and rustc processes are absent; matching owned temp paths are absent. Driver-specific cleanup records show all PTYs waited, authenticated worker stop, and scratch removal (`cleanup-final.json`).

Runtime PTY/network evidence is macOS only. Linux and Windows runtime remain unverified. The Windows 1176 behavior is a failure-model test around the safe helper, not native Windows execution. No real/paid API, model, user client file, login registration, OS integration, Git action, release, benchmark, or accounting rerun occurred.
