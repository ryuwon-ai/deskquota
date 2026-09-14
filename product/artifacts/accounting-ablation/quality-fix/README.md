# Accounting ablation Task 1 QUALITY fix HOLD

Status: **DONE / HOLD for the same QUALITY reviewer.** Task 2 and the wizard
have not started.

## Changes

- Persisted accounting validation now requires both `initial_status` and
  `final_status`. Each saved ordinary-worker status is checked with the same
  runtime validator used at startup, so its config fingerprint and admission
  accounting must match the recorded TOML provenance.
- Resume rejects a missing status, an accounting/fingerprint mismatch, and a
  reserved run relabelled as actual even after the run and manifest hashes are
  recomputed. Rejection occurs before a later arm can start.
- Measurement start, measurement duration, every terminal `ended_s`, and the
  calculated cutoff must be finite numbers. `NaN` and positive/negative
  infinity cannot be assigned to within-cutoff or drain results.
- Successful synthetic fixtures now contain the ordinary worker's actual
  status structure: top-level state/counters, identity fields, and admission
  fields. The separate benchmark executable remains on its existing
  health-only startup boundary.

Only `docs/benchmark-method.md`, `scripts/benchmark_accounting.py`, and
`tests/test_benchmark_accounting.py` changed from the accepted SPEC-fix hold.
`source.diff` records that delta. No Rust, Cargo, dependency, binary, workload,
mock-response behavior, or prior artifact changed.

## Direct verification

```sh
PYTHONDONTWRITEBYTECODE=1 python3 -m unittest \
  tests.test_benchmark_accounting.AccountingRunValidationTests.test_nonfinite_measurement_timestamps_are_rejected \
  tests.test_benchmark_accounting.AccountingRunValidationTests.test_persisted_initial_and_final_worker_status_match_config \
  tests.test_benchmark_accounting.AccountingCliBoundaryTests.test_rehashed_runtime_status_forgery_rejects_before_next_spawn -v

PYTHONDONTWRITEBYTECODE=1 python3 -m unittest discover \
  -s tests -p test_benchmark_accounting.py -v

PYTHONDONTWRITEBYTECODE=1 python3 scripts/benchmark.py --self-check

PYTHONDONTWRITEBYTECODE=1 python3 scripts/benchmark.py --mode accounting \
  --smoke --binary target/native/release/llmgw --seeds 1 \
  --output artifacts/accounting-ablation/quality-fix/smoke-final.json
```

- `red-quality-findings.log`: 3 focused tests reproduced 20 pre-fix failures.
- `green-quality-attempt1.log`: retained intermediate run where one assertion
  expected an accounting mismatch but the relabelled artifact was correctly
  rejected even earlier for its fingerprint mismatch.
- `green-quality-findings.log`: all 3 focused tests passed after the assertion
  was aligned with the rejection contract.
- `unit-final.log`: all 29 accounting tests passed.
- `self-check-final.log`: passed.
- `smoke-final.json` / `smoke-final.log`: one final three-second accounting
  smoke completed. Each arm submitted and terminated 20 requests, with 17
  completions and 3 scheduled cancellations. Both arms happened to record 20
  attempts in this run. Every saved initial/final status matched its TOML
  fingerprint and accounting. This is validation only and supports no
  efficiency conclusion.
- `summary.json` and `cleanup.json`: independent source, archive, smoke,
  preservation, PID, and temporary-path audit.

## Held identity

- Source manifest: 59 files, SHA256
  `f77ceac838cdf83fb5f7f0d7bd63536a3a9c62ecbecccf932c0a07c911a72304`.
- Source archive: 60 members including the manifest, 174,816 bytes, SHA256
  `e057f32e5eb37c44eaa489fe15ab0f9846fde283261b7f9c690a4eefb4d5c9c4`.
- Final smoke manifest SHA256:
  `e1931ecd62031d39b38c163d1330de73036f7d2266aacaf7d8f11d5ba147e0fd`.
- Ordinary native binary remains 8,697,040 bytes, SHA256
  `e16ba39c54fcec9bc5ce4fb4039d551dccdd5cdfd3bec9e0a7c412e58bd4364f`.
- Smoke gateway PIDs `31684` and `31715` were reaped. No matching gateway or
  benchmark process and no owned temporary path remained at final audit.

No Rust/Pi suite or build, old benchmark executable run, 300-second quota
matrix, real or paid API/model call, model download/start, user configuration,
OS registration, Git/release action, Task 2, or wizard work was performed.
Client-observed usage remains fixture evidence rather than proof of the
gateway's internal refund behavior.
