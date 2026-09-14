# Accounting Ablation Task 1 — fresh SPEC review

**Verdict: FAIL / HOLD for the same implementer. Execution finished.** Four bounded
contract failures were reproduced. Do not start QUALITY or Task 2 until these
are fixed and freshly reviewed. This is a SPEC review using the required
subagent-driven-development review gate, not a quality review.

## Review target and preservation

Reviewed the actual five changed/added files, applicable AGENTS/README/HARNESS,
Task 1 plan, Native 1 source archive diff, and retained RED/GREEN evidence.
`before.json` and `after.json` independently verify all 59 held source files,
archive contents, retained Native 1 archive, three exact binaries, original
pilot manifest and its 40 raw JSON runs. No Rust/Cargo inputs changed. Mock
server and workload functions are byte-identical to the accepted Native 1
baseline; the HTTP diff only adds client usage observation. There is no Git
repository at this research root or product root; no Git mutation was performed.

- Task 1 manifest SHA256: `eb30edc170d8bc3f933c3f95962ef6103f948f8a6a65c1e981a1464963af69cc`
- Task 1 archive: 171,423 bytes, SHA256 `20a03d866f66a24bd2ff1335172f8c6ff44c5bb594037b25d17786d8cd2f96b3`
- Ordinary native binary: 8,697,040 bytes, SHA256 `e16ba39c54fcec9bc5ce4fb4039d551dccdd5cdfd3bec9e0a7c412e58bd4364f`

The review changed only this new research evidence directory. Product files,
prior evidence, source archives and measured binaries were preserved.

## Findings

1. **[P1] Provenance is built from intended text, not the actual launched TOML.**
   `product/scripts/benchmark.py:186-188` writes the generated text, and
   `:214-216` returns the metadata created before that write without comparing
   the actual file or startup identity. Its snapshot/hash originate at
   `product/scripts/benchmark_accounting.py:74-76`.
   The specifically forbidden mismatch was injected only at the builder return
   boundary: reserved TOML with actual metadata. The real held ordinary binary
   started successfully; the actual file and gateway admission status were
   `reserved`, while the metadata and snapshot remained `actual` and
   `validate_config_provenance` accepted them. Recorded and actual TOML hashes
   differed. See `probe_toml.py`, `probe-toml.json` and `probe-toml.log`.
   This is a **fault-injection verification of the required mismatch guard**,
   not a claim that the normal smoke used the wrong accounting policy. It
   requires an actual-file provenance check, not malicious-filesystem atomicity,
   an editor framework or a new security abstraction. The probe issued no
   upstream request; the owned child exited 0 and its temporary directory was
   removed.

2. **[P2] Resume accepts invalid numeric usage types.**
   `product/scripts/benchmark_accounting.py:253-273` checks status and numeric
   equality, but never repeats the required `type(value) is int`/u64 check on
   artifact data. Changing a completed generation's `prompt_tokens` from an
   integer to the numerically equal float, then rewriting the copied raw run
   and its registered SHA, passes the actual `resume_preflight` path. Python
   equality treats the float as equal to the expected integer. Thus a usage
   object the client observer would reject is accepted as verified saved
   evidence. `probe-contracts.json` records both `usage_equal_float` and
   `resume_rehashed_float_usage` as accepted; missing usage and a changed
   integer are correctly rejected. Preserve the terminal completion denominator
   while marking this accounting evidence invalid.

3. **[P2] Single-seed pilot contradicts the required checkpoint state.**
   `product/scripts/benchmark.py:548` derives `matrix_complete` solely from
   registered run count before the pilot branch at `:551-554`. A controlled
   fast fixture with `--mode accounting --phase quota --pilot-only --seeds 1
   --windows 5` produces `accounting_pilot_completed`, completed/planned pairs
   `1/1`, and **`matrix_complete=true`**. The accepted Task 1 contract explicitly
   requires pilot checkpoint `matrix_complete=false`. See `single_seed_pilot`
   in `probe-contracts.json` and the corresponding log. No 300-second quota
   runtime was used. The minimal state-contract remedy is an implementation
   decision; this finding does not require inventing a new minimum seed count.

4. **[P2] The standalone evaluator can double-count one physical pair.**
   `product/scripts/benchmark_accounting.py:378-380` rejects duplicate run keys,
   but `:387-413` iterates unvalidated `planned_seeds`. Calling
   `paired_summary(real_smoke_pair, [1, 1], "smoke")` succeeds and emits two
   identical seed-1 pair rows, `completed_pairs=2`, `planned_pairs=2`, from only
   two raw arm runs. See `duplicate_planned_seed` in `probe-contracts.json`.
   The standalone duplicate-pair requirement applies even though the CLI
   already rejects duplicate seeds. The existing duplicate-run negative check
   does pass.

## Direct verification

Commands ran in `product/`, with `PYTHONDONTWRITEBYTECODE=1` and separate output:

```sh
python3 -m unittest discover -s tests -p test_benchmark_accounting.py -v
python3 scripts/benchmark.py --self-check
python3 scripts/benchmark.py --mode accounting --smoke --binary target/native/release/llmgw --seeds 1 --output ../evidence/accounting-task1-spec-review/smoke.json
```

- `unit.log`: **19 passed**, including actual gateway startup repeated-cancel
  cleanup and controlled pilot/full-resume/failure fixtures.
- `self-check.log`: **PASS** for the existing transport, payload, boundary and
  baseline resume checks. No Rust/Pi suite was run.
- `smoke.json` / `smoke.log`: **completed_smoke**. Both arms submitted 20 and
  terminated all 20; 17 completed and 3 scheduled cancellations per arm.
  All 17 completions were within the independently recomputed 3-second cutoff.
  Actual observed attempt counts were reserved 19 and actual 20. Both arms had
  15 observed generation usages, 2 metadata not-applicable and 3 cancelled
  not-observed. These are this run's observations, not fixed attempt/usage
  requirements. Both used the exact same ordinary binary and empty features.
- `after.json`: raw JSON SHA and independent denominator/cutoff audit of both
  the reviewer smoke and implementer's final smoke; their outcomes match.
- `probe_contracts.py`: independent copied-result and fast-fixture probes;
  normal results, missing/changed usage, forged cutoff, duplicate runs, actual
  rehashed API-path/model-bound TOML snapshots, equal-float usage, one-seed
  pilot and duplicate planned seeds. The real recomputed changed TOML hashes
  are rejected. The implementer tests use placeholder hashes for those two
  cases; this review supplied the requested stronger negative check.
- `probe_toml.py`: one owned real native gateway startup with the focused
  metadata/TOML disagreement described above, followed by successful cleanup.

No new full quota matrix or efficiency measurement occurred. The normal smoke
retains `smoke_validation_only_no_efficiency_conclusion`.

## Contract coverage

| Task 1 contract | Review result |
|---|---|
| Same ordinary binary, two accounting values, no benchmark binary dependency | Verified in code, identities and real smoke |
| Baseline four arms, CLI default and internal explicit mode, invalid combination pre-I/O | Verified in diff/unit/self-check |
| Alternating `(phase, seed, arm)` schedule and disjoint IDs | Verified in code/unit/raw manifests |
| Actual TOML/snapshot/declared metadata accountability | **Finding 1**; normal generation agrees; rehashed API path/model-bound negatives reject |
| No credentials/body artifacts, fixed loopback/auth/retry/cancel/root/model/quota settings | Verified in changed code and synthetic artifacts |
| All accounting registered runs checked against planned identity/schedule/config before spawn | Verified in code/unit and own rehash probe, except **Finding 2** |
| Orphan/failed/tmp/duplicate/path/hash protection and startup PID/repeated-cancel cleanup | Retained code, unit/self-check pass |
| Full planned seed schedule retained, pair checkpoint and full resume, partial/second-arm failure | Unit/controlled fixtures pass, except **Finding 3** |
| Complete terminal denominator, exact cutoff, drain separation and per-seed/group accounting | Code/unit/raw audit pass |
| Standalone duration and duplicate-pair checks | Duration/duplicate raw run pass; **Finding 4** |
| Client fixed-fixture SSE usage type/range/order and completion/attempt linkage | Client unit tests pass; resume type gap is **Finding 2** |
| Missing completed generation usage fails; metadata/cancel absence stays valid | Independently checked and normal smoke confirmed |
| Retained RED then GREEN | Retained initial absent-module/mode-path failures and later integration failures inspected; startup file named `red-startup-resume.log` is honestly retained GREEN |
| Source-identical Rust usage basis, no new runtime/dependency changes | Current hashes match all eight files in `accounting-usage-source-basis.json`; no Rust rerun |
| Final held source, ordinary binary, commands and cleanup evidence | Verified before/after; this SPEC gate remains failed |

## Limits and final HOLD

No QUALITY review, product edit/build, Task 2 auditor, 300-second matrix, real or
paid provider/model call, model download/start, user client configuration, OS
registration or Git action was performed. Client response usage still does not
prove internal refund; the matching Rust contract files/log audit is a separate
source-backed basis, not a newly executed Rust test.

`after.json` records source/artifact invariance, all three recorded reviewer
smoke/probe gateway PIDs gone, no current llmgw process, and no matching owned
benchmark/review/unit temporary directory. Unit startup cleanup also directly
asserted its owned child no longer existed. **`execution_finished=true`**.

HOLD: return these four bounded findings to the same implementer. No runtime or
child remains owned by this review.
