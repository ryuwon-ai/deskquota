# Core Task 4 independent quality re-review

Assessment: **READY FOR TASK 5**. Reviewed 2026-09-12. The initial findings remain preserved in `../task4-quality-review/README.md`; both are resolved in the current review target. No new Critical, Important or Minor finding was identified in the inspected fixes.

## Review target

- Script SHA-256: `3339124308fc5f589dc73260146f02ab9186fec6fb454163463b80542444ff68`.
- Regression test SHA-256: `a57a770723dfbf5071fa1640584d97a5eaef58596b73dd3b0662e23f45bc9d11`.
- Unchanged accepted binary SHA-256: `1149c72214dcb0121ef3dc92d023e424f9d9c8159b51bb0cc34fbd2bae51b771`.

All three hashes were checked before and after the independent checks and remained identical. The reviewer changed only this research evidence directory, ran no Git mutations, real Pi/model/API calls, user configuration operations, or OS service operations, and spawned no review subagents.

## Strengths and resolved findings

- `product/scripts/probe_pi.py:423-440` now owns Pi cleanup in `finally`: a live owned process is terminated/reaped through the existing bounded helper, both captured pipes are closed, and `KeyboardInterrupt` continues to propagate. The change preserves normal and timeout results without adding a new lifecycle abstraction.
- `product/scripts/probe_pi.py:84-115` returns unknown reference metadata immediately when the optional manifest is absent. It records a Git revision only after confirming the discovered top-level directory is the intended clone. The latter behavior was inspected in source; no synthetic Git repository was created.
- `product/scripts/probe_pi.py:715-719` leaves the comparison null when no reference version is known. The regression uses a synthetic package/binary and rejects any attempted Git lookup for an absent clone.
- The four focused standard-library regression tests exercise real owned child processes for normal, timeout and interrupted paths. They verify process exit and pipe closure; the absent-clone case verifies actual metadata returned by `build_metadata`.

## Direct independent verification

`python3 -B -m unittest tests.test_pi_probe -v` passed **4/4** tests with exit **0**. The runner inherited only PATH, LANG and bytecode-disable settings. The saved output is `unittest.log`.

The reviewer also repeated the original actual-SIGINT counterexample independently of the new mocked-communicate test: an unchanged module invocation of `run_pi` launched a reviewer-owned Python child sleeping for 12 seconds. A timer sent SIGINT only to the reviewer runner after 0.3 seconds. Observations after `run_pi` unwound:

| Observation | Result |
|---|---|
| `KeyboardInterrupt` propagated | true |
| Owned child still alive | false |
| Captured stdout and stderr closed | both true |
| Reviewer termination needed | false |
| Remaining owned child count | 0 |
| Remaining temporary-directory contents | 0 |

These results and before/after hashes are in `targeted-checks.json`. This reproduction used only a synthetic temporary directory and bounded owned process; no actual Pi or user file was involved.

## Assessment and limits

**READY FOR TASK 5** from the code quality review. The original cancellation leak is directly shown to be fixed, normal/timeout behavior remains covered, and absent optional provenance now stays unknown. The initial review's positive/negative acceptance and isolation strengths remain applicable.

This re-review did not rerun the unchanged Rust suite or the previously successful actual-Pi positive/negative flows. Current Pi artifacts and complete source-fingerprint reconciliation are the coordinating agent's separate checks. This verdict is for the Task 4 development compatibility harness; it makes no new runtime deployment, real-model, performance, or Windows claim.
