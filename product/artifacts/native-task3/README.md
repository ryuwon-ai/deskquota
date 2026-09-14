# Native Task 3 ConfigPatch FINAL_HOLD

Status: **FINAL_HOLD**. `execution_finished=true`; writer/runtime ownership is released for fresh SPEC review.

ConfigPatch now provides reviewed JSON, comments-only JSON, and TOML edits with protected before-images, stageful raw-free journals, per-resource locks, same-directory replacement, immediate snapshot and permission/ACL rechecks, exact preview-hash enforcement, and value-aware disconnect restoration. Repeated connect preserves the first pre-ownership value. Partial apply, interrupted apply, and partial restore retain explicit recovery state. Existing setup behavior is unchanged; only its Windows ambiguous-replacement helper moved into the shared narrow storage module.

Direct final verification: **19/19** focused debug contracts, **19/19** optimized contracts, and **283/283** full debug tests. Rustfmt, `cargo check --all-targets --locked`, `cargo clippy --all-targets --locked -- -D warnings`, and `cargo build --release --locked` passed. macOS runtime fixtures covered permissions, ACLs, partial writes/restores, and child interruption. Windows evidence is static plus three shared replacement failure-model tests executed on macOS; there is no Windows runtime claim.

Source: 79 files; 12 changed from the accepted 71-file Task 2 baseline. Manifest `6cdfbec0b41839689364345e4b973a2f6af9409234fc5e2189f77f4144873e9a`; archive `17c82d6aec084de464f4ae4b1eca850772c46f195d55997dcd534b0b15b4451d` (234307 bytes). Release `215511357446c5583ebd5d4a3d9aee6b8ea5df4d83387d31d0ce23ae2a97eb6d` (9314736 bytes); debug `66142c7dd7cd21a500b4075e77bf840283c4741ef178b275a09a5f19d3ba504c` (28254424 bytes); Cargo.lock `c82e9357a135eb84f0d1be70d0a5f16d2a6c0f7a520cc140562306418a0d3e1c`.

All 426 prior protected artifacts matched the accepted rereview manifest, including 40 benchmark raw event files and 10 accounting raw event files. No owned Task 3 process or temporary test/lock path remains. Synthetic upstream, local-data, and control-token sentinels were absent from every Task 3 log.

Earlier failed attempts are preserved. In particular, `json-boundary-green.log`, `patch-contract-final-focused.log`, and `restore-partial-green.log` are failed intermediate runs despite their filenames. Canonical final evidence is `patch-contract-final.log`, `cargo-test-full-final.log`, `patch-contract-release-final.log`, and `final-hold.json`.
