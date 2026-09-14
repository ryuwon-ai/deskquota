# Native Task 5 evidence erratum — final v2

Recorded: `2026-09-13T03:38:58.538079+00:00`

This corrects metadata and preservation statements only. Product source, binaries, the source manifest, source archive, Cargo.lock, earlier logs, and v1 evidence were not edited or rebuilt.

## Corrections

1. The complete Cargo.lock SHA-256 is `c82e9357a135eb84f0d1be70d0a5f16d2a6c0f7a520cc140562306418a0d3e1c`. The v1 README omitted its final character. This value was extracted from Cargo.lock and matched to `identities-and-stability-final.json`.
2. The v1 claim that two moved diagnostic logs remained recoverable is retracted. Each original path and exact known Trash path is absent. Their bytes and hashes are unavailable, so no restore or reconstruction occurred. Prior observation classified them as compiler/rustfmt diagnostics containing synthetic test source literals. Synthetic literals are not actual user credentials. No generated credential disclosure was observed; unavailable bytes prevent a fresh content proof.
3. `source-fingerprint-final.log` is preserved evidence of a path-resolution failure before the fingerprint script ran. `source-fingerprint-final-attempt2.log` is preserved evidence of the successful run that created the 92-file manifest. Attempt 1 did not write a manifest; attempt 2 created it once. No older manifest, source, or archive bytes were overwritten, and no lost version is claimed or reconstructed.
4. The first generated v2 correction artifacts are preserved as a failed metadata attempt. They used the wrong root to resolve manifest paths and falsely reported 92 missing files. They are not release evidence. This final v2 record resolves paths from `/Users/ryuwon/Desktop/ryuwon-project/llm-gateway-research` and verifies 92 files with 0 drift.

## Current identities

- Source manifest: `560063431970bc5d3c48a77e31b8b140e6563e3508a53c20bca4eb808f224b42`
- Source archive: `e107342c1692a05703e89fb978f0fcdd42822700d034520182b6a895dabb86c1`
- Release binary: `f3f64a801a629ef9ddcd2309e95b233a4fa70e6b78daf32ec3099c0454571b59`
- Cargo.lock: `c82e9357a135eb84f0d1be70d0a5f16d2a6c0f7a520cc140562306418a0d3e1c`
- Correction record: `e6cc528706246da0dc6f92ba5af21871b42c97e75144d47aacc6c7f745d96272`

Actual user registration was not changed. Actual login remains unverified.
