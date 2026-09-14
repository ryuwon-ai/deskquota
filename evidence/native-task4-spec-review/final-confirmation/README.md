# Native Task 4 final same-SPEC confirmation

**Verdict: SPEC PASS on the final QUALITY Q1/Q2-fixed source.** The original SPEC-1 fix remains effective, the Q1/Q2 changes satisfy the approved Task 4 preconditions, and no new actionable SPEC finding was established. This final confirmation supersedes the earlier SPEC verdict only for the new held source identity. It does not execute or accept Task 5.

## Fresh independent execution

The same SPEC reviewer compiled `final_spec_probes.rs` against the held final debug rlib using Rust 1.88.0. `execution.json` records the exact compile/test commands, compilation exit 0, and eight separately executed named tests with **8 passed, 0 failed**. Each test process used its own allowlisted temporary HOME/USERPROFILE/XDG/temp environment. All outputs went only to this final-confirmation directory.

| Fresh named check | Direct observation |
|---|---|
| Original SPEC-1 shared native target | Git-tracked project `.claude/settings.json` remains refused. |
| Private Claude preservation/restoration | Unrelated env/custom header survives; reviewed public placeholder is written; disconnect restores the private settings. |
| Composite hash/current snapshot | Wrong hash fails, changed desired gateway fingerprint changes the hash, changed client bytes invalidate the snapshot. |
| Active Pi/Codex provider ownership | Same-journal reconnect succeeds; user-changed provider is refused/preserved; retired ownership cannot be freshly re-adopted. |
| Git tracking changes after preview | Pre-activation validation and apply both refuse; client bytes and journal stay unchanged. |
| Safe native canonical parent alias | Ignored/untracked repository target continues to support preview, validation, and apply. |
| Git ignore changes after preview | Removing the ignore rule makes both pre-activation validation and apply refuse with no client/journal write. |
| Retained Pi/Codex reconnect after retirement | A retained plan changing the model refuses after disconnect and exact old-byte recreation; both recreated client bytes and retired journal bytes remain unchanged. The one named test covers both clients. |

No fresh test failed in this confirmation. Historical failed probes and rejected review assumptions remain preserved. In particular, a blanket ban on safe parent-directory symlinks is still not part of the contract, and changed newly created files may correctly be reported through `preserved_created_files` instead of key conflicts.

## Source inspection of the final delta

Exactly ten existing files differ from the accepted SPEC-fix hold, with no additions/removals or dependency changes. The source/archive audit is in `source-archive-evidence-audit.json`.

- **SPEC-1 retained:** the Claude adapter's actual canonical target, ancestor repository detection, tracking/ignore checks, and native/project private-target checks are unchanged except for the private `ScopeCheck` → `PlanCheck` name. Normal native directories and safe aliases remain supported.
- **Q1:** `product/src/cli.rs:431,456–469` retains the reviewed runtime action and reads fresh authenticated status after confirmation. If the required action changed, client application is refused and a new review is required. `cli.rs:495–535` requires the desired fingerprint to be ready through that fresh status or the existing expected-fingerprint activation helpers. Config-changing setup also gets a fresh post-apply status check. The exact in-memory client plan/hash remains retained; no new implicit approval or second config writer was introduced.
- **Q2:** `product/src/clients/mod.rs:363–377,391–394` retains active ownership prerequisites for Pi/Codex dedicated-provider reconnects and validates them before activation/application. `product/src/config_patch/apply.rs:85–99,131–149` checks them while holding the existing journal transaction lock and resource lock, before a retired journal may be reset or an applying journal published. The transaction lock remains held through apply, so a cooperating same-journal disconnect cannot retire ownership between this check and the writes. Existing expected-byte checks and per-resource write locking remain in place.
- `OwnedKeyRequirement` and the adapter checks are internal, narrow preconditions for the two existing dedicated providers. They do not add a public journal schema flag, generic sensitive-object framework, service/watcher, compatibility layer, dependency, or protocol conversion. Core route limit, fairness/accounting, HTTP/upstream hot path, lifecycle helpers, and installed runtime requirements are unchanged.
- Client compatibility documentation describes post-confirmation status and retired-plan refusal without changing the accepted scope or overstating client network control.

## Held evidence audited, not re-executed here

The existing independent QUALITY rereview is source- and artifact-backed evidence, not a fresh SPEC execution:

- Its stop-during-confirmation PTY case observes product exit 1, stopped worker, no client config/journal, unchanged gateway digest, and no applied report. Its healthy control observes product exit 0, client applied, and the exact reviewed/config fingerprint still authenticated. Both reports match the final release hash.
- Its seven named Rust tests cover retired Pi/Codex plans, normal reconnect, two permission-failure/retry boundaries, explicit locking, and 16 cooperating races. All 16 observed races were disconnect-winning; neither that evidence nor this confirmation claims exhaustive schedules. We did not rerun these locks/races or PTYs because the fresh focused checks and inspected delta did not establish a new concern.
- The held implementer full-test log has **339 passed, zero failed across 16 result rows**; the held release client-profile log has **24 passed, zero failed**; the Python log records **7 tests, OK**. These logs were audited, not rerun.
- All three held installed-client reports have `passed=true` and bind to final release SHA `cc15eb4172148a681faba2d4e2932e94a4853fc303de61d234b7a016c05b6a90`. Pi listing remains verified; Claude listing is not requested and its installed-version discovery remains unsupported; Codex catalog listing remains unverified. All off controls record upstream zero separately, with Claude/Codex bounded timeouts explicitly retained.

No fresh gateway CLI/worker, actual installed-client, full suite, performance/accounting matrix, Windows/Linux cross-build, or platform installation was executed by this final SPEC confirmation. Actual installed-client evidence is macOS-only. No latency/RSS/other-OS claim follows from the final binary size or these test counts.

## Identities, preservation, and cleanup

- Current source manifest: 88 files, SHA `69fe932ae90f46abf65305be8fd0197d4c4a0e6bdc68cceb6ff63b38406ed20b`.
- Source archive: 89 members including the embedded manifest, SHA `85d184adc9ad10e859a6988436a843bbd19ce41a250c743413d2466be5a12d3c`; all 88 source members match current source and manifest.
- Held default rlib: SHA `8651b48f5c771b351a33e266bf6ea9116eb45267879e6ca8137c4f5ae33bd632`.
- Final implementation and QUALITY README/HOLD/after identities match their supplied hashes. Cargo.lock is unchanged.
- `before.json` and `after.json` preserve **1,245 files with zero drift**, including the latest 1,221-entry QUALITY baseline, parent preservation entries, and the latest QUALITY rereview outputs. Parent-owned mutable tracking/reports are excluded.
- Product source and all held binaries/targets/artifacts were read-only. Compile outputs exist only here. Fixture-only Git init/add was used for scope assertions; no user/repository VCS mutation occurred.
- Test processes exited, temporary fixtures were reaped, and no owned process or scratch path remains. No actual user config/auth/credential/keychain/login, real API, model operation, package installation, or OS registration was accessed or changed.

## Concern retained without a resolution claim

The earlier single macOS protected-journal ACL-path `EINVAL` remains disclosed. Its precise lower-level cause, exact phase, and original post-error state are unconfirmed; the failed fixture cleanup removed that state. Later normal and permission-fault recovery passes, including this confirmation's normal restoration pass, do not reproduce or explain that event. No retry/masking fix was introduced. This retained platform observation is not relabeled as resolved by the SPEC PASS.

`execution_finished=true`; `FINAL_HOLD`; product writer/runtime/test ownership released. Final same-SPEC Task 4 confirmation is complete. Task 5 has not started in this review.
