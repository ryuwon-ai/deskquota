# Native Task 2 QUALITY Q1/Q2 Fix FINAL_HOLD

Status: FINAL_HOLD. `execution_finished=true`; product source and isolated binaries are frozen for the same fresh QUALITY reviewer.

## Fixed findings

- **Q1:** Existing valid TOML now stays in the `toml_edit` path for endpoint and model changes. The editor compares the original and desired typed config, mutates only changed fields, supports array-of-tables and inline table collections, and validates the rendered document equals the desired config. Actual PTYs retained the unrelated heading/port comments and untouched second-route internal array comments while adding an endpoint, and retained model/route comments while changing the model ID and reservation cap.
- **Q2:** Restart now constructs and drops the same configured reqwest proxy/TLS client under the existing operation lock before stopping an authenticated worker. Missing, malformed, and empty CA errors use the existing sanitized transport errors. Actual missing and malformed CA PTYs returned exit 1, kept the old PID/nonce/fingerprint running, saved pending config, and made zero upstream requests. A valid explicit CA path reached authenticated startup.

F8 SaveOnly followed by SaveAndStart applied the saved desired fingerprint; F6 unchanged SaveAndStart preserved exact config bytes and the entire worker identity. Idempotent `on`, `status`, and `off` do not reread the pending CA file.

## Direct verification

- Focused after implementation: setup **42/42**, lifecycle **26/26**.
- Final `cargo test --locked --all-targets`: **264 passed, 0 failed** across 13 result rows.
- `cargo check --locked --all-targets`, `cargo clippy --locked --all-targets -- -D warnings`, direct Rustfmt check, and `cargo build --locked --release` passed in the isolated quality-fix target.
- Exact final release PTYs: **8 sessions** — endpoint edit, model edit, missing CA restart, malformed CA restart, valid CA start, F8 SaveOnly and SaveAndStart, F6 no-op. All PTYs were waited; all seven owned configs reached authenticated stopped state before scratch deletion.
- RED is retained from the held F8 release and two focused contract failures. The initial final clippy attempt found only a new test `format!` style lint; its failed log is retained, the test was formatted, and final clippy passed.

`cargo fmt --locked` is unsupported by cargo-fmt and its exact failure is retained. Because every Cargo invocation was required to include `--locked`, formatting was applied and checked directly with the pinned Rustfmt binary instead of issuing an unlocked Cargo command.

## Identity

- Source files: 71; changed paths relative to F8: 6.
- Source manifest: `3495daf57907d5e4472513aaef5131cd6bf418fa58d369041b2d0484454ca603`.
- Source archive: `b221562a379edbd6cfe1e6faad77a1176ac0622774936a479d7734a190a6cd99` (215107 bytes).
- Changed-files manifest: `3678da5d4b080ff634712f2b3cc1ea06835734675b4246d3cd2b5979a187420a`.
- Final release: `aa4015d5b34426b5d8e502d8cfbb2f4af2757e16a7cd2094b43e219c974cb3a6` (9335616 bytes).
- Final debug: `606b268ce4c0822263ffe2960f0075b61fec125eb3b02ac944bc8fb61b6aab4d` (28267032 bytes).
- F8 release/source manifest/archive/final-hold remain `c890b6a498095bdba572d19b921b1865e48927af6807b767156f2e2f3a9ef1ba`, `2107b93a752ebcbefbd9f2214c1688f6c527883ba6e061cbc1fbaa80781b11cb`, `682f1dde419f2fa35ea4adadf1816b889e9c2b927ce6820282733b10cbec0b89`, `f19162307fc7ae90351f1024b4dc950e13b802c84baaf331c7e4a327c3991a85`.
- Fresh review README remains `55265cf113515d96cf280d6cbeb96840f2fcc7e8edd60eceee30b7624177e549`. All 317 reviewer-listed protected files had zero drift.
- Protected ordinary/held binaries remain `e16ba39c54fcec9bc5ce4fb4039d551dccdd5cdfd3bec9e0a7c412e58bd4364f` at 8697040 bytes.

## Limits

Runtime proof is owned macOS PTY and loopback only. Linux/Windows runtime, real providers, model startup/download, user client files, login registration, OS integration, Git actions, benchmark, and accounting runs remain unverified or untouched. Q2 performs no upstream availability request.
