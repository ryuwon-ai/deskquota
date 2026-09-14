# Native Task 1 fresh SPEC review

Verdict: **SPEC FAIL — one directly reproduced P2 contract miss.** FINAL HOLD remains in force. No product source, documentation, fixture, binary, prior evidence, or reference clone was changed; no build, package install, Git operation, external provider/model call, client configuration change, or OS registration was performed.

Review target: final 61-file source hold, not the earlier pre-fix candidate. Source was compared with accepted Core Task 8 archive and all lifecycle modules and contract tests were read. Parent-authorized state-path, operation-lock, drain listener, Windows unsafe scope, ACL and child exit-code decisions are accepted rather than reopened.

## Blocking finding

**P2 — Report `restart_required` for a changed invalid config while an authenticated worker is running.** `product/src/lifecycle/mod.rs:108-112` parses/validates the disk configuration before reaching the worker-lock and fingerprint branch at lines 116–120. Start a valid temporary config, replace its bytes with `invalid = [`, then run `on`: the held release exits 1 with only `invalid configuration syntax or shape`, whereas `status --json` correctly reports the existing running worker and `pending_restart=true`. The Native Task 1 third checklist item and native setup spec §2 prescribe changed fingerprint → `restart_required`/exit1 without limiting this to valid edits. This can mislead lifecycle callers into treating an already running, immutable old configuration as a failed start instead of a pending change. The existing worker is preserved, so this is not a process-safety finding.

Evidence class: **directly verified** by `probe.py`, with exact outputs in `probe-results.json` (`invalid_disk_on`). A valid changed comment yields `restart_required` in the same probe, isolating parse-before-identity ordering as the trigger. Required scope: resolve the canonical instance and check authenticated running identity/current bytes before validating a fresh launch; preserve validation-before-disruption for `restart`. Add a regression for malformed edited configuration with a running worker. For new/stopped instances, validate configuration before creating token files; for restart, preserve new-config and credential validation before any old-worker stop. No config fallback or migration is needed.

## Task 1 requirement accounting

| Requirement | Evidence and result |
| --- | --- |
| Lifecycle files and temp contract suite | Source present; no setup/default config/client/autostart/installer success claims. |
| Behavioral RED before implementation | Read preserved `artifacts/native-task1/red.log`: six actual assertion failures, zero passes. Read preserved stale-log RED: expected new failure was incorrectly reported `port_in_use`; final implementation uses child status. These are prior execution evidence, not tests rerun by reviewer. |
| Concurrent two on, repeated on/off, same fingerprint | Independent actual release CLI: two concurrent on exit0, one observed identity; idempotent off/stopped; existing suite further checks nonce. |
| Different fingerprint / immutable worker | Valid changed bytes produce restart_required; invalid changed bytes miss required diagnostic (finding above). Status pending_restart and rejected invalid restart preserve old nonce. |
| Canonical path + nonce + token, bound address/content fingerprint handshake | Direct corruption of each path hash, fingerprint, nonce, bound address, PID and start-time field rejects off. Original worker and unrelated owned helper stay alive. Conditional stale nonce receives409. |
| Worker lifetime file lock | Source uses fs4 handle ownership held through worker shutdown and record write; stable files are not unlinked. Independent concurrent-start and stale-PID observations support behavior. |
| Separate command operation lock | Approved implementation; serializes on/off/restart, no supervisor added. |
| Stale PID / unknown identity / no PID-only kill | Actual stale record with owned unrelated helper PID: off0, on0, unrelated helper alive; all active identity corruptions fail closed. No product kill/signal-by-PID path exists. |
| Native detached spawn | Source uses current_exe + argv, null stdio, Unix setsid before runtime, Windows creation flags. Prior Darwin session artifact records SID=PGID=PID. Windows runtime remains unverified. |
| Five-second readiness | Source READY=5s and authenticated status bounded with timeout_at. No upstream call constitutes readiness. No independent delayed-start timeout counterexample executed. |
| off drain10s + confirmation2s | Source STOP=12s plus fixed server drain10s. Focused held release lifecycle test was executed by reviewer (see drain-test.log): observable draining, closed upstream/downstream, stopped, port reusable. No rebuild. |
| New ingress during drain and resource ownership | Source gate rejects data503 before body/admission; control retains original128-connection budget and cannot extend deadline. Updated core stream test retains cleanup assertions. Prior final suites/probes exercise this; reviewer did not rerun full suites. |
| Restart validates before stopping old worker | Actual malformed replacement restart fails while old nonce remains unchanged. Source also checks credentials before stop, then starts fingerprint-pinned child. |
| Status stopped success | Direct exit0 + stopped observation. Identity failure yields failed/error exit1; stale stopped metadata is separately exposed. |
| Loopback proxy bypass | Source `.no_proxy()`, no redirects/retries, bounded control response; prior lifecycle proxy test retained. |
| Quota hold/cooldown separate from running, empty queue observable | Source explicit startup_hold_ms/shared_cooldown_ms independent of queue blocked reason; two focused lifecycle cases and final211 test logs retained. |
| Logs/readiness avoid secrets | Source sanitized bounded worker event, no token/URL in control identity; config parse display suppresses TOML values. Review outputs contain no token values. |
| ACL protection without mutation | Direct inherited Mac parent ACL case: new state rejected before either token exists. Existing ACL/token cases present in suite. No chmod applied to product/user state. |
| Cleanup + unrelated fixture survival | Independent probe helper survives all negative off checks; cleanup records zero live owned PIDs, closed last worker port, removed temp directory. Probe cleanup off1 was expected after intentionally rejected ACL state; no worker existed at that point. |

## Windows fixture assessment and acceptance boundary

The parent's candidate concern is supported by source: `tests/lifecycle_contract.rs:145-159` manually creates the stale-PID state directory and runtime file using std filesystem calls, and establishes permissions only under `cfg(unix)`. Windows production `platform/windows.rs:191-194` rejects a DACL lacking `SE_DACL_PROTECTED`; the fixture neither creates that DACL nor runs the production provisioner. Therefore on ordinary Windows inherited temp ACLs, this case is expected to fail at `off` line161 before testing stale PID behavior. This is **source-supported / runtime unverified**, not a newly observed Windows failure and not evidence of a production ACL bug. Before Windows suite acceptance, establish this fixture by starting/stopping through the public CLI and then replacing the protected runtime bytes (as the Unix owned-helper case already does), or use an explicitly secure platform fixture. Keep production rejection intact. This is a test portability gap, recorded separately from the directly reproduced lifecycle diagnostic blocker.

Full Windows product compile is still blocked at aws-lc-sys/SDK headers; isolated actual Windows API compile/clippy does not establish full build or runtime acceptance. Windows/Linux runtime, actual OS login, installers, wizard and clients remain later acceptance work. No performance conclusion follows from this review.

## Evidence and end state

`probe.py`/`probe-results.json` are independent actual held-release observations. `drain-test.log` is reviewer execution of the existing final release test executable only. `baseline.diff` is review-only accepted-baseline comparison. `identity-before.json`, `helper-identity.json`, and `identity-after.json` establish hold preservation. The review stops execution after cleanup and returns **HOLD / execution_finished**; no implementation changes are authorized by this verdict itself.
