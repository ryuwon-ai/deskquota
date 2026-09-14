# Native Task 3 final SPEC confirmation — PASS, FINAL_HOLD

**SPEC PASS remains valid on the final nonfinite-fix hold.** The accepted Q1–Q3 changes are consistent with the Task 3 safety contract and introduce no identified spec regression. S1–S5 remain resolved. This completes the requested narrow SPEC confirmation; Task 4 itself is still pending.

Compared all 79 manifest entries against the previously SPEC-passed unowned-fix archive. Exactly three paths differ: `storage.rs`, `document.rs`, and `patch_contract.rs`. Exact preserved-archive diffs are retained here.

- **Q1:** the adjacent fs4 lock keeps the normalized physical parent and resource basename plus a fixed suffix. Filesystem-equivalent case/Unicode spellings therefore resolve through the same actual lock entry even while the target is absent. There is no global case folding, process-temp dependency, or release-time unlink. This is consistent with the cooperative-lock requirement and makes no external-editor CAS promise.
- **Q2/Q3:** `toml_edit` AST inspection refuses an owned date/time or nonfinite float, including a nested owned subtree, before rendering can approve the preview. Direct apply starts by previewing, so this refusal precedes backup, state/journal, lock and client writes. Unrelated native TOML values remain untouched. These are the explicitly accepted safe-form refusals; no generic native-type conversion, compatibility layer or migration is required for Task 3.
- No dependency, ownership/journal/restore, private-file/ACL, secret-redaction, setup/lifecycle, adapter or hot-path source was changed since the previous SPEC pass.

## Direct regression replay

Recompiled the original four-case reviewer probe, unchanged from the preceding S5 pass, against the exact new held default debug rlib. **4 cases passed, 0 failed:** changed-created-file reconnect preservation; cross-connect overlapping ownership refusal with unchanged bytes and original-value restoration; complete three-target partial journal records; and pre-write-failed absent target followed by user creation, retry and restore. The last case preserved both `model=user-original` and `user_added=preserve-me`.

Commands, binary/library hashes and cleanup are in `probe-execution.json`; results are in `probe-result.json`. This is held-library replay, not a fresh full-product rebuild. The already independently executed QUALITY 10-probe/49-contract result was read with its exact known hash; this SPEC confirmation does not count those tests as freshly rerun. No broad benchmark, wizard, PTY or client matrix was repeated.

## Immutable identity and preservation

- Product final hold: `4bec8201c2e0b02c08815a61a907d8dbcbe5a2f9140acf33fc296ecec83aa313`.
- Source manifest: `68535b446b76c5f5f5fdf726f86232eb831b250340f3f5c5a9ccc58ac7e4c391`.
- Source archive: `7fceb9632164aa4709791b98ecd114a0cf9ce624fb38ef784c6ca7354d98146a`.
- Exact new default rlib: `7adb91e195f05aa349401d7ee1d36b3a984af664e0fb012ee5245880f01616f2`.
- Latest QUALITY final hold: `cd986ecd22743dc9e70fdc183887d32331e5a7925e08075696a2053719528548`.

`before.json` and `after.json` independently match all **79 source files, 79 archive members, 426 accepted protected files and all 357 extra files** from the latest QUALITY protection list, plus the explicit review/hold/library identities. Every drift list is empty. Earlier unavailable provisional evidence remains unavailable; no reconstruction is claimed.

Only new files in this final-confirmation directory were written. Product source, all default/held targets and prior evidence remain unchanged. All reviewer-owned subprocesses exited and were reaped; the isolated synthetic root was empty after execution and removed. No real user configuration/auth, gateway/client process, network/LLM/paid call, startup registration, VCS, reference edit or new OS-runtime claim occurred. Windows/Linux execution and Task 4 adapters remain outside this result.

**execution_finished=true; FINAL_HOLD; sole runtime/test ownership explicitly released.**
