# Native Task 4 SPEC-1 fix — FINAL HOLD

Status: `DONE_WITH_CONCERNS — FINAL_HOLD`

`execution_finished=true`. Product writer/runtime/test ownership is released for the same fresh SPEC reviewer. QUALITY has not started.

## Corrected behavior

The resolved native Claude `settings.json` target is now canonicalized and classified before a patch can be prepared. Ordinary native homes outside a Git worktree remain supported without requiring Git. When the canonical target is inside a worktree, llmgw requires it to be both untracked and ignored; tracked, unignored, or unknown states are refused. Safe parent-directory aliases remain supported. A symlinked target file is still refused by ConfigPatch.

The prepared profile retains the exact scope check and runs it again before gateway activation and immediately before the client write. This catches Git state changes after preview. Git subprocesses discard inherited repository/index/config redirection variables, including `GIT_DIR`, `GIT_WORK_TREE`, `GIT_INDEX_FILE`, and `GIT_CONFIG_PARAMETERS`, so an alternate index cannot authorize the target.

The native tracked-target unit regression preserves the client bytes and journal. The CLI regression exercises a missing route with `--restart`, a wrong hash, and a redirected `GIT_INDEX_FILE`; it refuses the shared target before gateway config, runtime state, journal, or client bytes change. Existing private native overrides, explicit private and untracked project-local configuration, and a canonical parent alias remain positive controls.

## Direct verification

- RED: `native-scope-red.log` records the new regression unexpectedly receiving `Ok` from prepare. That test did not apply the plan or observe a token write. The independent SPEC reviewer separately applied the held plan and observed the original write.
- Focused Claude scope: 9 passed, 0 failed in `claude-scope-focused-final4.log`.
- Same-reviewer held probe linked to the fresh rlib: the named blocker, private preservation/restore control, and hash/snapshot-before-activation control each passed.
- Full locked debug suite: 336 passed, 0 failed across 16 result rows in `cargo-test-full-final.log`.
- Locked release client profiles: 21 passed, 0 failed. Python driver tests: 7 passed, 0 failed.
- `cargo fmt --check`, locked all-target `cargo check`, locked all-target Clippy with warnings denied, and the final locked release build passed.
- Installed Claude Code 2.1.63: the generated profile ran without a driver auth environment; selection, Messages inference, exact synthetic Read tool result and follow-up, header stripping, gateway-off failure with zero upstream calls, disconnect, and installed-client reload passed. The probe matches the final release binary.

Pi and Codex were not rerun because the SPEC-1 implementation changes only Claude target-scope validation; their adapter tuple signatures changed only to use the same private type alias. Their earlier immutable installed-client evidence remains preserved and is not relabeled as fresh evidence.

## Frozen identities and preservation

- Final source manifest: `source-manifest-final2.json`, 88 files, SHA-256 `ffe2eaae543808401e51105cd948f071d7f73170e3c4b86c9253b7221c9a498f`.
- Final source archive: `source-hold-final2.tar.gz`, 279,756 bytes, SHA-256 `a8282024113d4da65b07642df98f1a5b093fcc941689e40c158f13f1763a4e33`.
- Release binary: 9,935,696 bytes, SHA-256 `4414957b603bc04d77cb55275d5ca335477645159c2509735ce84b3cd8013801`.
- Debug binary: SHA-256 `d61bc5de5fc89c8402753d6471aa75e99bfa1582621923e1063aa6987f15bd9f`.
- Fresh default-feature debug rlib: SHA-256 `9e658ae3ac6e2dea276bd5d81371389bf1d424f1e2d4fbfbacf65174f215bca0`.
- `Cargo.lock`: SHA-256 `c82e9357a135eb84f0d1be70d0a5f16d2a6c0f7a520cc140562306418a0d3e1`. `rust-toolchain.toml`: SHA-256 `a016e82d3b7986387adf4d7d9d88e1581f87bb2a9cfb91ecec4914411b07634b`.
- The final source differs from the integration hold in seven files, with no additions or removals. Pi and Codex changes are only the private return-type alias; ConfigPatch only exposes its existing canonicalizer within the crate.
- All 978 reviewer-held non-live-source entries, the 426 Task 2 protected entries, and the 357 Task 3 additional entries match with zero drift. The previous Task 4 manifest/archive, reviewer README/HOLD/after snapshot, and final Task 3 SPEC hold match their supplied hashes.
- Source remained stable after capture; the archive contains all 88 inputs plus its embedded manifest. No owned process or fixture path remained, and the bounded artifact scan found no generated secret sentinel in uncompressed logs, JSON, or retained executables.

## Concern and limits

One preserved focused run (`claude-scope-focused-final2.log`) observed a macOS ACL-path `EINVAL` while `disconnect` replaced the protected Claude journal, after client apply assertions had passed. The path-prefixed error came through exacl 0.13.0 in the existing ConfigPatch ACL-preservation path. The lower-level syscall cause is unconfirmed because it did not reproduce. The panicking fixture's `Drop` removed its temporary tree, so state after that error was not inspected. The exact test then passed in isolation, in 20 of 20 sequential repetitions, in two later focused runs, and in the full suite. No retry or masking behavior was added.

Actual runtime evidence remains macOS-only. Windows and Linux runtime are unverified. Claude discovery remains unsupported for 2.1.63, Codex catalog listing remains unverified, and no new latency or RSS claim follows from this fix. Historical performance/accounting matrices and unchanged cross-build failures were not rerun.
