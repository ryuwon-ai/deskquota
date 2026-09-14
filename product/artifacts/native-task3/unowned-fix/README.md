# Native Task 3 unowned retry correction — FINAL HOLD

Status: `DONE_WITH_CONCERNS — FINAL_HOLD`

`execution_finished=true`; product writer/runtime ownership is released. This hold fixes only SPEC finding S5. The initial Task 3, lock-fix, and spec-fix holds remain immutable.

## Result

A failed or unstarted journal row with no owned state no longer carries preview-time file-creation ownership into a later retry. Under the resource lock, after any required protected backup succeeds, its original hash, created-file flag, created hashes, and privacy requirement are initialized from the first actual snapshot that begins ownership. Records with established owned keys, written hashes, pending hashes, or backups retain their original preownership state.

This preserves a user-created file when an earlier absent-file attempt failed or remained unstarted. It also handles the reverse boundary: if an existing preview never became owned and the user removes that file before a fresh reviewed retry, the later llmgw-created file is removable while its exact bytes remain owned.

Only these source files changed from the spec-fix hold:

- `product/src/config_patch/apply.rs`
- `product/tests/patch_contract.rs`

There are 79 source files. Twelve differ from the accepted Task 2 baseline and two differ from the spec-fix hold.

## RED and GREEN

- `patch-contract-unowned-red.log`: expected RED, 35 passed and 3 failed. The failures cover the reviewer's failed-first absent-file counterexample, an absent later target left unstarted by a partial transaction, and the reverse existing-to-absent first-ownership boundary.
- `patch-contract-final.log`: settled debug GREEN, 38 passed and 0 failed.
- `patch-contract-release-final.log`: release GREEN, 38 passed and 0 failed.
- The copied reviewer probe passed all four cases, including the original S5 counterexample; `spec-probe-result.json` now records the user file and both original fields after restore.

## Final verification

Directly executed on macOS with `CARGO_TARGET_DIR=/Users/ryuwon/Desktop/ryuwon-project/llm-gateway-research/product/target/native-task3-unowned-fix`:

- Focused debug: 38 passed, 0 failed.
- Focused release: 38 passed, 0 failed.
- Full debug suite: 302 passed, 0 failed across 15 rows: `[11, 0, 3, 19, 22, 26, 38, 12, 7, 19, 42, 32, 27, 44, 0]`.
- Adapted SPEC probe: 4 passed, 0 failed.
- `cargo fmt --check`, `cargo check --all-targets --locked`, and `cargo clippy --all-targets --locked -- -D warnings`: passed.
- `cargo build --locked` and `cargo build --release --locked`: passed.

`build-execution.json` records both build argv arrays, the absolute unowned-fix target, logs, and resulting binary identities. The release log is byte-identical to the earlier spec-fix log because both terse Cargo outputs reported the same package and a 12.61-second completion. The new command executed after the final source edit against the distinct unowned-fix target and produced the binary pinned below.

## Frozen identities

- Final hold JSON: `863232acda87ca141cc2c4b62a44e31ff1f596eda4d049261aeb6b7b7b3d0e3f`
- Source manifest: `a73f9e5d7b587bce40ab7c6d4a70135540009c73cd657d0a3180bb8064a185bb`
- Source archive: `7fba05262ce872d1e845f8b9c32345ea71b13fdd6042077889354d858bb12f89` (238,725 bytes)
- Task 2-relative changed-files manifest: `4c5be73a0f9a4ed04eb0c939352e4716b719510239ddc5a9811ba8b3132ab491`
- Spec-fix-relative changed-files manifest: `59fc0d959a2c07e0d616594f9738da62edbe62ca33ac1ba776f77647d9adf7b4`
- Release binary: `target/native-task3-unowned-fix/release/llmgw`, `c325b5ce341626e6b57585a76f2d8cc026f5db3199b651b7c98cd0f6cc2c29a5`, 9,313,936 bytes.
- Debug binary: `target/native-task3-unowned-fix/debug/llmgw`, `f1252148b2efc1edd67bf51c7210a7f373ee9c30f035bbcf6448da5beb15acc2`, 28,150,952 bytes.
- Probe-linked debug rlib: `target/native-task3-unowned-fix/debug/deps/libllmgw-eba4665eb2ddaf45.rlib`, `0b05c38f6d2588af3899ff79c2afa31497b8f3d485421869fcde91fab755c612`, 47,284,488 bytes.
- `Cargo.lock`: `c82e9357a135eb84f0d1be70d0a5f16d2a6c0f7a520cc140562306418a0d3e1c`.
- `rust-toolchain.toml`: `a016e82d3b7986387adf4d7d9d88e1581f87bb2a9cfb91ecec4914411b07634b`.
- rustc 1.88.0 commit `6b00bc3880198600130e1cf62b8f8a93494488cc`; cargo 1.88.0 commit `873a0649350c486caf67be772828a4f36bb4734c`.

## Preservation, cleanup, and limits

Final checks matched all 426 accepted Task 2 protected items and 149 additional Task 3/reviewer items, including the immutable spec-fix hold `7e549e...`, source `323973...`, archive `c01ffa...`, and rereview hold `713aa191...`. All 40 benchmark raws and 10 accounting raws remain unchanged. No owned process or matching fixture path remained.

Windows and Linux runtime remain unverified. External-editor final-check-to-rename filesystem CAS is not claimed. The unavailable early pre-redaction capture was not reconstructed. Task 4 adapters and CLI wiring remain outside this correction.

The source, archive, tests, and binaries are frozen. The same SPEC reviewer may now rerun; QUALITY and Task 4 remain pending their required handoffs.
