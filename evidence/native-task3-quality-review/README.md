# Native Task 3 QUALITY review — CHANGES_REQUESTED, FINAL_HOLD

Review scope: the focused ConfigPatch library and extraction of the shared Windows replacement helper, relative to the accepted Native Task 2 source archive. No Git metadata exists in research/product; no Git operations were used. Task 4 client adapters, Task 5 startup, and Task 6 packaging are outside this gate.

## Strengths

- The six ConfigPatch modules separate API/validation, document editing, storage, apply, restore, and journal inspection. The implementation uses existing `toml_edit` and `fs4`, adds only pinned `jsonc-parser 0.33.1` with narrowly selected features, and shares the existing Windows replacement-recovery implementation instead of introducing a fallback framework. The changed Cargo.lock adds only that package and its llmgw dependency edge.
- Current code records reviewed resources before writes, keeps first-preownership before-images for established ownership, and initializes genuinely unowned retries from their actual snapshot. Independent replay of all 38 unchanged contract tests passed, including S1–S5, typed-secret redaction, actual macOS ACL cases, alias/TMPDIR locks, partial recovery, and owned crash children.
- Two new independent neighboring probes passed: partial restore followed by same-key/disjoint reconnect retained the first original values and unrelated user changes; escaped property names and nested array originals round-tripped while retaining an unrelated JSON comment.

## Important issues

### Q1 — Missing case-equivalent paths use different cooperating locks

**Location:** `product/src/config_patch/storage.rs:170-180` and `199-204`.

For absent paths, normalization canonicalizes the existing ancestor but appends missing names verbatim. On the actual case-insensitive macOS fixture filesystem, absent `SETTINGS.JSON` and `settings.json` therefore produce different adjacent lock names for the same directory entry. A held `cooperating_lock_path(SETTINGS.JSON)` does not stop `apply(settings.json)` from successfully creating the target. This is the supported cooperating-editor boundary, not an external-editor CAS claim.

**Direct reproduction:** `probe.rs:21-35`, test `absent_case_alias_must_honor_existing_cooperating_lock`. The fixture first verifies the filesystem actually equates case aliases using an unrelated synthetic marker, creates and holds the public API's upper-case resource lock using fs4, then previews/applies the absent lower-case path. Observed `different_lock=true, apply_succeeded=true, target_written=true`; expected refusal and no client write. All fixture files and the held lock descriptor were cleaned during unwind.

**Impact:** New resources can bypass a cooperating editor's lock; the current existing-file case-alias test misses this creation boundary. Use a stable identity/locking rule that also covers missing entries on the actual filesystem, with appropriate alias-transition tests. Preserve distinct names on case-sensitive filesystems; blanket lowercasing is not an acceptable fix. A narrowly documented safe refusal is preferable to claiming serialization for an unsupported form.

### Q2 — TOML original values lose their type during restore

**Location:** `product/src/config_patch/document.rs:21-26`, `329-335`; restoration enters this conversion at `product/src/config_patch/restore.rs:178-201`.

Before-images are parsed through `serde_json::Value`, so a TOML datetime becomes toml_edit's private datetime marker object. Restore routes that object through `toml_value`, which emits an ordinary inline table. The result parses successfully and is marked restored, despite changing the original value's type.

**Direct reproduction:** `probe.rs:38-45`, test `toml_datetime_owned_value_roundtrip_keeps_its_type`. Starting with `last_used = 2026-09-13T07:00:00Z` and `keep = 42`, apply a reviewed string value to `last_used`, then restore. Restore succeeds with no conflicts but produces `last_used = { "$__toml_private_datetime" = "2026-09-13T07:00:00Z" }`. `keep` remains intact. No real configuration was used.

**Impact:** A supported TOML transaction can report successful recovery while changing the original semantic type. Restore the owned original TOML item using the maintained TOML representation, or reject non-round-trippable owned values before the first client write. Do not treat a user-authored ordinary marker-shaped table as a datetime merely by matching its key. A focused refusal satisfies this layer without broadening the editor into a conversion framework.

## Critical / Minor

No separate Critical or Minor finding. The two Important issues above are concrete correctness blockers; no future adapter wiring, platform runtime, performance work, or style-only change is required by this review.

## Direct execution and limitations

`execution.json` records exact compiler/test commands, toolchain, exit codes, binary hashes, durations, and cleanup. Review binaries were newly compiled with Rust 1.88.0 against the exact held default-feature rlib, SHA-256 `0b05c38f6d2588af3899ff79c2afa31497b8f3d485421869fcde91fab755c612`; that file's bytes were independently hashed. This is held-library execution, not a full product rebuild.

- Fresh independent probes: **2 passed, 2 failed**, exit 101. Negative output is retained in `probe.stdout` and `probe.stderr`.
- Unchanged current contract tests: **38 passed, 0 failed**, exit 0. Four tests are child entry points that no-op outside the parent fixture environment; their parent tests invoke them. Output is retained in `contract.stdout` and `contract.stderr`.
- Both top-level executions completed and were reaped. RAII fixture/child cleanup completed; both isolated TMPDIR roots were empty and removed. No review-owned process remains. No gateway, real client, network/LLM/paid API, real user config/auth, startup registration, Git/release, reference checkout write, or source change occurred.
- No fresh full302/release/build/check/clippy, performance/RSS, Windows, or Linux runtime claim is made. Existing Windows failure-model tests and macOS ACL tests do not establish other-OS support. The extracted Windows helper was reviewed in the source diff; its actual Windows runtime remains unverified.

## Preservation and handoff

`before.json` and `after.json` match **79 source files, 79 archive members, 426 accepted protected files, and 226 additional protected files**, all with zero drift. The additional set includes all 206 explicitly required Task 3 artifacts/held binaries/prior reviews plus the latest SPEC S5 evidence. The earlier unavailable provisional/pre-redaction bytes remain unavailable; they were not reconstructed or presented as preserved.

Exact reviewed head: `product/artifacts/native-task3/unowned-fix/final-hold.json`, SHA-256 `863232acda87ca141cc2c4b62a44e31ff1f596eda4d049261aeb6b7b7b3d0e3f`; source manifest SHA-256 `a73f9e5d7b587bce40ab7c6d4a70135540009c73cd657d0a3180bb8064a185bb`; archive SHA-256 `7fba05262ce872d1e845f8b9c32345ea71b13fdd6042077889354d858bb12f89`.

**QUALITY CHANGES_REQUESTED for this layer.** Keep Task 4 gated; return Q1/Q2 and these frozen probes to the same implementer for focused fixes and subsequent SPEC/QUALITY review as applicable.

**execution_finished=true; FINAL_HOLD; all runtime/test ownership explicitly released.** Only this evidence directory was written. The next owner may begin after this handoff.
