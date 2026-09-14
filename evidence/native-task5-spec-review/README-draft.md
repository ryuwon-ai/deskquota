# Native Task 5 independent SPEC review

Verdict: **CHANGES_REQUESTED**. Seven actionable software/specification findings remain. The unavailable temporary OS account and human login cycle are a separate, disclosed acceptance gap; that external gap is not itself a software finding.

Product source and held artifacts were read-only. This reviewer exclusively owned the bounded product runtime/tests during this review. All new outputs are under `research/evidence/native-task5-spec-review`. No stock `verify_native.py --templates-only` invocation, real `--exercise-user-service` stage, actual OS registration, account creation, login, installed-client run, package installation, reference-clone edit, Cargo build, or Git operation occurred.

## Findings

### S1 — P2: Scope the Windows logon trigger to the actual current user

`product/src/autostart/mod.rs:916-917` emits `<LogonTrigger><Enabled>true</Enabled></LogonTrigger>` without `UserId`. `Principal id="CurrentUser"` is a schema reference name, not the trigger's user selector. The official [LogonTrigger.UserId documentation](https://learn.microsoft.com/en-us/windows/win32/taskschd/logontrigger-userid) says a null user identifier triggers on any user's login. This violates the approved current-user login contract and can start the gateway when another user logs in while its principal still has an interactive session.

Fresh held-rlib rendering and the independent `windows_logon_trigger_is_scoped_to_current_user` assertion fail (exit 101; generated XML retained). Resolve the actual current-user identity, bind the trigger explicitly, and include that identity in the reviewed plan/hash. Existing `windows-sys` features and native token/SID handling in `lifecycle/platform/windows.rs:55-106` already provide a maintained dependency and relevant native pattern; no new identity resolver service is required. Actual Windows task execution was not tested.

### S2 — P2: Preserve unknown next-login authentication as unknown

`product/src/autostart/mod.rs:49-53` returns `ConfiguredButUnavailable` for every `Auth::Env`, without any login-environment observation; `query_current` and setup/doctor print it as the availability result. The adjacent `current_shell_auth_is_login_proof=false` does not turn an asserted absence into an unknown state. The binding design explicitly distinguishes unknown from observed absence/success.

The independent held-rlib `unobserved_login_env_is_unknown` assertion fails. A fresh held-release doctor fixture had its synthetic terminal auth present, had never observed the login environment, and nevertheless reported `configured_but_unavailable`. Represent this as unknown/unverified. Keep the observed empty-environment run's missing-auth result separately classified as configured-but-unavailable. This requires no credential copying or discovery service.

### S3 — P1: Keep templates-only away from real OS registration on every platform

`product/scripts/verify_native.py:195-207` unconditionally calls `apply_autostart(..., "on")` under only a temporary HOME. There is no platform guard or injected file-only registration backend. On Windows the Rust action reaches `schtasks /Create` (`src/autostart/mod.rs:734-741`), whose scheduler is not isolated by HOME/USERPROFILE. On Linux it reaches `systemctl --user enable` (`:710-713`); changing HOME is not a user-manager isolation boundary. This contradicts the templates-only/read-only-validator contract. The record still hardcodes `actual_user_registration_changed=false`.

Fresh mock-only dispatch probes for `sys.platform=linux` and `win32` both reached `apply_autostart` and were stopped before mutation. No actual cross-platform registration was attempted. Split safe template/owned-fixture checks from explicitly authorized manager exercise; use render-only and read-only validators or injected manager fixtures off macOS. Available Linux validator detection currently never leads to execution. A successful macOS temporary LaunchAgents file fixture does not establish this driver's safety on Windows/Linux.

### S4 — P2: Give verifier builds an independent, caller-controlled target

`product/scripts/verify_native.py:173-175` overwrites `CARGO_TARGET_DIR` with the held `product/target/native-task5` even when the caller supplies an external target or a specific held binary. Thus repeating a seemingly bounded verification can rebuild the frozen runtime/test target and destroys its reproducibility boundary. A mock subprocess probe confirmed the explicit external target is ignored. The stock driver was deliberately not executed.

Honor an explicit unique build target, or separate the Rust contract-test build from a binary-only fixture invocation. Report the exact source/test/binary identities and preserve previous output paths; do not silently rebuild the held target. This is a verifier/reproducibility defect, not an additional product runtime dependency.

### S5 — P2: Do not run the manual terminal-auth stage with the empty-login fixture environment

`product/scripts/verify_native.py:112-116` routes all CLI calls through `safe_child_env`; `manual_stage` uses that same route for manual on/off at `:357-371`. It discards the user's terminal auth reference and native OS environment, then labels the on result `terminal_auth_availability`. A correctly configured terminal therefore fails the stage for an artifact of the harness, so the required terminal-versus-login comparison cannot be performed. The function also returns exit 0 for a failed on/stopped result (`:398`).

Fresh mock-only probes confirm parent synthetic auth is absent from the child environment and a manual-cycle with `on_exit=1`, `running=stopped` returns driver exit 0. No real manual service stage was executed. Use distinct explicit environments for the human terminal stage and empty-login fixture; preserve required native Windows environment without recording secrets. Classify failed manual lifecycle checks as failures rather than successful completion. Keep actual login observations gated by the human attestation and timestamp.

### S6 — P2: Derive registration state from the authoritative manager, including failures and other scopes

`product/src/autostart/mod.rs:544-555` makes a missing local registration sidecar definitively `disabled` regardless of manager result. On Windows, a task may still exist after its XML sidecar is removed; `/Query` success becomes manager `enabled` while registration remains `disabled`. Any `/Query` failure with a missing sidecar is silently mapped to `task_absent` with no reason (`:631-634`), including policy errors. `/Query` success also does not distinguish a disabled scheduled task. `remove` returns success before querying when the sidecar is absent (`:296-300`). On Linux the missing-local-file path skips the manager entirely (`:594-595`), concealing a still-enabled global/org unit of the same label, including after a user-scope disable.

These are source-supported deterministic branches, not fresh Windows/Linux executions. Reconcile the actual manager definition/state and ownership with local metadata, retain unknown/blocked reasons on query errors, and report foreign/global influence without changing those registrations. Preview/hash and apply must use the same authoritative snapshot, so local XML alone cannot approve a changed scheduler task. The approved contract explicitly requires not silently treating command failure as disabled/healthy and preserving global/org/foreign registrations.

### S7 — P2: Account for Task Scheduler environment expansion in literal percent paths

`product/src/autostart/mod.rs:882-904` only applies Windows command-line quote/backslash escaping. `windows_definition` passes the result and executable path directly to `Exec` XML (`:928-940`). A literal valid directory component such as `%USERNAME%` stays unescaped. Windows Task Scheduler supports environment variables in executable Path, Arguments, and WorkingDirectory, according to the official [ExecAction documentation](https://learn.microsoft.com/en-us/windows/win32/taskschd/execaction). XML escaping and ordinary argv quoting do not establish literal delivery through that expansion layer. The existing test explicitly expects literal `%PATH%` unchanged in XML (`tests/autostart_templates.rs:60-85`), so it misses this semantic layer.

This is source/document-supported; actual Windows expansion was not executed here. Meet the approved literal-percent contract with a native approach backed by an actual argument-delivery test on Windows, or explicitly surface the unsupported path as blocked while it remains unresolved. Do not claim the current string containment test proves literal `%...%` execution. No shell/VBS wrapper is proposed.

## Fresh evidence

| Check | Result | Evidence and meaning |
|---|---|---|
| Before/after preserved available file set | 1,346 files, 0 drift | `before.json`, `after.json`; includes current 92 source files and available Task5/earlier artifacts |
| Archive content | 93 members, 0 drift | 92 source members plus embedded manifest, byte hashes checked |
| Independent Rust probe against exact held default debug rlib | 3 passed, 2 failed; exit 101 | `spec_probe.rs`, `compile.log`, `spec-probe.log`; no Cargo or held-target rebuild |
| Held-release owned macOS fixture | 15 explicit acceptance assertions passed | `runtime_probe.py`, `runtime-probe.json`; exact release SHA below |
| Driver boundary counterexamples | 5 positive reproductions of defects; mocked subprocess only | `driver_probes.py`, `driver-probes.json`; not five product acceptance passes |

The fresh macOS fixture observed stopped→register with zero owned worker, parsed exact ProgramArguments with Hangul/space/XML/$/%/quote/backslash config path, validated plist using `/usr/bin/plutil`, rejected a wrong preview hash without mutation, started manually, unregistered while preserving the same authenticated nonce, and stopped through authenticated `off`. An empty-login environment failed for missing synthetic env auth, and registration contained neither the synthetic auth value nor env-name reference. Both owned configurations were authenticated-off and observed stopped before removing the owned fixture tree. No synthetic auth value was written to result logs.

The 350 existing Rust passes, 11 release template passes, and 5 Python passes are held implementer evidence, not fresh reviewer executions. The earlier installed Pi/Claude/Codex Task4 probes used the older `cc15…` release and were not repeated or attributed to this `f3f64…` release.

## Source-supported conforming scope

- Task4→Task5 diff: 4 added source files, 7 changed, 0 removed; unchanged Cargo.lock and dependency manifest. No HTTP/SSE/quota/accounting/fairness implementation change.
- OS manager target is foreground `--config ABS run`; existing lifetime lock and authenticated identity remain in the worker.
- macOS install/remove is file-only, with no bootstrap/bootout; Linux enable/disable has no `--now`; Windows no Run/End and no automatic restart or supervisor fallback. Official [schtasks delete](https://learn.microsoft.com/en-us/windows-server/administration/windows-commands/schtasks-delete) supports preserving an already-running program when deleting its task; this is not Windows nonce execution evidence.
- The CLI composes autostart status in one-shot status/doctor, outside `lifecycle::status` and its 25ms readiness loop. No manager subprocess added to that loop or request hot path.
- Setup saves core config then confirms and applies the same in-memory RegistrationPlan. Cancel/block reports core/manual availability; no generic transaction abstraction added. The separate stage may run after SaveOnly, but only its own concrete confirmation can apply registration.
- Windows settings include InteractiveToken, LeastPrivilege, IgnoreNew, PT0S, false battery/idle/network restrictions and WakeToRun=false. Hidden=false makes no console-hiding claim. Linux template uses native `:` to suppress `$` expansion, doubles `%`, uses absolute ExecStart and Restart=no/default.target. Real platform argument delivery remains unverified.

## Preservation, identities, and limitations

Release: `product/target/native-task5/release/llmgw`, 10,084,768 B, SHA-256 `f3f64a801a629ef9ddcd2309e95b233a4fa70e6b78daf32ec3099c0454571b59`.

Debug: `0e5b74e670c1c318c400d57869cc4e9696b50d55a5986ec537a77ced8f5a230b`; exact linked default rlib: `263c4a82af3e259f2178bf08b374214c58717d6f9d9cd168bc082d6d1f596b03`. Source manifest: `560063431970bc5d3c48a77e31b8b140e6563e3508a53c20bca4eb808f224b42`; archive: `e107342c1692a05703e89fb978f0fcdd42822700d034520182b6a895dabb86c1`; Cargo.lock: `c82e9357a135eb84f0d1be70d0a5f16d2a6c0f7a520cc140562306418a0d3e1c`.

All **available** 75 Task5 phase artifact identities in the parent preserve list were retained, including rejected v1/failed-v2 metadata. Two earlier diagnostic logs remain unavailable at their original/exact known Trash paths per the fixed handoff; this review did not browse Trash, recover/reconstruct them, or claim all historical Task5 artifacts were preserved. Prior synthetic-literal observations do not prove unavailable current contents. No new failure log was moved, deleted, or sanitized. The earlier one-off Mac ACL-path EINVAL remains unresolved at the exact syscall/phase/post-state, was not retried here, and is not claimed fixed.

Actual user registration, real Windows/Linux manager execution, human next-login/on/off acceptance, Windows console behavior, and end-user auth provisioning remain **UNVERIFIED**. No support-complete or CI/CD-safe claim follows from this bounded review. Source/runtime fixes are requested; external real-OS acceptance remains unfinished separately.

`execution_finished=true`; `release_ownership=released_for_same_implementer_spec_fixes_then_fresh_spec_and_quality_review`.
