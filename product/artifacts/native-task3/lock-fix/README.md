# Native Task 3 cooperative lock correction — FINAL HOLD

Status: `DONE_WITH_CONCERNS — FINAL_HOLD`

`execution_finished=true`; product writer/runtime ownership is released. This directory is the canonical correction hold for the Task 3 cooperative resource lock contract. The initial `artifacts/native-task3/` hold remains unchanged as evidence of the implementation against which the alias/TMPDIR bypass was reproduced.

## Result

ConfigPatch now resolves each supported client resource to one normalized physical path before preview hashing, resource locking, journal ownership matching, apply, and restore. The lock is a stable private sibling file in the resource directory, so its location does not depend on `TMPDIR`, `TMP`, `TEMP`, or the transaction journal directory. The file is retained after unlock so concurrent open handles cannot diverge across lock-file unlink/recreation.

Existing resource aliases including `client/../client/settings.json`, parent-directory symlink aliases, relative/absolute spellings, and case spellings that this host filesystem resolves to the same directory entry converge through filesystem canonicalization. A new path is resolved from its nearest existing physical ancestor. An unresolved parent alias through a missing component is explicitly refused before preview or write. Leaf symlinks and Windows reparse-point forms remain refused.

Apply and restore acquire the same resource lock. Journal resource paths are normalized, so repeated connect through an equivalent spelling preserves one resource record and the first preownership value. If adjacent lock creation fails before a resource can be read or owned, the journal records a sanitized failed resource row with no fabricated owned keys or backup reference.

The adjacent lock uses the existing `fs4` and protected-file open primitives. It does not chmod an existing client directory. Actual macOS tests verified a `0755` parent remains `0755` through apply and restore. Setup persistence was not changed by this correction.

## Changed source from the initial Task 3 hold

- `product/src/config_patch/apply.rs`
- `product/src/config_patch/restore.rs`
- `product/src/config_patch/storage.rs`
- `product/tests/patch_contract.rs`

There are 79 current source files. Four differ from the initial Task 3 hold; 12 differ from the accepted 71-file Task 2 baseline. See `lock-fix-changed-files.json` and `changed-files.json`.

## RED and GREEN evidence

- `patch-contract-lock-red.log`: expected RED, top-level suite 18 passed and 5 failed. Failures directly covered alias apply while the original spelling was locked, process temp-directory identity, alias restore, case aliasing on this filesystem, and repeated-connect journal duplication.
- `patch-contract-lock-green-attempt-1.log`: failed intermediate, 21 passed and 2 failed. It exposed the earlier adjacent-lock failure stage and canonical report-path expectations.
- `patch-contract-lock-green-attempt-2.log`: intermediate GREEN, 23 passed.
- `patch-contract-final.log`: canonical debug GREEN, 25 passed and 0 failed.
- `patch-contract-release-final.log`: canonical release GREEN, 25 passed and 0 failed.

Child test-process output is suppressed in canonical logs, so each focused log contains one top-level `test result` row. The RED log predates that evidence cleanup and includes successful child rows before its top-level result; its stated 18/5 count is the top-level suite result, not a sum of rows.

## Final verification

Directly executed on macOS with `CARGO_TARGET_DIR=/Users/ryuwon/Desktop/ryuwon-project/llm-gateway-research/product/target/native-task3-lock-fix`:

- `cargo test --test patch_contract --locked`: 25 passed, 0 failed.
- `cargo test --release --test patch_contract --locked`: 25 passed, 0 failed.
- `cargo test --locked`: 289 passed, 0 failed across 15 result rows: `[11, 0, 3, 19, 22, 26, 25, 12, 7, 19, 42, 32, 27, 44, 0]`.
- `cargo fmt --check`: passed with empty output.
- `cargo check --all-targets --locked`: passed.
- `cargo clippy --all-targets --locked -- -D warnings`: passed.
- `cargo build --locked`: passed.
- `cargo build --release --locked`: passed.

The focused suite retained the local-token privacy, Debug/error redaction, real macOS extended-ACL, ordinary file metadata, two-file partial apply/restore, bounded child interruption, failed repeated connect, and changed-created-file preservation contracts.

Windows and Linux were not executed. The new platform-gated Windows UTF-16 path digest and existing Windows protected-file open path received only host parser/static inspection; this hold makes no Windows runtime claim. The known Windows SDK/aws-lc cross-target blocker was not retried.

## Frozen identities

- Final hold JSON: `903a07a420fc899e756d890703fad42264adca179d969094a288d598d6782f2d`
- Source manifest: `f73a434551c85776950dd76794a86e14f05846dc366609f827f28a204613060c`
- Source archive: `1b4b96aa32e2c4414eb52aaf87ed2382cb40d310974b01adef63c90b20bbbe2e` (236,051 bytes)
- Task 2-relative changed-files manifest: `7d79d9f12142b200698cdbb3257b9397301a279589d911bf9578f2f3a19ace26`
- Initial Task 3-relative lock-fix manifest: `1c97d111169d4f757c1706605339cb77cd7da0a5c0d6783ccd867e93ce1b6003`
- Release binary: `target/native-task3-lock-fix/release/llmgw`, `6b7a35b2b340037891cdf77a7b1c50fadfcc0767677bfcf7ec1fb8595a7c0598`, 9,314,160 bytes.
- Debug binary: `target/native-task3-lock-fix/debug/llmgw`, `9a5fa1d6754c4cb88061434d253429e0b89586b5eedf212de0277fa83227da91`, 28,148,632 bytes.
- `Cargo.lock`: `c82e9357a135eb84f0d1be70d0a5f16d2a6c0f7a520cc140562306418a0d3e1c`, 52,711 bytes.
- `rust-toolchain.toml`: `a016e82d3b7986387adf4d7d9d88e1581f87bb2a9cfb91ecec4914411b07634b`, 52 bytes.
- rustc 1.88.0 commit `6b00bc3880198600130e1cf62b8f8a93494488cc`.
- cargo 1.88.0 commit `873a0649350c486caf67be772828a4f36bb4734c`.

## Preservation and cleanup

A fresh check matched all 426 accepted Task 2 protected artifacts, all 40 benchmark raw JSON files, and all 10 accounting raw JSON files. The initial Task 3 source manifest (`6cdfbec0...`), source archive (`17c82d6...`), final hold (`601a465a...`), debug binary (`66142c7d...`), and release binary (`21551135...`) remain unchanged. The prior `superseded/pre-acl` capture is also retained with its current exact identities in `protected-check.json`.

The first finalization attempt is preserved as `cleanup-final-attempt-1.json`; it stopped because the intentional RED run left its old implementation's per-user temporary lock directory. After confirming that it contained only zero-byte, user-owned synthetic lock files and that no owned process remained, that exact directory was removed. Final cleanup found no owned process and no matching Task 3 fixture, Windows fixture, or old global-lock temporary path.

## Limits and evidence concern

The earlier pre-redaction capture that had already been overwritten before this correction remains unavailable. It was not reconstructed. The preserved `superseded/pre-acl` snapshot and both immutable Task 3 holds remain distinct.

Cooperating llmgw processes now share one resource identity for the tested supported spellings. External editors can still race the last metadata/hash check and atomic rename; ConfigPatch does not claim filesystem CAS. Task 4 adapters, client CLI wiring, login, autostart, installer behavior, real user config changes, credentials, external calls, benchmarks, accounting reruns, Git, and release publication remain outside this correction.
