# Native Task 2 F8 Fix FINAL_HOLD

Status: FINAL_HOLD. `execution_finished=true`; product source and isolated binaries are frozen for the same fresh SPEC reviewer.

## F8 result

Setup now builds a typed `RuntimeImpact` from the desired rendered config fingerprint and authenticated worker identity. A saved pending or external edit is disclosed before the final choice with both fingerprints, queue cancellation, active drain up to ten seconds, and desired-fingerprint readiness. Selecting Save and start carries explicit restart intent into apply. Apply rechecks the decision and expected fingerprint under the setup and lifecycle operation locks; unverified state or a mismatch whose restart impact was not previewed is refused before writing or restarting.

If desired config already matches the worker, setup uses authenticated idempotent `on` even when the prior disk state had been pending. Save only remains non-disruptive. Public Native1 on/restart behavior is unchanged; setup uses narrow expected-fingerprint variants.

## Direct verification

- Focused Rust: 95 passed, 0 failed — lib 11, config 19, lifecycle 25, setup 40 (`focused-tests.log`).
- Full `cargo test --locked --all-targets`: 261 passed, 0 failed across 13 result rows (`full-tests-final.log`).
- fmt, check, clippy with warnings denied, and release build passed in `target/native-task2-f8-fix`.
- Exact final release PTYs: four sessions. SaveOnly→SaveAndStart applies the saved fingerprint and clears pending restart; an external edit predating setup does the same; unchanged F6 preserves PID, nonce, fingerprint, address, and start time. Every worker was authenticated-stopped and every PTY waited.
- RED: held release SaveAndStart exited 1 with `restart_required` and retained the old fingerprint (`saved-pending-red-results.json`). The new typed API first failed compilation in `setup-contract-red.log`.

The intermediate `setup-contract-green-first.log` is intentionally retained: direct restart inside an integration-test executable starts `current_exe()` as the test binary and produced `worker_start_failed`. Final restart/readiness evidence therefore uses the actual `llmgw` PTY without adding a test-only product override. `runtime-decision-green-first.log` is an invocation-argument error; its corrected run is `runtime-decision-green.log`.

## Identity

- Source files: 70.
- Source manifest: `2107b93a752ebcbefbd9f2214c1688f6c527883ba6e061cbc1fbaa80781b11cb`.
- Source archive: `682f1dde419f2fa35ea4adadf1816b889e9c2b927ce6820282733b10cbec0b89` (212826 bytes).
- Changed-files manifest: `99782210f445bd1acd84b9293fdbfd39a54158829eeb42709cb24d874355b5ae` (8 paths relative to the prior SPEC-fix HOLD).
- Final release: `c890b6a498095bdba572d19b921b1865e48927af6807b767156f2e2f3a9ef1ba` (9316432 bytes).
- Final debug: `4e92596c2da4a64b7c08ab1286c7fb7fb48b0908d4e2ecc3ceacea200266ae3e` (28229160 bytes).
- Prior SPEC-fix source/archive/final-hold remain `1ad4bd171021f1249b3e5a6fbdfd4377b55c4f87c055e2cfe6fe3594d8d1b55f`, `7183f33d9c4eff60a31f61aa44b5b0cddb860788c385149c60ccbb2f97eb1955`, and `ec0cf615ff1f92add7cac715b3fadfa024e3d08f9c615e49bf3114cf1ecc604f`. Its release remains `821973c23b30b62270a08b19db3b73399275c06d44f2d0d81526c7426fb91530`.
- Re-review README remains `dd58b9989eb4b0488f2dffd1270b448f4efe626698fd02e82caf8e8cdd16bab6`.
- Protected ordinary/held binaries remain `e16ba39c54fcec9bc5ce4fb4039d551dccdd5cdfd3bec9e0a7c412e58bd4364f` at 8697040 bytes.

## Cleanup and limits

No owned llmgw/Cargo/rustc process or matching temporary path remains (`cleanup-final.json`). Runtime proof is macOS only. Linux/Windows runtime, real providers, model startup/download, user client files, login registration, OS integration, Git actions, benchmark, and accounting runs remain unverified or untouched.
