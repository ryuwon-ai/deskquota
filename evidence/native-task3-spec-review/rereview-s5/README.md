# Native Task 3 SPEC S5 re-review — PASS, FINAL_HOLD

**SPEC PASS for the reviewed Task 3 scope.** S1–S5 are resolved by code inspection and the bounded direct checks described below. No remaining actionable spec finding was identified in this re-review. Fresh QUALITY review is still required; Task 4 is not accepted or implemented by this result.

## S5 resolution

The only production change since the preceding hold is `product/src/config_patch/apply.rs:261-272`: after taking the resource lock, reading/validating the current snapshot and preparing any required protected before-image, a resource with no owned state is initialized from that actual snapshot. Its `original_hash`, `created` state and privacy requirement therefore reflect the first real ownership attempt instead of stale planned intent. Already-owned resources bypass this reset, retaining the initial preownership value and the sticky S1 decision that a user-modified created file is no longer wholly owned. The other changed file is `product/tests/patch_contract.rs`, which adds three relevant tests. Both diffs are retained here.

The exact S5 counterexample was independently replayed with the same probe source as the previous failed re-review: absent file → parent mode0500 causes pre-write failure → user creates a file → repaired retry → restore. The file now remains, with `model=user-original` and `user_added=preserve-me`. The previous review recorded its deletion; this review records `spec_pass: true`.

## Directly executed

- **Four independent behavior probes: 4/4 pass.** These are the original S1 created-file/user-edit/reconnect case; S2 overlapping ownership with the reviewed refusal expectation, unchanged bytes and original-value restoration; S3 all-three-target partial journal inspection; and the S5 no-write failure/user-created-file/retry case.
- **Untouched current contract suite: 38 passed, 0 failed.** This includes all previous 35 contracts and the three ownership-initialization neighbors: failed absent→user-created existing file; later never-started absent→user-created existing file while prior ownership is retained; and failed planned-existing→absent before retry. The tests also exercise original S1–S4, actual typed-secret Debug/preview/apply/doctor/error/stdout/stderr/journal assertions, ordinary and created-reconnect crash recovery, partial restore, same-key/disjoint ownership, overlap refusal in both directions, aliases/TMPDIR lock identity, ordinary mode and actual macOS ACL preservation/refusal.
- Four of the 38 tests are explicit child entry points that no-op outside their fixture environment. Parent tests invoke the relevant children. Test-owned roots were empty after completion and removed. All child/probe processes were reaped.

`probe-result.json`, `probe-execution.json`, `contract-tests.stdout` and `contract-execution.json` contain the observations and exact commands. Both reviewer executables were newly compiled against the exact held default library `product/target/native-task3-unowned-fix/debug/deps/libllmgw-eba4665eb2ddaf45.rlib`, SHA-256 `0b05c38f6d2588af3899ff79c2afa31497b8f3d485421869fcde91fab755c612`. This is a held-library replay, not a claim that the reviewer rebuilt the entire product. Only this new review directory was written; no product source or held/default build target was mutated.

## Source and preservation

`before.json` and `after.json` independently match all **79 source files**, **79 archive members**, **426 accepted protected files**, and additional Task 3 artifacts/binaries and both prior SPEC review directories, with zero drift.

- Target final hold SHA-256: `863232acda87ca141cc2c4b62a44e31ff1f596eda4d049261aeb6b7b7b3d0e3f`.
- Source manifest SHA-256: `a73f9e5d7b587bce40ab7c6d4a70135540009c73cd657d0a3180bb8064a185bb`.
- Source archive SHA-256: `7fba05262ce872d1e845f8b9c32345ea71b13fdd6042077889354d858bb12f89`.

The missing early provisional/pre-redaction capture remains unavailable as previously documented; it was not reconstructed or conflated with any current hold. The implementer's full302/release/fmt/check/clippy/build claims were not independently rerun. No performance, Windows/Linux runtime, real-client or deployed-product claim is made. No gateway process, external request/LLM/paid call, real user config/auth, OAuth, startup registration, Git action or product edit occurred. Existing platform limitations remain explicit.

**execution_finished=true; sole runtime/test ownership released; FINAL_HOLD.** All reviewer-owned synthetic roots and processes are cleaned. Parent may begin a fresh QUALITY review on this exact hold.
