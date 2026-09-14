# Native Task 5 evidence erratum

Recorded: `2026-09-13T03:37:02.572417+00:00`

This is a metadata and preservation correction. Product source, binaries, source manifest, source archive, Cargo.lock, prior logs, and prior v1 README/HOLD files were not edited or rebuilt.

## Corrected statements

1. The complete Cargo.lock SHA-256 is `c82e9357a135eb84f0d1be70d0a5f16d2a6c0f7a520cc140562306418a0d3e1c`. The v1 README omitted its final character. The value here was extracted from the current file and matched against `identities-and-stability-final.json`.
2. The v1 README claim that two moved diagnostic logs remained recoverable is retracted. The original paths and the exact known Trash paths are absent, so their bytes and hashes are unavailable and no restoration or reconstruction was performed. Their earlier classification was based on observation before the move: compiler/rustfmt diagnostics included synthetic test source literals. Those literals were not actual user credentials. No generated credential disclosure was observed, but the unavailable bytes prevent a fresh content proof.
3. `source-fingerprint-final.log` records a path-resolution failure before the fingerprint script ran. The final manifest was absent after that attempt. `source-fingerprint-final-attempt2.log` records the successful command that created the 92-file manifest. Neither attempt overwrote an older manifest or source artifact; no lost version is claimed or reconstructed.

## Current immutable identity check

- Source manifest: `560063431970bc5d3c48a77e31b8b140e6563e3508a53c20bca4eb808f224b42`
- Source archive: `e107342c1692a05703e89fb978f0fcdd42822700d034520182b6a895dabb86c1`
- Release binary: `f3f64a801a629ef9ddcd2309e95b233a4fa70e6b78daf32ec3099c0454571b59`
- Cargo.lock: `c82e9357a135eb84f0d1be70d0a5f16d2a6c0f7a520cc140562306418a0d3e1c`
- Live source files checked against the manifest: `92`, drift: `92`
- Correction record: `26465de97721ea06c818695db66ee41b343d5fff83f96a4b0498eecebf5b5b0b`

Actual user registration was not changed and actual login remains unverified.
