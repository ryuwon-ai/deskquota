# Accounting Ablation Task 1 — SPEC rereview

**Verdict: PASS for the held SPEC-fix source. Execution finished / HOLD for the
parent's fresh QUALITY gate.** The four original required findings are resolved;
no remaining required SPEC finding was found within the Task 1 scope. QUALITY
and Task 2 were not started by this reviewer.

## Independent basis

Read the actual changed implementations and tests, and independently diffed the
current files against the original Task 1 archive. `independent-source.diff`
confirms only these four files changed since the failed review:

- `product/scripts/benchmark.py`
- `product/scripts/benchmark_accounting.py`
- `product/tests/test_benchmark_accounting.py`
- `product/docs/benchmark-method.md`

`benchmark_http.py`, Rust/Cargo, Mock server, workload, and the accepted ordinary
binary are unchanged. The original Task 1 SPEC report, probe programs, failure
JSON, logs and smoke remain intact. The prior full Task 1 contract coverage
continues to apply, with the four failures resolved below.

## Original finding closure

| Finding | Same-trigger direct verification | Result |
|---|---|---|
| S1: intended text mistaken for actual launched TOML | Builder returns reserved TOML with actual metadata. `start_gateway` now raises `written TOML does not match generated metadata`; subprocess mock call count is **zero**, and no gateway-start event exists. The exact written file contains reserved as injected. | PASS |
| S2: equal-valued float usage accepted on resume | Change a completed generation's saved prompt token integer to the equal float; rewrite copied raw run and its registered SHA; invoke actual `resume_preflight`. It now raises `completed generation usage must contain u64 integers`. | PASS |
| S3: one-seed pilot marked complete | Controlled fast quota-shaped `--pilot-only --seeds 1 --windows 5` fixture now gives `accounting_pilot_completed`, pair counts 1/1, `matrix_complete=false`. Resume without pilot-only gives `completed_accounting_matrix` and true while an assertion guards against any call to `run_arm`. | PASS |
| S4: duplicate planned seeds double-count one pair | `paired_summary(real_smoke_pair, [1,1], "smoke")` now raises `duplicate planned seed`. Duplicate raw run rejection remains intact. | PASS |

`probe_contracts.py` adapts the original review probe without changing its
counterexample triggers. `probe-contracts.json` and `.log` preserve results,
including normal run validation and adjacent missing/changed usage, forged
cutoff, and recomputed real TOML hash/API-path/model-bound negatives.

`probe_toml.py` preserves the original S1 builder mismatch trigger and additionally
checks the new readiness fingerprint comparison with one owned real ordinary
native child. The actual status fingerprint matched the written file and
accounting was actual. Corrupting only the returned fingerprint caused
`runtime config fingerprint mismatch`; the child was reaped before the exception
returned, temporary state was removed, and upstream attempts were zero.
`probe-toml.json` records the PID and cleanup. This is focused fault injection;
no malicious filesystem atomicity or new security framework is required.

The written-file guard reads bytes with `Path.read_bytes`, parses and hashes
those bytes, compares declared metadata, then validates the fixture profile.
Ordinary readiness compares the authenticated worker fingerprint and admission
accounting. The numeric artifact guard now repeats exact `int` and u64 range
checks. Pilot-only explicitly leaves `matrix_complete=false`. Both the pure
schedule helper and standalone evaluator reject duplicate seed inputs rather
than deduplicating silently.

## Direct execution versus retained evidence

Commands executed by this reviewer in `product/`, all with
`PYTHONDONTWRITEBYTECODE=1`:

```sh
python3 -m unittest discover -s tests -p test_benchmark_accounting.py -v
python3 scripts/benchmark.py --self-check
python3 scripts/benchmark.py --mode accounting --smoke --binary target/native/release/llmgw --seeds 1 --output ../evidence/accounting-task1-spec-review/rereview/smoke.json
```

The reviewer also ran the two research probes from the research root:

```sh
python3 evidence/accounting-task1-spec-review/rereview/probe_contracts.py
python3 evidence/accounting-task1-spec-review/rereview/probe_toml.py
```

- **Direct:** `unit.log`: 26 tests passed. This includes before-spawn builder and
  actual-write mismatch guards, real runtime-accounting mismatch cleanup,
  repeated startup cancellation cleanup, one-seed checkpoint/resume, equal-float
  resume rejection, and a mocked baseline benchmark-arm startup boundary.
- **Direct:** `self-check.log`: PASS, preserving the original transport,
  denominator and baseline resume checks.
- **Direct:** `smoke.json` / `smoke.log`: `completed_smoke`. Each arm submitted
  and terminated 20 requests: 17 completions, all within the recomputed 3-second
  cutoff, and 3 scheduled cancellations. Both arms used the same ordinary binary
  SHA and empty feature set. This review observed **19 attempts in each arm**.
  Each arm had 15 observed generation usages, 2 metadata not-applicable and 3
  cancelled not-observed. Attempt/usage observations are not invented fixed
  dispatch requirements.
- **Direct independent file audit:** `after.json` rechecks raw SHA, denominator,
  timestamp cutoff, completed generation input/output formulas and exact types,
  and runtime fingerprint/accounting agreement for both reviewer smoke and
  implementer final smoke. The implementer's preserved smoke had 20 reserved
  attempts and 19 actual attempts. Its artifacts were audited, not relabelled as
  reviewer execution.
- **Retained logs inspected:** implementer focused RED says 7 tests / 6 failures;
  focused GREEN says 9 passed; baseline fixture says 1 passed. These historical
  executions were not repeated as separate suites. Their relevant tests are
  included in this review's direct 26-test run.
- **Excluded diagnostic:** `baseline-bench-startup.log` records startup failure
  from the implementer's retained mixed-generation executable diagnostic. It is
  not counted as a current product regression result or baseline performance
  measurement. No old baseline executable was rerun; no fallback or Rust change
  was added. Preserved baseline startup behavior is verified by source diff and
  its focused mocked boundary, not by a newly measured four-arm matrix.

An initial edit of the review probe had an indentation error before execution;
`probe-contracts-initial-syntax.log` preserves it. Only the final successful
probe is counted as verification. No product code was changed to repair that
review-script error.

## Identity and final HOLD

`before.json` and `after.json` verify the unchanged 59 held source files and 91
preserved artifact/evidence files, including the three retained binaries,
original 40 raw pilot runs, old Task 1 archive and previous review evidence.
The current archive has 60 members: 59 matching source files and its matching
manifest.

- Source manifest SHA256: `681a6be9040d99038afbdb249bbb73923a14bb640c9dea2c1398ec34521c7dd6`
- Source archive: 173,487 bytes, SHA256 `8fa679bc16b651912e6ac68d3babc8d5958903782db12278633ad9195be5e255`
- Ordinary native binary: 8,697,040 bytes, SHA256 `e16ba39c54fcec9bc5ce4fb4039d551dccdd5cdfd3bec9e0a7c412e58bd4364f`

The review's smoke gateway PIDs 25985 and 26035 and fingerprint-probe PID 26590
are gone. Unit tests directly assert their separately owned cancelled/mismatched
startup children are gone. The final snapshot found no current llmgw process and
no matching owned benchmark/accounting temporary directory.

**`execution_finished=true`. No reviewer-owned runtime remains.** Source edits,
builds, Rust/Pi suites, a full 300-second quota matrix, real/paid API or model
calls, model download/start, user client settings, OS registration, Git actions,
QUALITY and Task 2 were not performed. Smoke establishes no efficiency result;
client-observed usage remains separate from internal refund proof.

SPEC PASS / HOLD: the parent may now dispatch fresh QUALITY against this exact
held source. This reviewer does not begin that gate or any next task.
