# Task 8 QUALITY corrections — final HOLD

Only `scripts/benchmark.py` and `docs/benchmark-method.md` changed from the pre-fix review source. No Rust, Cargo manifest/lock, scheduler, executable, measured fixture or original result changed. The original full matrix was not rerun. The unchanged Rust suite/build was not repeated.

## Q8-1: reject unregistered resume artifacts before dispatch

The complete run directory is prevalidated before writing the manifest or launching any arm, including a fresh output with an existing run directory. Registered records must match expected ID, path, SHA256 and content ID/phase/seed/arm and pass denominator validation. Unknown files and incomplete journals fail closed. The dispatch loop reuses only prevalidated registered results. A crash between completed-run write and manifest registration leaves an orphan requiring inspection and a new output; no automatic adoption or provenance migration occurs.

Behavioral RED was reproduced with independent copies of the reviewer's probes before edits: a seed 1 result renamed to seed 999 was accepted, and a later orphan allowed an earlier seed 998 launch attempt (intercepted, zero actual arms). `resume-red.log`, `resume-orphan-result.json`, `orphan-preflight-red.log`, and `orphan-preflight-result.json` preserve this evidence. Each probe exited 0 because its assertions demonstrated the bug.

GREEN: `probe_resume_green.py` and `probe_orphan_preflight_green.py` reject those same counterexamples with zero launched arms. Their corresponding `*-green-result.json` and logs preserve the observations. The integrated `--self-check` additionally verifies valid resume, stale SHA, redirected path, duplicate/missing entry, each mismatched content identity field, fresh orphan, later orphan and later incomplete journal. Rejections preserve existing input bytes and the fresh orphan test creates no manifest.

## Q8-2: retain child ownership throughout readiness

`start_gateway` closes its parent log handle through a context manager, journals the acquired Process PID before readiness, and retains ownership until successful return. BaseException cleanup terminates/waits up to 3 seconds, then kills/waits up to 3 seconds if needed. Shielding keeps repeated cancellation from interrupting that cleanup. The original exception is re-raised; cleanup errors are attached as notes rather than replacing it. No stale PID is used to kill a process.

Behavioral RED: `probe_startup_cancel.py` cancelled readiness after a real held gateway spawned. PID 68747 survived run cleanup and had no gateway-started journal record. The reproduction's own finally terminated/reaped it; `startup-cancel-red.log` and `startup-cancel-result.json` preserve the failure and cleanup. The original reviewer files were never written.

GREEN: the copied counterexample now leaves no surviving child and records its PID before readiness. `lifecycle_regression.py` verifies an injected readiness exception retains object identity, two readiness cancellations retain the original cancellation argument, and normal readiness returns a live Process which the run owner stops with exit 0. `startup_exit_regression.py` uses the held binary with deliberately invalid fixture arguments to verify a real nonzero startup exit is journaled/reaped and reports `gateway failed startup`. These startup regressions submit zero HTTP data ingress and zero mock upstream attempts. All owned children were waited for; logs/results are retained. Terminate-to-kill escalation was implemented with bounded waits but was not forced against an unresponsive child in these probes.

## Commands and evidence

All paths below are relative to product. Commands used the local stdlib Python runtime and held binaries only.

| Command | Exit / evidence |
|---|---|
| `python3 artifacts/task8-quality-fix/probe_resume.py` | 0, behavioral RED, `resume-red.log` |
| `python3 artifacts/task8-quality-fix/probe_orphan_preflight.py` | 0, behavioral RED, `orphan-preflight-red.log` |
| `python3 artifacts/task8-quality-fix/probe_startup_cancel.py` | 0, behavioral RED, `startup-cancel-red.log` |
| `python3 artifacts/task8-quality-fix/probe_resume_green.py` | 0, GREEN, `resume-green.log` |
| `python3 artifacts/task8-quality-fix/probe_orphan_preflight_green.py` | 0, GREEN, `orphan-preflight-green.log` |
| `python3 artifacts/task8-quality-fix/probe_startup_cancel_green.py` | 0, GREEN, `startup-cancel-green.log` |
| `python3 artifacts/task8-quality-fix/lifecycle_regression.py` | 0, `lifecycle-green.log` |
| `python3 artifacts/task8-quality-fix/startup_exit_regression.py` | 0, `startup-exit-green.log` |
| `python3 scripts/benchmark.py --self-check` | 0, final `final-self-check.log` |
| `python3 scripts/benchmark.py --binary target/release/llmgw --reference-binary target/bench/release/examples/bench_gateway --seeds 941 --windows 1 --smoke --output artifacts/task8-quality-fix/smoke.json` | 0, session 76887 completed; `smoke.log` |
| Initial artifact `audit.py` | 1, helper incorrectly read attempt `terminal` instead of `outcome`; preserved `audit-first-failed.py` and `final-audit.log`; product unaffected |
| Corrected artifact `audit.py` | 0, `final-audit-green.log`, `final-audit.json` |

Smoke: four arms, 80 ingress = 68 completed + 12 cancelled; 77 observed mock attempts = 68 completed + 9 disconnected (20 direct, 19 each proxy). All ingress and attempts terminal, no pending discarded. Three cancellations never reached the mock. This is unlimited-quota compressed correctness smoke, not a new performance measurement or a replacement for the original quota matrix.

The final audit verified original pilot + 40 raw run files + 2 binaries (43 files), both source archives, and the four new smoke artifact hashes. It checked all ten recorded harness/child PIDs from this correction's probes/smoke; none remained present. Lifecycle tests and smoke had already reaped owned children before this read-only PID check.

## Identity and measurement boundary

`final-source-hold.json` records all 49 source hashes plus `.gitignore` and both executable identities. The measured source and pre-fix review source remain separate archives. New failure/resume checks are post-measurement. The successful fresh-run change is earlier PID journaling and guarded readiness ownership; it does not change HTTP timestamp definitions, submitted fixture data, native code, or the existing measurements. No corrected-source performance claim is made from the correctness smoke.

- Production binary: `410645600d70a69e69d9558420ea1cda61bf127a878497a6b8a0e88b17ba342e` (no features).
- Benchmark binary: `38b24b2934448716c74bad2dceaa91492e1bbc168b351e1f61460b90572c5faa` (`bench-harness`).
- Original pilot: `9e6cbbb57bfaac2149eb39efaf43169dbf8e47cf708075cf83b83716615854f6`.
- Measured source archive: `622225adc3c885d4a7a6ed659ca5b9e32604b1318fb5606354dd36b4ebdce075`.
- Pre-fix review source archive: `b2a1813489d6f146b7b550994bfc57331622dca3cf379baf701e2ef71bc60f23`.

Original performance findings and limitations remain unchanged: the no-wait result measures an empty ledger, known-quota populated-ledger pure overhead remains unmeasured, and real tasks/clients/models, other OSes and low-end hardware remain unverified. No broad gateway performance advantage is established.

**HOLD.** No owned process remains and no further product edits or runs are active. Return to the same QUALITY reviewer; this report is implementation evidence, not self-approval.
