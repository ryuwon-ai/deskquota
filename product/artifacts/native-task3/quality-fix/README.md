# Native Task 3 QUALITY corrections — FINAL HOLD

Status: `DONE_WITH_CONCERNS — FINAL_HOLD`

`execution_finished=true`; product writer/runtime ownership is released. This hold fixes only QUALITY findings Q1 and Q2. All earlier Task 3 holds, failed probes, binaries, and source archives remain immutable.

## Result

- **Q1:** ConfigPatch now derives a lock from the normalized physical parent and the target filename plus `.llmgw-config-patch.lock`. The underlying filesystem therefore makes absent case and Unicode aliases contend on the same stable fs4 lock. No lowercasing or custom case folding is used, so distinct names remain independent on case-sensitive filesystems. The stable lock file is not unlinked on release.
- **Q2:** TOML preview now inspects the actual `toml_edit` AST at each owned key. A native date/time value anywhere in that owned value or subtree is rejected with a sanitized error before backup, journal creation, or client write. Unrelated date/time fields remain supported, and an ordinary inline table containing the serde marker-shaped key is treated as ordinary user data.

Changed source from the unowned-fix hold:

- `product/src/config_patch/storage.rs`
- `product/src/config_patch/document.rs`
- `product/tests/patch_contract.rs`

There are 79 source files. Twelve differ from the accepted Task 2 baseline and three differ from the unowned-fix hold. No dependency or lockfile changed.

## RED and GREEN

- `patch-contract-quality-red.log`: expected RED, 41 passed and 4 failed. Both actual absent-path aliases bypassed the hashed locks, and both direct/nested TOML native-value refusal tests were accepted.
- `patch-contract-quality-green-attempt-1.log`: 45 passed after the two production fixes, before adding the explicit lock-name contract.
- `patch-contract-quality-green-attempt-2.log`: 45 passed after formatting the initial new tests.
- `patch-contract-quality-green-attempt-3.log`: 46 passed after adding the lock-name contract; the preceding format check requested only multiline assertion layout.
- `patch-contract-final.log`: final formatted debug GREEN, 46 passed and 0 failed.
- `patch-contract-release-final.log`: release GREEN, 46 passed and 0 failed.
- The adapted QUALITY probe passed all four tests, covering both original failures and both neighboring regression cases.

## Final verification

Directly executed on macOS with `CARGO_TARGET_DIR=/Users/ryuwon/Desktop/ryuwon-project/llm-gateway-research/product/target/native-task3-quality-fix`:

- Focused debug: 46 passed, 0 failed.
- Focused release: 46 passed, 0 failed.
- Full debug suite: 310 passed, 0 failed across 15 rows: `[11, 0, 3, 19, 22, 26, 46, 12, 7, 19, 42, 32, 27, 44, 0]`.
- Adapted QUALITY probe: 4 passed, 0 failed.
- `cargo fmt --check`, `cargo check --all-targets --locked`, and `cargo clippy --all-targets --locked -- -D warnings`: passed.
- `cargo build --locked` and `cargo build --release --locked`: passed.

## Frozen identities

- Final hold JSON: `3b7912899d00f9164f447fc293909f570d9a8c0f101c23d631bd5f930e8d86b9`
- Source manifest: `375024c646828619b8ddf5d40955db78f416fef69cc14be3b720c727fe2d4e41`
- Source archive: `32c628f0fceb06d3b731985d45137b27f6cd76b34a838e8a39611dc28131b135` (240,116 bytes)
- Task 2-relative changed-files manifest: `21fa1afa2f0e94b851e256df9434163ee171ac8bbf1121c1a28782147fee84fa`
- Unowned-fix-relative changed-files manifest: `0a23a92f8422719a9b86da94780c54c32c78f7e10166ec9a3f6d61aa4bc3a68e`
- Release binary: `target/native-task3-quality-fix/release/llmgw`, `8b6fde7b380d00568b3ce6fbcc2ea21b577cfdc50d730a01955355d8ca16b233`, 9,315,184 bytes.
- Debug binary: `target/native-task3-quality-fix/debug/llmgw`, `15eac52098867407b4f06ed3c39e3fc87b69ecd67c063979da2852fac3682e65`, 28,150,984 bytes.
- Probe-linked debug rlib: `target/native-task3-quality-fix/debug/deps/libllmgw-eba4665eb2ddaf45.rlib`, `2dfb2de4ff95d0d4909dce3d342bf9ebd54422c2b4ad25c39eb46cf9c94d31a7`, 47,457,624 bytes.
- `Cargo.lock`: `c82e9357a135eb84f0d1be70d0a5f16d2a6c0f7a520cc140562306418a0d3e1c`.
- `rust-toolchain.toml`: `a016e82d3b7986387adf4d7d9d88e1581f87bb2a9cfb91ecec4914411b07634b`.
- rustc 1.88.0 commit `6b00bc3880198600130e1cf62b8f8a93494488cc`; cargo 1.88.0 commit `873a0649350c486caf67be772828a4f36bb4734c`.

## Preservation, cleanup, and limits

Final checks matched all 426 accepted Task 2 protected items and 226 additional Task 3/reviewer items, including the frozen unowned-fix hold `863232ac...`, source `a73f9e5d...`, archive `7fba0526...`, and QUALITY review hold `e3b1733d...`. All 40 benchmark raws and 10 accounting raws remain unchanged. No owned process or matching fixture path remained.

Actual macOS tests proved case-insensitive and Unicode-normalizing absent aliases share one lock. Windows and Linux runtime remain unverified. Native TOML date/time ownership is explicitly unsupported and refused before write. External-editor final-check-to-rename filesystem CAS is not claimed. The unavailable early pre-redaction capture was not reconstructed. Task 4 remains outside this work.

The source, archive, tests, and binaries are frozen. The same QUALITY reviewer may now rerun Q1/Q2; Task 4 remains gated on acceptance.
