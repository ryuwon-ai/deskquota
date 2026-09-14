# Native Task 6 S1 documentation-only SPEC rereview

Verdict: **SPEC PASS — S1 is resolved.** No remaining SPEC finding was established. Product and held files remained read-only. This result authorizes the separately dispatched fresh QUALITY review; it is not itself QUALITY acceptance.

## S1 resolution

`product/docs/installation.md:121-134` now tells the Windows user to stop on `replace_failed_recovery_unconfirmed` and keep the reported target, destination-local `.llmgw-backup-*`, and candidate. It explicitly discloses that `llmgw.exe` may be absent while the previous executable exists only at the backup path.

Restoration requires all relevant facts from the finding: record original/recovery errors and all paths; confirm the target remains absent and no installer/process is writing it; match the backup to the known previous trusted checksum or release identity; copy the verified previous binary back, or move only when another recovery copy remains; retain backup and candidate until checksum, version, and execution pass; never blindly overwrite an existing target; and preserve everything for support when identity or writer state is unknown. `product/docs/installation.md:116-119` continues to classify Windows execution, Authenticode, SmartScreen, and user-PATH behavior as unverified.

This is a documentation correction for the installer recovery contract. It does not establish Windows runtime correctness. `product/packaging/install.ps1` and the executable are byte-identical to final-v2 and were not rerun.

## Independent binding and preservation checks

| Check | Direct result |
|---|---|
| Source delta | 101 paths; exactly `product/docs/installation.md` changed; 0 added, 0 removed; all other source bytes unchanged |
| Source archive | 102 unique regular members, exact embedded manifest, every member matches live/new manifest; SHA-256 `b8b2843f9dfdbeb18ce4368af9609f9a087bc95d171cc9324d4f968ab26d383f` |
| Source manifest | SHA-256 `f228071555f45c1774b7c2f8e059f2adce6eef77c10674191e04713feb0d4c3f` |
| User package | exactly five unique regular members; all four docs equal new live source; packaged binary equals final-v2 `cf7c436c442c426a6e9a1d485a5261fe5e5a05c8b131b130e679e00eec2c646b`; package SHA-256 `ba03203e9416a4eb75ca972fb0b3e5eba8618c46b2c26c7f65e2b7ffff387a22` |
| Doc-fix HOLD | 12 rows including `FINAL_HOLD.json`; hold SHA-256 `608022e67da4d18ce08e140faf4e9cee79273a636a1d422f00cb666eecfdd3f3`; 0 drift |
| Prior preservation | 1,646 rows, including all 17 files from the original SPEC review and its `FINAL_HOLD.json`; 0 drift |
| Before/after | all source/archive/package/HOLD/prior/root-check values equal; no product mutation |

`contract-assessment.json` confirms all 12 S1 clauses and exact live/package document bytes. The first `contract-audit.json` remains preserved with `passed=false`: its reviewer literals accidentally included a `+` after encoded line breaks. The assessment records this reviewer error and uses whitespace-normalized complete phrases; no product result was overwritten or reconstructed.

An initial read-only `shasum` command also assumed a standalone `spec-doc-fix/llmgw-macos-arm64` file, which this doc-only phase intentionally does not contain. It failed after hashing the four requested existing artifacts and wrote nothing. `rereview_probe.py` then read the binary from the package and compared it directly with final-v2. The parent's earlier wrong-base read-only audit is separately disclosed in `evidence/native-task6-spec-doc-parent-check.json`; neither failed check lost or modified an artifact.

No Cargo/code/test/client/native measurement was rerun because only documentation changed. The previous wrapper's raw false aggregate and corrected assessment remain preserved within the 1,646-row baseline. Actual PowerShell/Windows, Windows/Linux/macOS-x64 targets, clean account, low-end hardware, signing/quarantine, real login, registry PATH, and OS registration remain **UNVERIFIED**.

`execution_finished=true`; `runtime_ownership_released=true`; `owned_workers_remaining=0`.
