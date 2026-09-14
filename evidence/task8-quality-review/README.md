# Core Task 8 independent QUALITY review

**Verdict: CHANGES REQUIRED — two Important issues. Do not advance to Native Task 1 yet.** No Critical or additional Minor issues identified in the reviewed Task 8 delta. Fresh SPEC PASS was treated as the scope boundary, not as a quality verdict. Product source and measurements remain unchanged; reviewer execution is finished and HOLD is released for a narrowly scoped implementer fix.

## Important findings

1. **Unregistered completed artifacts bypass provenance validation.** `product/scripts/benchmark.py:381-393`, especially 384-385, accepts any existing scheduled filename with `validate_run()`, even if it was absent from the old manifest. The preflight at 357-372 only covers recorded entries. It checks neither the reused run's id/phase/seed nor source/binary provenance. In `probe_resume.py`, the actual original measured `no_wait-seed1-direct` JSON was copied into an exclusive reviewer output as `no_wait-seed999-direct.json`; the requested seed was 999 and the current source identity differs from the measured source. With all new arm execution forbidden, `main()` emitted `completed_no_wait_only` and recorded seed999 while its file contained seed1. Fresh ingress/upstream: **0/0**. See `resume-orphan-result.json` and `resume-orphan.log`.

   An additional existing-manifest fixture placed an unregistered malformed JSON at a later scheduled run. `main()` attempted to launch the earlier new arm before noticing that already discoverable orphan; the reviewer blocked the launch. See `orphan-preflight-result.json`. Reject unregistered completed JSONs across the output's full run state before any new arm starts, including when the manifest does not exist. Retain the existing path/hash/duplicate guards and reject content identity mismatches for reused registered runs. Do not silently adopt orphan files: the current per-run format lacks enough identity to establish safe provenance. This is a resume/fresh-output reuse defect, not evidence that the original uninterrupted matrix reused foreign results.

2. **Startup cancellation loses ownership of the spawned gateway.** `product/scripts/benchmark.py:174-185` spawns a child and then awaits readiness; `run_arm` only receives its `proc` at 233-235. Cancellation in that interval reaches the outer `finally` with `proc=None`, so `stop_gateway()` cannot clean the child. The independent `probe_startup_cancel.py` started the actual held production binary, deliberately gated the health await, and cancelled `run_arm` after spawn. PID **66733** remained alive after the function's failure/finally handling, and the journal contained no `gateway_started` ownership record. Fresh data ingress/upstream: **0/0**; readiness delay was fault injection, not a naturally measured network timeout. See `startup-cancel-result.json` and `startup-cancel.log`.

   Enclose all post-spawn startup awaits in exception/cancellation-safe child cleanup, terminate/kill only that child and await its exit, while retaining the original exception. Record the PID immediately after acquiring it rather than only after readiness. The reviewer terminated exactly PID66733 and waited for exit `-15`; it is absent now. This finding concerns the new benchmark harness, not native lifecycle features beyond Task 8.

## Strengths and reviewed boundaries

- Task 7 archive comparison shows only 14 changed/added Task 8 paths; no dependency or Cargo.lock changes. Runtime changes are the compile-time `bench-harness` FIFO seam and bounded retained-ledger count. Default production RR, shared HTTP transport/ledger/queue bounds, body-byte estimator and request deadlines remain intact by source comparison. FIFO consistently refuses bypass of the global oldest nonfitting head.
- The separate example requires `bench-harness`; product CLI/config does not gain a policy selector. Exact-cost/manual-clock traces remain outside the HTTP path. Existing references/design establish FIFO/RR and measurement conventions; no new architecture was invented for this review.
- The original 40 run files retain every ingress outcome, terminal attempts, warmups, failure counts, and fixed-time versus drain completion. Fresh read-only arithmetic independently reproduced direct328/product285/FIFO285/benchRR284 within 300s and product114 additional drain completions. All run hashes, manifest summaries and recomputed group summaries matched.
- No-wait samples are explicitly unlimited-quota **empty-ledger** observations on this M4. Paired index differences from separate runs, empirical ranges, sampled RSS, fixture cost units, artificial response-pair workflows and unobserved gateway reservation timestamps are appropriately bounded in the reports. The source/report does not claim scheduler superiority, real provider quota exactness, populated-ledger pure overhead, low-end/other-OS support or real-agent completion.
- The measured source archive differs from final review only in `scripts/benchmark.py` (resume guard/self-check) and `docs/benchmark-method.md` (resume/empty-ledger explanation). The original matrix used the fresh-run branch; no Rust/runtime/binary change occurred afterward. `post-measurement.diff` records this distinction. The initial noncanonical temporary-path resume test error and the later actual RED and GREEN logs are retained by the implementer.

## What I executed versus what I read

| Evidence | Fresh independent execution in this review |
|---|---|
| `benchmark.py --self-check` | PASS; 5 loopback HTTP requests (3 quota fixture exchanges, 2 malformed-response boundary exchanges), no gateway started. Its real resume self-check also passed. |
| `probe_resume.py` | Reproduced wrong-seed/unregistered artifact adoption; 0 fresh HTTP requests, 0 new arms. |
| `probe_orphan_preflight.py` | Reproduced missing whole-state preflight; attempted earlier arm was intercepted, 0 actual arms/HTTP. |
| `probe_startup_cancel.py` | 1 held production gateway process, 0 data ingress/upstream, controlled pending health await; child leak reproduced and reviewer cleaned it. |
| `audit_original.py` | Read-only 40 files, 4,000 measured ingress, 100 warmup, 3,700 measured mock attempts (quota1,700); hashes, source-to-measured-archive identity, all terminal denominators, 300s cutoff and group summaries checked. This is not replayed HTTP traffic. |
| `audit_identity.py before/after` | 50 source hashes, 2 binary hashes, original pilot and both archives identical before and after. |

I read the required build/fmt/clippy logs and the default Rust suite (191 pass) and default/feature exact traces (3/4 pass). I did **not** rerun Rust builds/tests, the full ~139-minute matrix, the SPEC review's smoke/payload probes, or any real API/model/client flow. The fresh SPEC review's 80-ingress smoke and other executions remain that reviewer's evidence, not mine. No reviewer target directory was needed.

## Identity and cleanup

`before-identity.json` and `after-identity.json` contain the complete per-file hashes and are equal. Final archive member bytes matched the 50-file manifest; `.gitignore` accounts for the difference from the implementer's 49-file source hold. `product/.git` is absent; no Git operations were run.

| Artifact | SHA256 before = after |
|---|---|
| Final review source archive | `b2a1813489d6f146b7b550994bfc57331622dca3cf379baf701e2ef71bc60f23` |
| Measured source archive | `622225adc3c885d4a7a6ed659ca5b9e32604b1318fb5606354dd36b4ebdce075` |
| Production release | `410645600d70a69e69d9558420ea1cda61bf127a878497a6b8a0e88b17ba342e` |
| Benchmark example | `38b24b2934448716c74bad2dceaa91492e1bbc168b351e1f61460b90572c5faa` |
| Original pilot | `9e6cbbb57bfaac2149eb39efaf43169dbf8e47cf708075cf83b83716615854f6` |

All original 31 recorded PIDs were independently absent at the closing artifact audit. Reviewer PID66733 was individually terminated and awaited; temporary probe directories were removed by their owners. No models, installs, real endpoints, current-user configuration, autostart, reference files or product files were changed. Reviewer outputs are confined to this directory. The first identity helper attempt encountered the archive's extra `source-manifest.json` member; it made no product changes, then correctly filtered to `product/` members before passing. This helper setup error is not a product failure.

## Next gate

Fix the two bounded harness paths, retain original measurement evidence, rerun the concrete regressions and relevant harness checks, then return the fixed source to independent review. Task 8 is not QUALITY PASS yet; Native Task 1 should wait. No full-project completion or deployment claim is made.
