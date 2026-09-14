# Native Task 3 fresh SPEC corrections — FINAL HOLD

Status: `DONE_WITH_CONCERNS — FINAL_HOLD`

`execution_finished=true`; product writer/runtime ownership is released. This directory is the canonical implementation hold for the four fresh SPEC findings. The initial Task 3 and cooperative lock-fix holds remain immutable historical evidence.

## Result

The four confirmed findings are resolved with focused changes to ConfigPatch ownership and journal behavior:

- **S1 — created-file reconnect:** a newly created file remains eligible for whole-file removal only while its current bytes match a committed or pending llmgw-created hash. Detecting any user byte change permanently switches that resource to owned-key restore, so later reconnects cannot absorb the user's fields or comments into a new removable-file hash. Exact unchanged llmgw-created files still remove cleanly. Failed and interrupted reconnects retain the last committed removal boundary.
- **S2 — ownership overlap:** reconnect now rejects a new ancestor or descendant key that overlaps a key already owned by the journal. Validation occurs under the journal lock before client bytes change. Equal-key refreshes and disjoint keys remain supported, and the earlier ownership record remains restorable after rejection.
- **S3 — reviewed resource accounting:** every reviewed target is recorded as `unstarted` before the first resource write. Later rows move independently through verified or failed stages. Never-written rows carry no fabricated owned keys, backup reference, or pending hash; journal inspection explains they were never owned, and restore skips them without touching their files.
- **S4 — typed-secret coverage:** the contract suite now passes actual `LocalDataToken`, `UpstreamCredential`, and `ControlToken` values through Debug, validation errors, preview and diff summaries, apply and restore reports, journal inspection, subprocess stdout/stderr, and raw journal checks. The coverage found no current leak; this was a test gap rather than a reproduced disclosure.

The existing stable adjacent resource lock, canonical path identity, strict per-file JSON grammar, local-token privacy checks, macOS ACL behavior, partial recovery, and per-owned-key restore behavior remain intact. Setup persistence source was not changed.

## Changed source from the lock-fix hold

- `product/src/config_patch/apply.rs`
- `product/src/config_patch/journal.rs`
- `product/src/config_patch/restore.rs`
- `product/tests/patch_contract.rs`

There are 79 current source files. Four differ from the lock-fix hold and 12 differ from the accepted 71-file Task 2 baseline. See `spec-fix-changed-files.json` and `changed-files.json`.

## RED and GREEN evidence

- `patch-contract-spec-red.log`: expected RED, 29 passed and 4 failed. It reproduced deletion of a user-modified created file after reconnect, acceptance of both ownership-overlap directions, and omission of the third reviewed resource after a partial failure. The new typed-secret test passed against the held implementation, confirming S4 was a coverage gap.
- `patch-contract-spec-green-attempt-1.log`: failed intermediate, 32 passed and 1 failed. Production overlap rejection was correct; the test incorrectly required byte-identical JSON after restoring an ancestor table. The assertion was corrected to semantic JSON equality.
- `patch-contract-spec-green-attempt-2.log`: intermediate GREEN, 33 passed and 0 failed.
- `patch-contract-spec-green-attempt-3.log`: settled debug GREEN, 35 passed and 0 failed after adding interrupted-created-reconnect coverage.
- `patch-contract-final.log`: canonical copy of the settled 35/35 debug result; no source or test rerun occurred for the copy.
- `patch-contract-release-final.log`: release GREEN, 35 passed and 0 failed.

The adapted reviewer probe in `spec_probe.rs` passed all three direct counterexamples. `spec-probe-result.json` records preservation of a user-added field after created-file reconnect and restore, refusal of reconnect overlap before byte changes with successful prior restore, and a three-resource partial journal with stages `verified`, `failed`, and `unstarted` and owned-key counts `1`, `0`, and `0`.

## Final verification

Directly executed on macOS with `CARGO_TARGET_DIR=/Users/ryuwon/Desktop/ryuwon-project/llm-gateway-research/product/target/native-task3-spec-fix`:

- `cargo test --test patch_contract --locked`: 35 passed, 0 failed.
- `cargo test --release --test patch_contract --locked`: 35 passed, 0 failed.
- `cargo test --locked`: 299 passed, 0 failed across 15 top-level result rows: `[11, 0, 3, 19, 22, 26, 35, 12, 7, 19, 42, 32, 27, 44, 0]`.
- Adapted SPEC probe: 3 passed, 0 failed.
- `cargo fmt --check`: passed with empty output.
- `cargo check --all-targets --locked`: passed.
- `cargo clippy --all-targets --locked -- -D warnings`: passed.
- `cargo build --locked`: passed.
- `cargo build --release --locked`: passed.

The 35 focused contracts include the four fresh findings plus stable adjacent-lock alias/TMPDIR identity, repeated-connect ownership, local-token permissions, Debug/error redaction, actual macOS extended ACLs, strict JSON and comments-only JSON boundaries, concurrent edits, multi-file partial failure, bounded child interruption, restore conflicts, and new-file handling.

## Frozen identities

- Final hold JSON: `7e549e645da2f86252a61b8d6223b586d213f3604a80494e885485e9f6ffe799`
- Source manifest: `3239733f0fda5ec89f6aff138a7cf4ceba92f915dcb472dbc6d70e2581967af0`
- Source archive: `c01ffa1ef1f3f7ba514b93a3a2cbe7ea2c67d92cfffdfbd8eced973f3fd055f3` (238,142 bytes)
- Task 2-relative changed-files manifest: `d48131ee287b301b71ecbfdb6cd954820ab938c4bcec439c865b2974df953687`
- Lock-fix-relative changed-files manifest: `d3b65129c98e2c4e1c78694c0532eb863bcc3084f21eeeac9ca03fa76463f19f`
- Release binary: `target/native-task3-spec-fix/release/llmgw`, `4472df4f3fb68000cf31d4eb33edc4fa9faf9176b0fb04b18db3a0d1184573e4`, 9,314,064 bytes.
- Debug binary: `target/native-task3-spec-fix/debug/llmgw`, `a0e5572d4010d338c858acc64d2ef20e3a9355a496efd988803eaa4b3429e706`, 28,148,600 bytes.
- Probe-linked debug rlib: `target/native-task3-spec-fix/debug/deps/libllmgw-eba4665eb2ddaf45.rlib`, `4cbd76271788fd41f9a0f2683c38a15171105cde5b0a0d456f8f231c42982b44`, 47,283,208 bytes.
- `Cargo.lock`: `c82e9357a135eb84f0d1be70d0a5f16d2a6c0f7a520cc140562306418a0d3e1c`, 52,711 bytes.
- `rust-toolchain.toml`: `a016e82d3b7986387adf4d7d9d88e1581f87bb2a9cfb91ecec4914411b07634b`, 52 bytes.
- rustc 1.88.0 commit `6b00bc3880198600130e1cf62b8f8a93494488cc`.
- cargo 1.88.0 commit `873a0649350c486caf67be772828a4f36bb4734c`.

## Preservation and cleanup

A fresh preservation check matched all 426 accepted Task 2 protected artifacts and 90 additional Task 3 artifacts from the SPEC-review baseline, including all 40 benchmark raw files and 10 accounting raw files. It also matched the initial Task 3 manifest (`6cdfbec0...`), archive (`17c82d6...`), hold (`601a465a...`), debug binary (`66142c7d...`), and release binary (`21551135...`), plus the lock-fix manifest (`f73a4345...`), archive (`1b4b96aa...`), hold (`903a07a4...`), debug binary (`9a5fa1d...`), and release binary (`6b7a35b2...`). The fresh SPEC review hold remains `39711385...`, and the parent's canonical lock rerun remains `25430315...`.

`cleanup-final.json` records no owned process and no matching temporary path. The first probe wrapper run passed its cases but then hit zsh's read-only `status` variable; that evidence is preserved in `spec-probe-run-attempt-1.txt`, its empty owned root was removed, and the canonical rerun completed. `finalize-attempt-1.txt` preserves a finalizer assumption that exactly one debug rlib would exist; all-target compilation legitimately produced another rlib, so final evidence was pinned to the exact default-feature rlib used by the probe. No source changed after capture.

## Limits and evidence concern

The earlier unavailable pre-redaction capture remains unavailable and was not reconstructed. External editors can still race the final metadata/hash check and atomic rename; ConfigPatch does not claim filesystem CAS. Windows and Linux received static cfg validation only and have no runtime claim. Task 4 adapters and CLI wiring remain outside this work.

The source, archive, tests, and binaries are frozen at this hold. Fresh SPEC review may now begin; QUALITY and Task 4 remain pending their required handoffs.
