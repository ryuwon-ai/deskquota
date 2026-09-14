# Native Task 6 independent SPEC review fallback

Verdict: **SPEC CHANGES REQUESTED — one P2 documentation finding.** Do not start QUALITY until the Windows installer recovery contract is documented and a fresh SPEC review passes. The unavailable Windows/Linux/clean-account/low-end/signing/login environments remain disclosed acceptance gaps; they were not converted into findings or fabricated passes.

The product, frozen source, held/default targets, prior evidence, user profiles, PATH registry, client configuration, credentials, keychain, login registration, and external providers remained read-only. This review wrote only this fresh evidence directory and owned temporary fixtures. No Cargo command, package installation, Git operation, real API/model/cloud/paid call, public release, registry write, profile edit, autostart mutation, account creation, or Trash scan occurred.

## Finding

### S1 — P2: document the Windows installer state where the original exists only in backup

`product/packaging/install.ps1:104-135` correctly preserves recovery material on an unconfirmed replacement failure. The recovery branch at lines 111–117 can remove `llmgw.exe` and then fail while moving the destination-local `.llmgw-backup-*` file back. In that state the original executable may exist only at the reported backup path, with the target absent; a candidate may also remain. The script keeps `preserveRecovery=true` and reports the paths.

The Windows instructions at `product/docs/installation.md:98-125` say only that the ZIP is verified before replacement. They do not describe `replace_failed_recovery_unconfirmed`, the target-absent/backup-only state, or what the user should preserve and restore. The nearby runtime text about `ReplaceFileW` is in the setup/config replacement section and does not document the packaging installer.

Add a Windows-specific recovery paragraph telling the user to stop and retain the reported target, backup, and candidate paths; explain that the previous executable may be only at `.llmgw-backup-*` and how to restore it without deleting either recovery copy. Keep this as a static contract until it is exercised on Windows. This is P2 because a failed reinstall can leave the command unavailable even though the old bytes are preserved.

## Fresh direct verification

| Scope | Result |
|---|---|
| Frozen source and archive | 101 live source files and 102 source-archive members, including exact embedded manifest; 0 drift; manifest `19818234…95e33`, archive `0b9e0a9a…a3c9` |
| Package and executable | exact 5 regular members, 4,202,300 B archive `902aa7e5…e902`, 10,011,264 B executable `cf7c436c…646b`; package docs equal frozen source |
| Prior/HOLD preservation | 44 Task6 HOLD rows, 3 final-build extras, and 1,578 prior baseline rows; before/after equal with 0 drift; `FINAL_HOLD.json` `ff1a9d56…d68c` |
| POSIX installer | exact held package local install and reinstall, hostile quoted PATH preview, unchanged profile, restricted-PATH absence, exact printed line in a new child shell, and `llmgw 0.1.0` all passed |
| Failure preservation | wrong hash, missing archive, traversal, symlink, duplicate, and unexpected entry all failed before replacement and preserved the executable; fake loopback HTTP release installed the exact held bytes |
| Installed clients | Pi 0.84.2 / Chat Completions SSE, Claude Code 2.1.63 / Messages SSE, Codex 0.154.0 / Responses with WebSocket disabled all used the same held binary; selection, exact tool result plus follow-up, gateway-off with 0 upstream attempts, disconnect, unrelated-edit preservation, and cleanup passed |
| Listing distinction | Pi verified; Claude not requested/unsupported for the fixed profile; Codex unverified because the gateway list is not a Codex catalog |
| Fresh native observation | two save-only wizard invocations, zero client-version children; direct PID sample 1 at 2,473,984 B and separate command-tree peak 8,896,512 B; idle 10/10 samples at 10,010,624 B; three raw on/off cycles retained; authenticated stopped and temp-home cleanup passed |
| Mach-O linkage | root read-only check `evidence/native-task6-linkage-check.json` (`f265b402…45ae`) found an arm64 Mach-O linked only to listed macOS system libraries; this is not clean-account, signing, or quarantine proof |

The held final logs were inspected rather than rerun: final source followed fmt, locked all-target check, locked all-target Clippy with warnings denied, locked full Rust tests, and locked release build; the retained results report Rust 365 and Python 64 passes. The review did not repeat the unchanged core/accounting suites or any quota matrix. The 15-file Task6 delta contains one runtime Rust edit, the first-run non-TTY diagnostic string; source inspection found no HTTP/SSE/quota/fairness watcher, discovery, credential, telemetry, or repeated client-version loop.

The first reviewer wrapper saved all four successful command results but marked its aggregate false because `runtime-native.json` intentionally has no top-level `passed` field. `runtime-commands.json` remains unchanged as the raw review failure. `runtime-assessment.json` uses the measure driver's preserved stdout verdict plus the JSON completion/cleanup fields and passes all four executions. No product result was reconstructed or overwritten.

## Evidence and limits

- `identity-before.json`, `identity-after.json`: source/archive/package/HOLD/prior preservation and exact equality
- `installer-probe.json`: independent held-package install and failure cases
- `runtime-{pi,claude,codex,native}.json`: fresh same-binary runtime outputs
- `runtime-commands.json`: preserved wrapper misclassification and raw exits/stdout/stderr
- `runtime-assessment.json`: explicit schema-aware follow-up assessment
- `findings.json`: actionable finding and correction
- `cleanup.json`: runtime ownership and fixture cleanup

Actual PowerShell/Windows execution, Windows x64/arm64, Linux x64/arm64, macOS x64, Windows PATH registry behavior, clean native account, low-end hardware, code signing/notarization/quarantine, real human login, and actual OS registration remain **UNVERIFIED**. A source/static Windows check is not runtime proof. The two setup memory values are separate invocations with different scopes; the one direct sample is not a peak. These observations do not establish performance superiority.

`execution_finished=true`; `runtime_ownership_released=true`; `FINAL_HOLD` remains in force for the documentation fix and fresh SPEC rereview.
