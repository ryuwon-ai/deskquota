# Native Task 6 final SPEC confirmation

Verdict: **Q1/Q2 and S1 SPEC confirmation PASS; one root-observed P3 packaged-document follow-up remains.** The quality fixes preserve the accepted installer, package-identity, and recovery-security contracts. The P3 does not invalidate those fixes, but the package reference should be corrected and verified before a clean final documentation result.

## Direct byte and evidence checks

- Current source manifest: 101 live paths, SHA-256 `d00ff4d106cd374857d4738441c3406a8085e8344d3c52e845f81a82763a88c7`, zero drift.
- Source archive: 102 unique regular members, exact embedded manifest, every byte matched, SHA-256 `0d8897fcd9300f433740f3fbd24edc042f8b7706e18715b50c5fc8a90de3ec2b`.
- The only changes from the accepted S1 documentation fix are `product/packaging/install.ps1`, `product/scripts/package_native.py`, `product/tests/test_installers.py`, and `product/tests/test_package_native.py`; no addition or removal.
- User package: 4,202,653 bytes, exactly five unique regular members, byte-identical to the accepted S1 package, SHA-256 `ba03203e9416a4eb75ca972fb0b3e5eba8618c46b2c26c7f65e2b7ffff387a22`. All docs equal current source; packaged `llmgw` equals the final-v2 binary at `cf7c436c442c426a6e9a1d485a5261fe5e5a05c8b131b130e679e00eec2c646b`.
- Quality-fix HOLD: 17 current rows including `FINAL_HOLD.json` (`79eb12f4184f37597baf0d7897b61fe8d6c0aaf89300acdff75faf4062276c2a`), zero drift.
- QUALITY rereview: PASS, zero remaining findings, all six listed files plus its HOLD (`682f13f1cedb9f88b42e30630c80ea0756a8a13672b5b91a032c78e283aa4fe4`) matched.
- Historical preservation used the existing parent audit: 1,680 rows, zero drift. It was not redundantly recollected.

## Static contract confirmation

`product/scripts/package_native.py:40,62,79` reads the executable into `contents["llmgw"]`, writes that captured value to the archive, and returns the SHA-256 of the same captured value. A later change to the input path cannot relabel the archived member.

`product/packaging/install.ps1:103-105` catches a failed replacement, records the original error, and reports target, destination-local backup, and candidate. The catch has no copy, move, remove, or `preserveRecovery=false`; the script has no target removal; final candidate/backup cleanup remains guarded by `-not $preserveRecovery`. It therefore leaves an ambiguous state untouched for the documented manual recovery flow.

`product/docs/installation.md:121-134` remains unchanged and accurate: stop, retain target/backup/candidate, recognize the backup-only and target-absent state, establish known previous identity and no competing writer, copy before any optional move, retain recovery copies through checksum/version/execution verification, and never blindly overwrite. “Original and recovery errors” means retain the complete emitted replacement error and the recovery-unconfirmed state; the fixed catch performs no automatic recovery operation and therefore creates no second recovery exception. Lines 116–119 still mark Windows execution unverified.

## Root-observed P3 follow-up

`product/docs/runtime-contract.md:907` links `[Benchmark method](benchmark-method.md)`, which resolves to `docs/benchmark-method.md` in a source checkout. The exact five-member user package contains `docs/runtime-contract.md` but omits `docs/benchmark-method.md`, so the packaged link is broken. A direct archive listing confirmed the target is absent. This is a documentation navigation defect, not a Q1/Q2 runtime or security regression.

Replace the packaged relative link with text that explicitly identifies the benchmark method as source-checkout-only, or include a required supported document consistently in the package and installer allowlists. The root has selected the former one-document correction. Verify the resulting source/archive/package binding after that change.

## Limits and retained evidence

This confirmation did not rerun source generation, Cargo, Rust365, the old Python64 suite, the five current focused tests, clients, installer smoke, or native measurement. The five focused passes are retained current packaging evidence; Rust365 remains retained evidence for byte-identical compiled inputs and binary. Python64 predates the packaging changes and is not presented as a fresh current total. Q2 is source/static proof because PowerShell and Windows were unavailable.

The original QUALITY HOLD correction remains explicit: the current retained HOLD is `c0b26468b3e94adf6ab801fb81b29ee04f8a250993e3a1f490e899676631205e`; `evidence/native-task6-quality-parent-check.json` records the earlier announced value and that the unavailable earlier raw HOLD was not reconstructed. This confirmation did not regenerate it or the ignored unheld pyc files.

Actual Windows/PowerShell, Windows/Linux/macOS-x64 targets, clean account, low-end hardware, signing/quarantine, real login, registry PATH, OS registration, models, and paid providers remain **UNVERIFIED**. No product file, held target, user config/auth/PATH/registry/registration, package installation, external service, VCS state, or prior evidence was changed.

`execution_finished=true`; `runtime_ownership_released=true`; `owned_workers_remaining=0`; Q1/Q2/S1 confirmed, P3 follow-up pending.
