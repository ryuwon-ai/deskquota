# Native Task 4 independent QUALITY review

**Verdict: QUALITY FAIL — two Important (P2) issues require fixes.** SPEC had passed before this fresh quality review. This review covers the complete Native Task 3 → Task 4 delta, not only the seven-file SPEC correction. Product sources and held targets were read-only.

## Important issues

### Q1 — Refresh authenticated readiness after the user's client confirmation

- **Location:** `product/src/cli.rs:480–482` (status captured at line 418, confirmation at 434–447, write at 510).
- **Trigger:** During first setup's separate Pi client confirmation, the gateway is initially authenticated and running. Another command stops that same owned worker before the user answers yes.
- **Direct result:** The retained release binary exits setup successfully, writes `models.json`, marks the client journal `applied`, and prints client-patch success while authenticated status remains `stopped`. Gateway config bytes did not change. No upstream request or actual installed client execution was needed.
- **Cause:** `ready` uses the status captured before the unbounded human confirmation. The unchanged-config branch skips both activation and any new readiness check.
- **Impact:** Setup points the selected client at a stopped service while reporting completion. This violates the required desired-fingerprint readiness-before-client-write boundary.
- **Fix direction:** After the same in-memory plan/hash is confirmed and revalidated, obtain fresh authenticated status before deciding whether the existing lifecycle activation helpers are necessary. Confirm the desired fingerprint before writing the client. If readiness cannot be established, preserve the client as pending. Keep hash/snapshot refusal before activation. This is a missing check across a human interaction, not a claim that a worker can never stop after a final check.
- **Evidence:** `readiness_probe.py` / `readiness-result.json` reproduce the issue once. `readiness_identity_probe.py --stop` / `readiness-stop-result.json` repeat it with preserved structural fingerprint/config-hash fields. Both repro drivers intentionally exit 0 when the bug is observed; these are **confirmed failing product scenarios**, not product passes. The same identity probe without `--stop` is the positive control in `readiness-ready-control-result.json`.

### Q2 — Retain and revalidate provider ownership for prepared reconnect plans

- **Location:** `product/src/clients/mod.rs:337–343,356–359`; preparation-only guards are `clients/pi.rs:73–96` and `clients/codex.rs:51–74`.
- **Trigger:** Connect Pi or Codex to an existing private config, prepare a model-changing reconnect while its provider is owned, then disconnect using the same journal. Recreate the exact previously written config bytes before applying the retained reconnect plan.
- **Direct result:** A fresh `prepare` correctly refuses the recreated provider because its journal is retired. The already prepared plan nevertheless passes `validate_current_snapshot`, passes `apply_reviewed`, and overwrites the provider with the newly selected model. Separate retained Pi and Codex tests fail with exit 101. The fixture uses an existing client file, so this does not depend on the accepted changed-created-file preservation contract.
- **Cause:** Adapter ownership is checked only during `prepare`. A prepared plan retains Claude scope checks but no Pi/Codex provider-ownership guard. The generic snapshot/hash validates client bytes; it does not invalidate the plan when its journal ownership is retired. ConfigPatch subsequently loads a fresh journal and adopts the recreated object.
- **Impact:** A user can terminate the original connection and restore a saved config, yet an outstanding reviewed reconnect can still modify that now-unowned provider. It also passes the API intended to reject stale plans before gateway activation.
- **Fix direction:** Preserve the narrow ownership precondition with the prepared provider edit and validate it before activation and during application under the existing cooperating journal/resource lock boundary. Refuse a retired/conflicted ownership even if file bytes match the previous snapshot. Preserve legitimate same-journal reconnect; avoid a new generic fallback/ownership framework.
- **Evidence:** `quality_probes.rs`, `pi_retired_journal_after_preview_refuses_recreated_value.log`, `codex_retired_journal_after_preview_refuses_recreated_value.log`, and `rust-probes-execution.json`. Both tests print only booleans showing fresh-plan refusal versus retained-plan acceptance and changed client bytes.

## Strengths

- Native formats are separated into Pi, Claude, and Codex modules, with a shared reviewed-profile API. The narrow secret-object edit reuses ConfigPatch's existing parsing, protected storage, journal, locks, and restoration rather than adding a resolver/service/runtime dependency.
- Task 4 changes no Cargo dependency, HTTP forwarding policy, quota/accounting/fairness implementation, or lifecycle worker implementation. Route creation uses `SetupDraft::ensure_client_route` and existing config validation; it does not invent client-side capacity from the gateway reservation fallback.
- The installed-client drivers distinguish protocol selection, actual inference, typed tool identity/result/follow-up, gateway-off negative controls, and reload. Inference return codes are included in the tool-success conditions. Current evidence does not claim a catalog where only Responses inference was verified, nor actual Windows/Linux/performance support.
- Claude's corrected canonical scope check and its repeat checks remain in place. Normal private settings preserve unrelated env/header content; no blanket parent-alias ban was introduced.
- Fresh normal reconnect/disconnect controls pass for all three native profiles. The held implementation tests cover collisions, defaults, permissions, managed/process conflict, unsupported versions/protocols, wrong hashes, activation failure, Git scope, and retired journal behavior when preparing a new plan. Q2 identifies the missing retained-plan variant of that last boundary.

## Minor maintainability observation

**P3, non-blocking:** `product/src/cli.rs` grows from 379 to 1,011 lines (+632), including native location/version discovery, connection orchestration, selected-client prompts, and client status formatting. The adapter modules themselves are small (Pi 97, Codex 75, Claude 296 lines); the CLI is now the main concentration of responsibilities. Moving the existing native connect/location/prompt helpers into a focused CLI module would make the sequencing easier to review without introducing a new abstraction. The 1,186-line verification driver and 1,259-line profile test file also group three clients, but are development-only; no runtime size/performance claim follows from their length. This observation is not an additional acceptance blocker.

## Fresh execution and its limits

| Execution | Result | Meaning |
|---|---|---|
| Held release setup PTY, stop during client confirmation | Bug reproduced twice | Q1; second run captures unchanged config digest and the authenticated pre-confirm fingerprint |
| Held release setup PTY, running worker unchanged | Positive control passes | Exact fingerprint remains running, client applies, and owned worker cleanup succeeds |
| Fresh private Rust probes linked to held default rlib | 5 named tests: 3 passed, 2 failed | Two Q2 regressions fail; normal reconnect/restore and two bounded recovery fault checks pass |
| Held complete Rust log audit | 336 passed, 0 failed, 16 rows | Implementer execution evidence only; no reviewer full-suite rerun |

The reviewer compiled only `quality_probes.rs` with Rust 1.88.0 using the held default-feature rlib and its dependency directory. Output went exclusively to this review directory. No Cargo build, held target rebuild, default target mutation, installed-client wire run, paid/external API, keychain/login, model operation, installation, OS registration, or VCS operation was performed. The PTY used a synthetic version-only Pi executable and allowlisted HOME/USERPROFILE/XDG/native/temp environment; the real held gateway lifecycle/configuration code ran. All owned workers were authenticated-stopped, all PTY/test subprocesses reaped, and owned temporary fixtures removed. Only structural results are retained, not client config/token bytes or raw PTY prompts.

The existing installed Claude result belongs to the final held release below. Existing Pi/Codex wire evidence belongs to the prior integration-fix release SHA `c7258f5109882596b90d33d479187093aa2ef47dd42fb64e47e7395ddfe06355`; it was not rerun or promoted to fresh evidence. No benchmark, accounting matrix, Windows SDK/aws-lc cross-build, or other-OS runtime was repeated.

## ACL concern: still unexplained, bounded recovery observations only

The preserved `spec-fix/claude-scope-focused-final2.log` contains one macOS ACL-path `EINVAL` during disconnect after successful client apply assertions. Existing exacl 0.13.0 storage calls can surface this error; the precise failing syscall/stage and post-error fixture state remain unconfirmed. The original fixture Drop removed that state. Subsequent passes and this review's normal restore do not establish a fix or root cause.

Two fresh adjacent fault checks use deterministic temporary-directory write denial, **not injected exacl EINVAL**:

1. Denying journal-parent publication before disconnect's initial save returns an error and preserves both client and journal bytes. Restoring the owned directory permission permits successful retry.
2. Denying client-resource publication after restore's initial journal save preserves client bytes and records `RestorePartial`. Restoring the owned directory permission permits retry, removes managed token settings, and preserves unrelated settings.

Source review of `config_patch/restore.rs` shows per-resource errors record partial progress; writes before a later journal-save failure can be reconciled on retry using prior/current value hashes and created-file hashes. This does not establish which branch the unexplained event took or eliminate all platform filesystem failures. No retry/masking was added. The concern is retained separately from the two deterministic blockers.

## Frozen identities and preservation

`source-evidence-audit.json` verifies all 88 current source inputs against the held archive (89 archive entries including its manifest), with zero source/archive drift. The Task3→Task4 comparison has 19 changed/new paths: nine additions, ten modifications, no removals. `task4.diff` and `delta.json` retain this comparison; the Git-index fixture is binary data and its display in the text diff is lossy, while its source/archive hash is verified.

`before.json` and `after.json` preserve **1,151 files with zero drift**: the previous SPEC snapshot's 1,134 entries, the latest SPEC review files, and the parent fix audit. Parent-owned mutable research tracking/reports were excluded. Research/product have no `.git`; neither was initialized or used for Git commands.

- Final source manifest SHA-256: `ffe2eaae543808401e51105cd948f071d7f73170e3c4b86c9253b7221c9a498f`.
- Final source archive SHA-256: `a8282024113d4da65b07642df98f1a5b093fcc941689e40c158f13f1763a4e33`.
- Held release SHA-256: `4414957b603bc04d77cb55275d5ca335477645159c2509735ce84b3cd8013801`.
- Held default rlib SHA-256: `9e658ae3ac6e2dea276bd5d81371389bf1d424f1e2d4fbfbacf65174f215bca0`.
- Debug binary, lockfile, toolchain, corrected implementer entry/HOLD, and SPEC entry/HOLD also match their supplied identities in the audit.

**Assessment:** Not ready for Task4 quality acceptance. Fix Q1 and Q2, then rerun the retained counterexamples and their adjacent positive controls. The unroot-caused ACL event remains a disclosed limitation; these results do not mark performance, real OS readiness, or Task5 complete.

`execution_finished=true`; `FINAL_HOLD`; reviewer runtime/test ownership released for the same implementer's bounded fix. No product source was edited and no owned process or temporary fixture remains.
