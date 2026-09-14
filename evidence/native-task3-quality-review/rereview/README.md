# Native Task 3 QUALITY re-review — Q1/Q2 resolved; Q3 CHANGES_REQUESTED

**QUALITY CHANGES_REQUESTED for this layer.** The original two findings are resolved by the quality-fix hold; one directly adjacent owned-TOML round-trip case still fails. No product source or held/default build target was changed by this review.

## Strengths and resolved findings

- **Q1 PASS.** The lock uses the normalized parent plus the original resource basename and a fixed suffix. Actual filesystem aliases therefore contend without custom case folding. The original absent-case held-lock/apply probe now rejects the competing write. A new independent Unicode alias test also verifies contention before creation, during restore after creation, and stable lock inode retention through restore/removal/recreation. These are actual fs4 contention checks, not an assertion that differently spelled PathBuf strings must compare equal. Case-sensitive runtime was not available; the product's conditional case-sensitive test returns early on this filesystem.
- **Q2 PASS.** The original owned datetime is refused at preview, and a direct apply attempt also refuses without client or state-directory writes. AST inspection distinguishes temporal values from ordinary marker-shaped user tables. Independent checks cover owned ArrayOfTables with time/date entries and Remove, an unrelated temporal sibling, and restoration of an ordinary marker-shaped inline table containing a valid date-looking string. No generic date conversion is required.
- Existing ownership S1–S5, privacy, macOS ACL, crash/recovery, and alias/TMPDIR contracts continue to pass. Partial restore/reconnect and escaped-key/nested-array independent probes also remain green. Only storage.rs, document.rs, and patch_contract.rs differ from the preceding held source; the three exact diffs are retained here.

## Important Q3 — Accepted nonfinite TOML ownership cannot restore

**Location:** `product/src/config_patch/document.rs:240-245,294-301` and `343-346`; restoration reads the original through the semantic parser at `product/src/config_patch/restore.rs:178-201`.

The owned-value AST refusal treats every `Value::Float` as supported. TOML `nan`, `+inf`, and `-inf` convert through `serde_json::Value` to Null. A reviewed finite replacement is accepted and written; restore then attempts to render that Null and fails with `TOML has no safe null value representation`.

**Direct reproduction:** `probe.rs:122-132`, `nonfinite_owned_toml_is_refused_or_roundtrips`, fails for `value = nan`. The separate `nonfinite-detail.rs` deterministically repeats `nan`, `+inf`, and `-inf`: start with `value = <literal>` and `keep = true`, preview/apply `value = 1`, then restore and retry restore. For **all three** cases:

- preview and apply succeed;
- restore returns an error, with journal `RestorePartial`;
- the current value remains the replacement `1`;
- the protected original before-image is retained and exact;
- the unrelated `keep` value remains intact;
- retrying restore fails for the same Null representation error.

`nonfinite-detail.json` retains these observations. Its process exit 0 means the expected failure observations were collected successfully, not that product restoration passed. The assertion-based independent test exits 101 for this defect.

**Impact:** A valid TOML original is accepted into ownership but cannot be restored by the advertised retry path. Protected recovery material remains available, so this is an Important correctness issue, not a claim of irreversible loss. Extend the narrow owned-AST safe-form refusal to nonfinite floats, including owned subtrees, before the first write; preserve unrelated nonfinite fields and finite numeric values. A lossless existing-library solution is also acceptable if simpler. No conversion framework, migration, or unrelated parser expansion is requested.

No separate Critical or Minor finding. Q1/Q2 remain closed; Q3 is the only current blocker.

## Direct execution

- Newly compiled independent probe suite: **7 passed, 1 failed**, exit 101.
- Newly compiled untouched current product contract suite: **46 passed, 0 failed**, exit 0. Four child entry-point tests no-op without their parent fixture environment; their parents invoke the real children.
- Newly compiled bounded Q3 observation executable: all **3/3** nonfinite cases reproduce the failure and retain exact before-images; exit 0 by design.

All executables were compiled inside this rereview directory against the exact new default-feature debug rlib `product/target/native-task3-quality-fix/debug/deps/libllmgw-eba4665eb2ddaf45.rlib`, independently verified SHA-256 `2dfb2de4ff95d0d4909dce3d342bf9ebd54422c2b4ad25c39eb46cf9c94d31a7`. `execution.json` and `nonfinite-execution.json` record compiler/test commands, statuses, executable hashes, elapsed time, and empty temporary roots. Old linked executables were not reused. This is held-library replay, not a fresh full product build, full310/release, or CI claim.

No gateway/real client, external network or paid/LLM call, user config/auth, startup registration, Git/release action, benchmark, or reference write occurred. Actual Windows/Linux execution and new performance/RSS remain unverified; the conditional case-sensitive test does not establish that runtime on this Mac.

## Preservation and final handoff

Before/after audit: **79 live source files, 79 archive members, 426 accepted protected files, and 283 additional protected files**, all with **zero drift**. The additional set covers all explicitly required original 226 items, original failed QUALITY review evidence, new quality-fix artifacts/binaries/rlib, and prior SPEC evidence. The unavailable earliest provisional bytes remain unavailable and were not reconstructed.

Reviewed hold SHA-256: `3b7912899d00f9164f447fc293909f570d9a8c0f101c23d631bd5f930e8d86b9`. Source manifest: `375024c646828619b8ddf5d40955db78f416fef69cc14be3b720c727fe2d4e41`. Archive: `32c628f0fceb06d3b731985d45137b27f6cd76b34a838e8a39611dc28131b135`.

All owned top-level processes completed and were reaped; contract children use RAII cleanup. All three isolated temporary roots were empty and removed, including failure cases. The original review directory's files were not changed; all new writes are under its `rereview/` subdirectory.

**execution_finished=true; FINAL_HOLD; runtime/test ownership explicitly released.** Return Q3 to the same implementer; Task 4 remains gated until acceptance.
