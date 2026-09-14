# Native Task 6 SPEC S1 documentation fix

Status: **DOC FIX COMPLETE — ready for the same SPEC reviewer**. `product/docs/installation.md` is the only source change from `native-task6-final-v2`.

The Windows section now documents `replace_failed_recovery_unconfirmed`: stop and retain the reported target, destination-local `.llmgw-backup-*`, and candidate; the previous executable may exist only in the backup while `llmgw.exe` is absent. It requires recording the original and recovery errors and all reported paths. Restoration is allowed only after confirming that the target remains absent, no competing writer exists, and the backup matches the known previous checksum or release identity. The verified previous executable may then be copied back, or moved only while another recovery copy remains. Backup and candidate files stay preserved until checksum, version, and execution verification succeeds. Blind overwrite, automatic reinstall, and cleanup are prohibited when recovery is unconfirmed.

## Fresh documentation/package verification

- Source comparison: 101 paths, exactly one changed path (`product/docs/installation.md`); no additions or removals.
- Every Cargo, Rust runtime, test, script, packaging, and other source input is byte-identical to final-v2.
- Source archive: 102 unique regular members including the exact embedded manifest; every archived byte matches the new manifest and live source.
- User package: exactly five unique regular members. The four documents match the newly frozen source, and `llmgw` matches the final-v2 held binary exactly.
- Source manifest SHA-256: `f228071555f45c1774b7c2f8e059f2adce6eef77c10674191e04713feb0d4c3f`
- Source archive SHA-256: `b8b2843f9dfdbeb18ce4368af9609f9a087bc95d171cc9324d4f968ab26d383f`
- Binary SHA-256: `cf7c436c442c426a6e9a1d485a5261fe5e5a05c8b131b130e679e00eec2c646b`
- User package SHA-256: `ba03203e9416a4eb75ca972fb0b3e5eba8618c46b2c26c7f65e2b7ffff387a22`
- Package checksum-manifest SHA-256: `c9ca619d26d310d7dbe06431c7a3d959541bc3c6c134899322d7e3c8b4d3ac91`

## Retained evidence and limits

No Cargo command, Rust/Python test suite, client matrix, native measurement, or Windows runtime was rerun for this documentation-only change. The final-v2 Rust 365/Python 64 results and Pi 0.84.2, Claude 2.1.63, Codex 0.154.0 observations remain retained evidence for byte-identical runtime/test/script/Cargo inputs and binary SHA `cf7c436c442c426a6e9a1d485a5261fe5e5a05c8b131b130e679e00eec2c646b`. They are not presented as new executions.

PowerShell and Windows remain source/static-only and unverified. No real user files, credentials, PATH/profile/registry state, OS registration, package install, cloud action, public release, or version-control operation was used.

`execution_finished=true`; `runtime_ownership_released=true`.
