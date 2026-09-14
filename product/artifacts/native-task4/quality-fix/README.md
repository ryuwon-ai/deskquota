# NativeTask4 QUALITY fix HOLD

Status: **DONE_WITH_CONCERNS**. Product/runtime/test execution is finished and ownership is released for the same QUALITY reviewer. A final same-SPEC confirmation remains a separate review step.

## Closed findings

### Q1: readiness across interactive confirmation

Interactive setup now binds the displayed client plan to the reviewed runtime action, validates the exact client snapshot, then obtains a fresh authenticated lifecycle status after confirmation. If the runtime action changed during the human wait, setup leaves the client resource pending and requires review again. A client patch is applied only after the desired gateway fingerprint is authenticated as ready.

The retained stopped-worker scenario now exits 1 with no client file and no applied journal. The ready control exits 0, observes the exact desired fingerprint, and applies the client resource. Both use the final release binary `cc15eb4172148a681faba2d4e2932e94a4853fc303de61d234b7a016c05b6a90`.

### Q2: retired dedicated-provider ownership

Pi and Codex reconnect plans carry an active dedicated-provider ownership requirement. It is checked before any gateway activation and checked again while ConfigPatch holds the existing journal transaction lock and stable resource lock, before a restored journal can be reset or a new applying state can be published. A disconnect that retires the journal invalidates a retained plan even if a third party recreates byte-identical provider data.

New regressions cover Pi and Codex retired journals plus 16 cooperating apply/disconnect races. Existing new-provider creation, same-journal reconnect, conflict preservation, and partial-failure retry controls remain green.

## TDD evidence

- Q1 RED: the held release wrote the client profile after the worker was stopped during confirmation; the correctness probe intentionally exited nonzero. See `q1-readiness-red.log`.
- Q2 RED: both retained Pi and Codex plans accepted retired ownership and changed recreated client bytes. See `q2-retired-plan-red.log`.
- GREEN: full locked suite passed **339/339** across 16 result rows; release `client_profiles` passed **24/24**; Python driver tests passed **7/7**.
- The implementer compiled and ran the retained reviewer `quality_probes.rs`. Its first default-parallel execution had one `AlreadyExists` fixture collision because two tests use the same PID-only temporary directory name. That pre-assertion failure is preserved. The serial implementer run passed **5/5**. This is implementer evidence and is not an independent reviewer rerun.

## Installed-client evidence

All three installed clients ran through isolated allowlisted environments against the frozen release binary. No real user configuration, credentials, login state, keychain, or external paid API was used.

- Pi 0.84.2: model listing and selection, inference, exact synthetic read tool result, follow-up request, gateway-off upstream-zero negative, and disconnect reload verified.
- Codex 0.154.0: named profile load/control, selected Responses model with WebSockets disabled, inference and exact tool roundtrip, gateway-off upstream-zero negative, and disconnect reload verified. Codex model listing remains unverified because the gateway model list is not a Codex catalog.
- Claude 2.1.63: generated profile without driver auth environment, selected Messages model, inference and exact read tool roundtrip, gateway-off upstream-zero negative, and disconnect reload verified.

## Verification

- `cargo fmt --check`: pass
- `cargo check --locked --all-targets`: pass
- `cargo clippy --locked --all-targets -- -D warnings`: pass
- `cargo test --locked`: 339 passed, 0 failed
- `cargo test --locked --release --test client_profiles`: 24 passed, 0 failed
- `cargo build --locked --release`: pass
- Actual Pi, Codex, and Claude isolated drivers: pass and bound to the release hash

No historical performance/accounting matrix or unchanged Windows cross-build was rerun. The HTTP/fairness/quota/accounting hot path and dependency lock were unchanged.

## Immutable identities

- Release binary: `product/target/native-task4-quality-fix/release/llmgw` — `cc15eb4172148a681faba2d4e2932e94a4853fc303de61d234b7a016c05b6a90` (9936768 bytes)
- Debug binary: `product/target/native-task4-quality-fix/debug/llmgw` — `547e0776af5552c2bb57f4050bba8b152ed26bb776e1082bdbbcf49dea8f6144` (30463832 bytes)
- Debug rlib used by the implementer retained-probe run: `product/target/native-task4-quality-fix/debug/deps/libllmgw-eba4665eb2ddaf45.rlib` — `8651b48f5c771b351a33e266bf6ea9116eb45267879e6ca8137c4f5ae33bd632` (49717568 bytes)
- Source manifest: `product/artifacts/native-task4/quality-fix/source-manifest-final.json` — `69fe932ae90f46abf65305be8fd0197d4c4a0e6bdc68cceb6ff63b38406ed20b` (16074 bytes)
- Source archive: `product/artifacts/native-task4/quality-fix/source-hold-final.tar.gz` — `85d184adc9ad10e859a6988436a843bbd19ce41a250c743413d2466be5a12d3c` (281703 bytes)
- Cargo.lock: `c82e9357a135eb84f0d1be70d0a5f16d2a6c0f7a520cc140562306418a0d3e1c` (52711 bytes)
- rust-toolchain.toml: `a016e82d3b7986387adf4d7d9d88e1581f87bb2a9cfb91ecec4914411b07634b`
- Toolchain: `cargo 1.88.0 (873a06493 2025-05-10)`; rustc 1.88.0, host aarch64-apple-darwin

The archive contains the 88 fingerprinted source inputs plus its embedded manifest. The current source still matches the manifest.

## Source delta from accepted SPEC fix

- `product/docs/client-compatibility.md`
- `product/src/cli.rs`
- `product/src/clients/claude.rs`
- `product/src/clients/codex.rs`
- `product/src/clients/mod.rs`
- `product/src/clients/pi.rs`
- `product/src/config_patch/apply.rs`
- `product/src/config_patch/journal.rs`
- `product/src/config_patch/mod.rs`
- `product/tests/client_profiles.rs`

No source was added or removed.

## Preservation and cleanup

- QUALITY after baseline: 1,151 entries; 88 live source entries excluded; remaining **1,063 checked, 0 drift**.
- QUALITY HOLD-listed files and fixed README/HOLD/after identities: 0 drift.
- Earlier accepted Task2 426-item and Task3 357-item bases: 0 drift.
- SPEC-fix parent 65-item and earlier parent 192-item preservation sets: 0 drift.
- Artifact privacy scan: pass; no generated credential/sentinel hit in scanned output.
- Owned worker processes and temporary paths remaining: none.

## Retained concern and limits

One macOS journal ACL-path `EINVAL` was observed in the earlier SPEC-fix phase and did not reproduce. The exact syscall and phase remain unconfirmed, and that failed test's fixture cleanup removed the post-error state before it could be inspected. No retry or masking source change was added. Fresh deterministic permission-fault recovery controls pass, but they do not establish the cause of that earlier observation.

Actual installed-client execution in this phase is macOS-only. Windows and Linux runtime behavior remains unverified here. The existing nonblocking observation that `cli.rs` is concentrated remains; this narrow correction did not perform a broad refactor.

## Review boundary

This HOLD records implementer execution only. It does not claim an independent QUALITY rereview. `execution_finished=true`; product writer/runtime/test ownership is released.
