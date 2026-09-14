# Native Task 3 SPEC re-review — ISSUES FOUND, FINAL_HOLD

The original three behavior counterexamples now pass, and S4's actual-secret assertions are meaningful and pass. **SPEC remains blocked by one directly reproduced P1 recovery defect.** Product source and held targets were not modified. QUALITY and Task 4 remain pending.

## Remaining finding S5 — P1: never-owned target retains stale file-creation ownership on retry

`product/src/config_patch/apply.rs:145-165` initializes an unstarted resource's `created` flag from the reviewed absent snapshot. A pre-write failure leaves this row without owned keys, backup references, or written/pending hashes. On retry, `plan_reviewed_targets` reuses the existing row (`:123-143`). The ownership demotion in `apply_one` (`:226-240`) is conditional on an existing `created_hash` or `pending_created_hash`, so it skips this truly unowned row even when a user has since created the file. The retry then records its entire candidate as created-file ownership (`:313-316,354-356`) and restore deletes the whole file (`restore.rs:124-139`).

Independent bounded counterexample, `probe.rs` fourth case:

1. Target is absent. Its synthetic parent is mode `0500`.
2. Reviewed apply fails before any client write; target remains absent.
3. Restore parent mode to `0700`; user creates `{"model":"user-original","user_added":"preserve-me"}`.
4. Preview and apply a retry that changes only `model` to `gateway-v2` using the same journal.
5. Restore returns successfully with `Restored` and zero preserved-created-files, but the entire user's file is absent.

`probe-result.json` records `first_apply_failed: true`, `file_exists_after_restore: false`, `final_value: null`, and `spec_pass: false`. This is a real macOS no-write failure/retry, not a hand-edited journal or crash-state simulation. Reinitialize truly unowned target metadata from the first actual protected preimage while preserving established ownership. Cover both pre-write-failed and never-started targets that a user creates before retry; do not promote planned intent to file ownership.

## Resolved findings and independent validation

- **S1 original:** create → user adds unrelated field → reconnect → restore now preserves the file. The contract additionally checks a user comment and removal of the gateway-owned key. Unchanged created-file reconnect still allows removal.
- **S2:** cross-connect descendant/ancestor ownership is refused before client writes; original probe now requires refusal and unchanged bytes, then verifies initial preownership restoration. Both directions pass in the contract tests. Exact/disjoint ownership still passes.
- **S3:** all three reviewed resources are present after second-resource failure, owned-key counts `[1,0,0]`. Contract checks `verified/failed/unstarted`, absent fabricated backups, and restoration of only the owned resource.
- **S4:** the test constructs actual `LocalDataToken`, `UpstreamCredential` and `ControlToken` values, uses a child to emit accepted/rejected transaction Debug, preview, apply, inspect/doctor, restore and errors, and asserts all distinct sentinels absent from stdout, stderr and journal. This passed. No code leakage was claimed by the initial review.
- The untouched product contract suite was separately compiled against the exact held library and executed on macOS: **35 passed, 0 failed**. Four tests are explicit child entry points; parent tests actually run them. This includes ordinary and created-reconnect child interruption, failed reconnect, partial restoration, mode/actual macOS ACL, lock/alias/TMPDIR and redaction cases. All fixture roots were empty after tests and then removed.
- Original three adapted probes pass. The adjacent fourth probe fails the spec as described above. The probe process returns zero because it records observations; per-case `spec_pass` is the acceptance result.
- Inspected all four changed files against the previous fixed hold: `apply.rs`, `journal.rs`, `restore.rs`, `patch_contract.rs`. No dependency, storage/atomic-replacement, parser, lifecycle/setup or hot-path changes. No broad benchmark or wizard rerun was necessary for this narrow review.

## Evidence identity and preservation

- New target final hold SHA-256: `7e549e645da2f86252a61b8d6223b586d213f3604a80494e885485e9f6ffe799`.
- New source manifest SHA-256: `3239733f0fda5ec89f6aff138a7cf4ceba92f915dcb472dbc6d70e2581967af0`.
- New source archive SHA-256: `c01ffa1ef1f3f7ba514b93a3a2cbe7ea2c67d92cfffdfbd8eced973f3fd055f3`.
- Both reviewer executables link the exact `product/target/native-task3-spec-fix/debug/deps/libllmgw-eba4665eb2ddaf45.rlib`, SHA-256 `4cbd76271788fd41f9a0f2683c38a15171105cde5b0a0d456f8f231c42982b44`. Commands are in execution JSON. This verifies the held library; it does not claim a fresh reviewer full-product build.
- `before.json`/`after.json` independently confirm all 79 source files, 79 archive members, 426 accepted protected files, and additional Task 3 evidence/binaries plus the original SPEC review files remain exact.
- The earlier unavailable provisional/pre-redaction capture remains unavailable. Nothing was reconstructed or conflated with the current hold.

The implementer's full299/release/fmt/check/clippy/build claims were not independently rerun. Windows/Linux runtime, real client or gateway process, OAuth/auth, external LLM/API/network, startup registration and benchmarks were not executed. No user config, paid call, Git command, commit or source edit occurred.

All reviewer-owned subprocesses exited and were reaped; synthetic roots were cleaned. **execution_finished=true; sole runtime/test ownership released; FINAL_HOLD.**
