# Accounting ablation Task 1 SPEC fix HOLD

Status: **DONE / HOLD for the same SPEC reviewer.** QUALITY review and Task 2
have not started.

The four review findings were fixed within the Python benchmark harness and its
method documentation:

1. Ordinary product runs now parse and hash the exact written TOML before any
   subprocess spawn, compare it with generated metadata, and verify the started
   worker's authenticated config fingerprint and admission accounting. A
   runtime mismatch reaps the owned child.
2. Persisted completed-generation usage is accepted only when both token fields
   have exact `int` type and fall in `0..=u64::MAX`; a rehashed equal-valued
   float fails resume before another run starts.
3. A `--pilot-only` checkpoint always records `matrix_complete=false`, including
   a one-seed schedule. Resuming the same one-seed identity without
   `--pilot-only` reuses the verified pair and completes without another run.
4. The accounting schedule and standalone paired evaluator reject duplicate
   planned seeds instead of counting the same physical pair twice. The CLI
   rejection remains before output creation or spawn.

The ordinary-only provenance checks are deliberately limited to
`production_rr` and `production_actual`. `benchmark_rr` retains its existing
health-only startup contract and bench-harness identity, covered by the focused
fixture. `baseline-bench-startup.log` is an honest diagnostic failure from
mixing the retained older benchmark executable with the newer native doctor's
state layout. It is not counted as a regression test or benchmark result, and
no compatibility fallback or rebuild was added.

## Direct verification

- `red-four-findings.log`: focused pre-fix run, 7 tests with the expected 6
  failures.
- `green-four-findings.log`: focused post-fix run, 9 tests passed.
- `baseline-fixture-green.log`: benchmark-arm health-only boundary fixture
  passed.
- `unit-final.log`: `python3 -m unittest discover -s tests -p
  test_benchmark_accounting.py -v`, 26 tests passed.
- `self-check-final.log`: `python3 scripts/benchmark.py --self-check`, passed.
- `smoke-final.log` and `smoke-final.json`: one final three-second accounting
  smoke completed. Each arm submitted and terminated 20 requests with 17
  completions and 3 scheduled cancellations. Observed attempts were 20 for
  reserved and 19 for actual. This is smoke validation only and supports no
  efficiency conclusion.
- `summary.json`: independent final source, smoke, provenance, preservation,
  and cleanup audit.
- `cleanup.json`: both final-smoke gateway PIDs were reaped; no matching process
  or owned temporary path remained.

## Held identity

- Source manifest: 59 files, SHA256
  `681a6be9040d99038afbdb249bbb73923a14bb640c9dea2c1398ec34521c7dd6`.
- Source archive: 60 members including the manifest, 173,487 bytes, SHA256
  `8fa679bc16b651912e6ac68d3babc8d5958903782db12278633ad9195be5e255`.
- Final smoke manifest SHA256:
  `cc7704c0209d72b715827eb39091a801dc1afd7e019c4f3050ce9260225d3a56`.
- Ordinary native binary remains 8,697,040 bytes, SHA256
  `e16ba39c54fcec9bc5ce4fb4039d551dccdd5cdfd3bec9e0a7c412e58bd4364f`.

Compared with the prior Task 1 hold, only `docs/benchmark-method.md`,
`scripts/benchmark.py`, `scripts/benchmark_accounting.py`, and
`tests/test_benchmark_accounting.py` changed. `source.diff` records that exact
delta. No Rust/Cargo source, dependency, binary, prior evidence, or measured
artifact changed.

No Rust/Pi suite or build, full 300-second quota matrix, real or paid API/model
call, download/model start, user setting, OS registration, Git action, release,
QUALITY review, or Task 2 work was performed. Client-observed fixture usage is
still not proof of the gateway's internal refund behavior.
