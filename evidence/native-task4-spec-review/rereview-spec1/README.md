# Native Task 4 SPEC-1 rereview

**Verdict: SPEC PASS.** SPEC-1 is closed against the new held source and rlib. No new actionable SPEC finding was established. This is the same SPEC reviewer's bounded rereview, not a QUALITY review or Task 5 acceptance.

## What was corrected

`product/src/clients/claude.rs:61–67` now validates the native target before preparing a client patch. `claude.rs:100–106` uses the existing ConfigPatch canonicalizer, looks for an actual repository ancestor, and requires a repository target to be untracked and ignored. The fixed implementation does not turn the earlier rejected blanket parent-symlink hypothesis into a restriction: ordinary non-repository native paths and safe canonical parent aliases remain supported.

`product/src/clients/mod.rs:337–343,356–371` retains and rechecks the specific Claude scope before client apply and during the pre-activation snapshot validation. The existing CLI invokes the latter before its gateway-activation branches. Git directory/index/config redirection variables are removed by `claude.rs:203–228`.

## Fresh independent execution

The reviewer compiled a private test executable from `scope_probes.rs` against the **held new debug rlib**, SHA-256 `9e658ae3ac6e2dea276bd5d81371389bf1d424f1e2d4fbfbacf65174f215bca0`. All compilation outputs and new evidence are under this rereview directory. No Cargo build or held target mutation was performed.

Eight separately executed, named tests passed (each actual test-process exit 0), recorded in `execution.json` and their individual logs:

| Check | Direct observation |
|---|---|
| Original shared native directory blocker | The original retained test now refuses a tracked project `.claude/settings.json`; the token write branch is not reached. |
| Normal private Claude native profile | Unrelated env/custom header survives apply; public placeholder is present; disconnect restores the private settings. |
| Composite hash and snapshot boundary | Wrong hash is refused, changing the desired gateway fingerprint changes the hash, and changed client bytes invalidate the reviewed snapshot. |
| Pi and Codex ownership boundary | Same-journal reconnect succeeds; user-changed provider is refused and preserved; a retired/conflicted journal cannot re-adopt a recreated old provider. |
| Git tracking changes after preview | Adding the previously ignored file to the actual Git index causes both pre-activation validation and client apply to fail; client bytes and journal remain untouched. |
| Safe canonical alias into private ignored repository target | Preview, current-snapshot validation, and apply succeed through the alias. |
| Ignore status changes after preview | Removing the ignore rule causes both pre-activation validation and client apply to fail; client bytes and journal remain untouched. |
| Inherited literal-pathspec environment | A tracked and ignored native target remains refused with `GIT_LITERAL_PATHSPECS=1`; the potential tracking-check bypass was not reproduced. |

The old `project_settings_parent_symlink_must_be_refused` function remains in the copied historical source but was **not executed** and is still a rejected review hypothesis. The corrected new-file preservation expectation remains accepted. No original failed evidence was overwritten.

All test subprocesses used allowlisted HOME/USERPROFILE/XDG/temp environments. Git init/add operations occurred only in newly owned temporary fixtures. The fresh checks did not execute an actual installed client, gateway CLI/worker, real API, user configuration, auth/keychain/login, model server, installation, or OS registration. The CLI pre-activation call order is source evidence; the implementer's held CLI fixture is not relabeled as a reviewer execution.

## Source, build evidence, and preservation

`source-and-evidence-audit.json` independently records:

- 88 current source files match the new held archive, with zero archive drift.
- Exactly seven files changed from the prior integration hold. Pi/Codex differences only supply the private adapter return alias and empty scope-check vector; ConfigPatch only exposes its existing canonicalizer within the crate. No dependency manifest, gateway policy, fairness/accounting, lifecycle, or hot-path source changed.
- New release/debug/rlib, Cargo.lock, corrected README-v2, and final-hold-v2 hashes match the supplied identities.
- The preserved full-test log contains 16 successful result rows totaling 336 passed and zero failed. This is a log audit, **not a fresh reviewer full-suite run**.

The implementer's new Claude actual-client evidence is bound to the new release SHA `4414957b603bc04d77cb55275d5ca335477645159c2509735ce84b3cd8013801`. Pi and Codex actual-client evidence remains the prior immutable binary's evidence, as explicitly disclosed. No full suite, installed client matrix, historical performance/accounting run, or Windows/Linux cross-build was rerun for this rereview. No new latency/RSS/other-OS claim follows.

`before.json` and `after.json` preserve 1,134 entries with zero drift: the original 978 non-live-source entries, the replacement current 88 source entries, original-review/fix-phase evidence supplied by the parent, and the new held binaries. Original source was legitimately replaced by the approved fix; its earlier immutable archive remains protected. Parent-owned mutable tracking/reports were excluded.

## Unresolved concern retained for QUALITY

`product/artifacts/native-task4/spec-fix/claude-scope-focused-final2.log` preserves one macOS `EINVAL` on the protected Claude journal during disconnect, after client apply assertions passed. The error surfaced through the existing exacl 0.13.0 ConfigPatch ACL path. The precise lower-level cause remains **unconfirmed**. The failed fixture's Drop removed its temporary tree, so state after the error was not inspected.

The implementer's later isolated, 20-repeat, focused, and full-suite successes do not establish a fix or root cause. This rereview's normal private restoration test passed once, and no retry/masking was added, but this does **not** resolve the earlier concern. Carry it unchanged into QUALITY review. No independent deterministic SPEC violation was established from that single preserved event.

`execution_finished=true`; `FINAL_HOLD`; product writer/runtime/test ownership released. No owned process or temporary fixture remains. Review source, binaries, logs, and results are retained as evidence.
