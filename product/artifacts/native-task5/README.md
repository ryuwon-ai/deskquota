# Native Task 5 implementation HOLD

Status: **DONE_WITH_CONCERNS**. Lightweight config-scoped user-login autostart is implemented and the product writer/runtime/test ownership is released for a fresh SPEC review followed by a fresh QUALITY review.

## Implemented boundary

- `llmgw autostart on/off` prints an exact OS target, label, absolute executable/config identity, argv, prior registration state, manager impact, and preview hash. Noninteractive mutation requires that hash; interactive setup confirms the same in-memory plan after the core config is saved.
- macOS uses a per-config LaunchAgent with literal `ProgramArguments`, `RunAtLoad=true`, and `KeepAlive=false`. Registration is file-only: no bootstrap or bootout.
- Linux uses a per-config user unit with literal absolute `ExecStart`, `Restart=no`, and `default.target`. Enable/disable never uses `--now`, never changes linger, and reports a missing user manager or policy failure.
- Windows uses a per-config current-user logon task with `InteractiveToken`, `LeastPrivilege`, `IgnoreNew`, `PT0S`, continuous-service power/idle/network settings, and no restart. Create checks an existing task before `/F`; removal verifies ownership and never calls Run or End.
- Status and doctor compose one-shot registration state outside lifecycle readiness. Authenticated runtime, registration, and next-login auth availability remain separate. Environment auth is `configured_but_unavailable` because shell availability is not login proof and no secret is copied.
- The OS manager owns foreground `llmgw --config ABS run`. There is no wrapper, watcher, privilege escalation, alternate supervisor, Startup/VBS fallback, or new runtime dependency.

## TDD and verification

The retained RED evidence includes missing autostart module, missing registration plan, CLI preview/status, doctor output, setup summary, moved/missing binary, Windows XML encoding, failed-status registration output, text status, login observation attestation, and verifier acceptance failures. The initial malformed Rust test diagnostic and one formatting diagnostic contained synthetic source literals and were moved to Trash; the corrected missing-module RED is retained.

Final frozen-source checks:

- `cargo fmt --all -- --check`: pass
- `cargo check --locked --all-targets`: pass
- `cargo clippy --locked --all-targets -- -D warnings`: pass
- `cargo test --locked`: **350 passed, 0 failed**
- `cargo build --locked --release`: pass
- release `autostart_templates`: **11 passed, 0 failed**
- Python verifier unit tests: **5 passed, 0 failed**
- release `verify_native.py --templates-only`: pass

The templates-only release fixture used owned temporary homes. On macOS, `/usr/bin/plutil` accepted the generated plist. It directly observed stopped → autostart on with zero worker, running → autostart off with the same authenticated nonce, authenticated manual cleanup, and empty-login-environment failure as `configured_but_unavailable`. No token or auth environment name appeared in registration. This evidence records `actual_user_registration_changed=false` and `actual_login_verified=false`.

## Immutable identities

- Release binary: `target/native-task5/release/llmgw` — `f3f64a801a629ef9ddcd2309e95b233a4fa70e6b78daf32ec3099c0454571b59` (10,084,768 bytes)
- Debug binary: `target/native-task5/debug/llmgw` — `0e5b74e670c1c318c400d57869cc4e9696b50d55a5986ec537a77ced8f5a230b` (30,576,216 bytes)
- Current default debug rlib: `target/native-task5/debug/deps/libllmgw-eba4665eb2ddaf45.rlib` — `263c4a82af3e259f2178bf08b374214c58717d6f9d9cd168bc082d6d1f596b03` (50,770,072 bytes)
- Source manifest: `artifacts/native-task5/source-manifest-final.json` — `560063431970bc5d3c48a77e31b8b140e6563e3508a53c20bca4eb808f224b42` (92 source files)
- Source archive: `artifacts/native-task5/source-hold-final.tar.gz` — `e107342c1692a05703e89fb978f0fcdd42822700d034520182b6a895dabb86c1` (297,809 bytes)
- Cargo.lock: `c82e9357a135eb84f0d1be70d0a5f16d2a6c0f7a520cc140562306418a0d3e1`
- Toolchain: rustc 1.88.0 `6b00bc388`, cargo 1.88.0 `873a06493`, aarch64-apple-darwin

The source archive contains 92 fingerprinted inputs plus its embedded manifest. The release templates-only probe matches the release binary hash. Relative to the accepted Task 4 manifest, 4 source files were added, 7 changed, none removed, and Cargo.lock did not change.

## Preservation and cleanup

- Native Task 4 final SPEC after-set excluding 88 live source entries: **1,157 checked, 0 drift**.
- Native Task 4 final SPEC own files: **17 checked, 0 drift**.
- Native Task 4 quality-fix/review preservation set: **82 checked, 0 drift**.
- Generated non-archive artifact privacy scan: pass, zero synthetic credential or auth-fixture sentinel hits.
- Owned worker processes and active temporary paths remaining: none.
- Failed owned fixtures and two diagnostic logs were moved to Trash and remain recoverable; Trash was not emptied and no reclaimed-space claim is made.

## Unverified limits

`--exercise-user-service` is a staged manual driver. It prints exact resources before requiring both `--execute` and temporary-account confirmation. Login stages require a timezone-bearing human login timestamp and explicit human attestation; a timestamp alone is not accepted as login proof. It was not run in this task because no separate OS account and human login cycle were available.

Actual next-login behavior, current-account registration, Windows Task Scheduler execution/console behavior, and Linux user-manager execution remain unverified. Core performance superiority remains unproven. The prior one-off macOS journal ACL-path `EINVAL` remains unresolved at its exact syscall/phase/posterror state; Task 5 did not retry, mask, or claim to resolve it.

`execution_finished=true`. No Git command, reference-clone edit, package install, actual OS registration, current client/config read, paid API, model start, or release publication was performed.
