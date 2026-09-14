# Core Task 8 QUALITY rereview

**QUALITY PASS — READY FOR NATIVE TASK 1 within the accepted Core Task 8 scope.** Q8-1 and Q8-2 are resolved. No remaining Critical, Important or Minor actionable findings were identified in this bounded correction review. Reviewer execution is finished; **HOLD**, with no owned processes remaining. This is not full-project completion, deployment approval, or a performance-superiority finding.

## Reviewed correction and strengths

The implementer remained HOLD. I read `product/artifacts/task8-quality-fix/REPORT.md`, `final-audit.json`, `final-source-hold.json`, then independently examined and executed the changed paths. Only `scripts/benchmark.py` and `docs/benchmark-method.md` differ from the original QUALITY snapshot. `fixed-delta.diff` preserves the complete delta. Rust, dependencies/lockfile, fixtures, both binaries and the original measurements did not change.

- **Q8-1 resolved:** `product/scripts/benchmark.py:365` centralizes full-directory validation before manifest writes or dispatch. Reused results are only those registered and prevalidated by manifest identity, expected path/hash and content ID/phase/seed/arm. Unregistered completed JSON, journal, failed output and temporary output fail closed across the entire discoverable directory. There is no orphan adoption or compatibility/migration branch.
- **Q8-2 resolved:** `product/scripts/benchmark.py:187` journals the acquired child's PID before readiness and retains ownership until successful return. The exception path runs the bounded `abort_startup()` task under shielding; later cancellations cannot replace the original exception or interrupt that cleanup. Normal successful readiness transfers the live Process to the run owner. The shared production binary and its HTTP behavior are unchanged.

## Fresh independent results

| Check | Direct observation |
|---|---|
| Exact original Q8-1 counterexample | The actual measured seed1/direct JSON, renamed into a fresh seed999 output directory, was rejected with **no manifest created and no arm launched**. See `original_resume_green.py`, `original-resume-green-result.json`. |
| Independent resume regression | **16 cases passed:** registered valid reuse; fresh orphan; later orphan; later incomplete journal/failed/tmp output; registered content ID/phase/seed/arm mismatch; stale SHA; missing file; duplicate record; redirected path; old source identity; nonterminal outcome. Every rejection preceded new dispatch and preserved existing bytes. `resume-results.json` and `resume.log`. These include the original later-orphan-before-earlier-arm boundary. |
| Exact original Q8-2 run ownership gap | Cancelling `run_arm` during deliberately gated health readiness left no child alive and its journal contained `gateway_started` PID. The original cancellation argument survived. Final-run PID72031 exited -15 and was waited for. |
| Readiness exception | The exact injected RuntimeError object was re-raised after PID72032 was terminated/reaped. |
| Repeated cancellation plus escalation | Three cancellations during pending readiness/cleanup retained the **first** cancellation argument. To force escalation, reviewer fault injection made the acquired Process's `terminate()` ineffective; its actual held-binary child was stopped and then killed by the production helper after ~3.006s. PID72033 exited **-9** and was reaped. This exercises the real kill/wait path but does not claim that the gateway naturally ignores SIGTERM. |
| Real nonzero startup | Held binary with a deliberately invalid argument exited **2**, was journaled/reaped, and reported `gateway failed startup`. PID72042. |
| Normal ownership transfer | Readiness returned the actual live Process; the run owner used normal control stop and got exit **0**. PID72043. |
| Integrated self-check | Fresh `benchmark.py --self-check` PASS, including **5 actual loopback HTTP exchanges** (3 quota fixture + 2 malformed-response boundary exchanges) and its own resume checks. `self-check.log`. |

The lifecycle checks are in the independently written `lifecycle_regression.py`, with `lifecycle-results.json` and `lifecycle.log`. They execute actual held binaries and temporary configs only. **Data-plane gateway ingress = 0; gateway mock-upstream attempts = 0.** The successful lifecycle set made four calls to the real control helper (health/stop attempts); these are not data requests, and we did not instrument the exact control wire-message count. No benchmark arm or new performance matrix was run.

An initial lifecycle fixture tried SIGSTOP alone to force ineffective SIGTERM. That premise did not hold on this Mac, so its escalation assertion failed after the child exited; this was a **reviewer test-setup failure**, not a remaining product bug. Preserve `lifecycle-first-setup.py`, `lifecycle-first-setup-failure.log`, and `lifecycle-first-setup-results.json`. Its two preceding cases passed and all three children71955/71956/71957 were reaped. The corrected fault injection is explicit in the successful probe. Across both attempts **8 actual child processes** were started and all were waited for and independently absent afterward.

## Identity and original evidence

`identity.py before` and `identity.py after` independently verified all **50 source files**, both binaries, original `pilot.json`, **40 raw run files**, and both preexisting source archives. The before/after JSONs are equal; raw files match their recorded pilot SHA256 values. The fixed-source changes remain limited to the two intended Python/documentation files. No original reviewer evidence file was rewritten; all new work is under this `rereview/` directory.

| Artifact | Verified SHA256 |
|---|---|
| Fixed source hold | `f9e9a2d68e0fa4bdd5266bd932a44d69b74ccc414b03e4c2cb47d2ab511cb1e7` |
| Parent fixed source archive | `c4bb57f143bc03e1c23fb51fee5bedba4d89ca01106f5f09106ec318a383cb29` |
| Production executable | `410645600d70a69e69d9558420ea1cda61bf127a878497a6b8a0e88b17ba342e` |
| Benchmark executable | `38b24b2934448716c74bad2dceaa91492e1bbc168b351e1f61460b90572c5faa` |
| Original pilot | `9e6cbbb57bfaac2149eb39efaf43169dbf8e47cf708075cf83b83716615854f6` |
| Measured source archive | `622225adc3c885d4a7a6ed659ca5b9e32604b1318fb5606354dd36b4ebdce075` |
| Original QUALITY source archive | `b2a1813489d6f146b7b550994bfc57331622dca3cf379baf701e2ef71bc60f23` |

The original review independently audited 40 runs/4,000 measured ingress plus100 warmups and their arithmetic; this rereview verifies those exact files remain unchanged. The original 191-test Rust suite,3/4 exact traces, builds/fmt/clippy and matrix are historical execution logs, **not rerun in this review**. The implementer's corrected four-arm80-ingress smoke was read as implementation evidence, not counted as my execution.

## Limits and cleanup

The final corrected Python source is distinct from the measured source. Earlier PID journaling and readiness ownership checks do not change the original binary, fixture, HTTP timing definitions or measured outcomes. The original performance figures remain historical results from their recorded source. No corrected-source performance measurement or scheduler advantage is claimed.

Empty-ledger/M4 overhead, sampled RSS, synthetic costs and artificial response pairs retain the original limitations. Populated-known-ledger pure overhead, real API/model/task/client behavior, low-end machines and other OS/native installation remain unverified and are not prerequisites newly added to Task8 acceptance.

`cleanup.json` records all8 owned PIDs absent after reaping, all probe temporary directories removed, and before/after identity equality. No Rust target was created. Product/reference/current-user config/autostart/model state was not changed; no install, real endpoint, Git mutation or unrelated process termination occurred. **QUALITY PASS applies to Core Task8; the parent may advance to the separately approved Native Task1.**
