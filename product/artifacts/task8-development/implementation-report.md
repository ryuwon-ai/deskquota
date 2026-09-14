# Core Task 8 implementation report — HOLD for independent SPEC / QUALITY

The full fixed matrix completed, and all owned processes have exited. The
measured source and original artifacts remain immutable. No competitor run,
real API/model, native client configuration/service change, or Git mutation was
performed.

## Directly verified

- Ordinary release and required-feature reference builds passed; both are native
  Mach-O ARM64 executables. No dependencies were added.
- `cargo fmt --check`; default `cargo clippy --all-targets --locked -- -D warnings`;
  feature example clippy; and `cargo test --locked` passed (191 Rust tests).
- Default exact trace target: 3 tests. Feature exact target: 4 tests, including
  both FIFO/RR arrival orders, timeout loss, continual small arrivals, sharing,
  cancellation and cap-one nonpreemption.
- 40 sequential HTTP runs: 2,000 no-wait measured completions plus 100 separately
  recorded warmups, and 2,000 quota submissions. Quota attempts actually received
  by the mock total 1,700. Every submitted ID and received attempt is terminal.
- Final quota outcomes across all arms: 1,526 completed, 152 rejected, 75 timeout,
  247 cancelled. This includes post-measurement drain and is not fixed-time goodput.
- Full run source identity matched all 49 held files and both executable hashes.
  Every completed artifact hash matches the original manifest. All 30 gateway
  PIDs plus harness PID 9443 are absent after exit. Session 78413 exited 0.

## Measured outcomes and limits

Every quota arm submitted 500 requests in five seeds of five real 60-second
composite windows, with RPM16 / TPM6000 and gateway concurrency2 / four roots.

| Arm | Complete within300s | Complete incl. drain | Rejected | Timeout | Cancelled | Paired fixture workflows within300s /250 |
|---|---:|---:|---:|---:|---:|---:|
| direct |328|328|150|0|22|125|
| production RR |285|399|1|25|75|130|
| benchmark FIFO |285|400|0|25|75|130|
| benchmark RR |284|399|1|25|75|129|

The paired workflows are predetermined independent fixture responses, not
agent/tool or real coding tasks. Direct paired completion varies21–28 per seed;
production RR and FIFO each produce26. The five-pair aggregate difference does
not establish superiority outside this artificial grouping. Direct completed
requests vary56–69 per seed; production RR completes57 within time for every seed.
Gateway overall completions after drain are not evidence of improved300s goodput.

Long requests: direct completes27/100 within time. Each gateway arm completes
25/100 within time and35/100 including drain, with15 long timeouts and50 long
cancellations. Gateway pending cancellation sent no mock request in this matrix;
direct's22 cancelled ingress had22 disconnected mock attempts. This is fixture
HTTP/budget behavior, not proof of provider compute cancellation.

Empty-ledger, no-wait, reused-connection paired completion-latency p95 difference
against direct: production RR0.1985–0.3480ms across seeds; FIFO0.1982–0.3541ms;
benchmark RR0.2335–0.3758ms. Negative pairs were retained. Those empirical ranges
overlap; no policy speed advantage is claimed. All three meet the tentative2ms
budget in this limited phase. Pure overhead with a populated known-quota ledger
remains unmeasured; quota wait/service samples do not fill that gap.

Production sampled idle RSS is8.625–8.688MiB; quota sampled peak11.094MiB. The idle
50MiB budget is met on this M4 development Mac. Sampled peaks are not OS high-water
marks. Production CPU delta is0.17–0.23 CPU-seconds per measurement-plus-drain run,
with the `ps` granularity retained in raw data. This does not prove low-end-PC,
Windows, sustained-memory, real-client, real-provider or actual task performance.

Two forwarded mock429s remain in the results (one TPM boundary in seed1 benchRR,
one RPM boundary during seed4 production drain). The parent independently
reconstructed mock budgets; gateway reservation timestamps are unobserved, so
clock/transport cause decomposition remains unconfirmed. No scheduler tuning,
deficit policy, altered quota window, or optimistic success filtering was used.
The pilot does not justify adding a more complex scheduler or claiming a general
throughput advantage. Further workload/accounting-mode evidence would be needed.

## Source and artifact identities

- Production SHA256: `410645600d70a69e69d9558420ea1cda61bf127a878497a6b8a0e88b17ba342e`
- Benchmark SHA256: `38b24b2934448716c74bad2dceaa91492e1bbc168b351e1f61460b90572c5faa`
- Original `artifacts/pilot.json` SHA256: `9e6cbbb57bfaac2149eb39efaf43169dbf8e47cf708075cf83b83716615854f6`
- Measured source archive: research `evidence/product-task8-measured-source.tar.gz`,
  SHA256 `622225adc3c885d4a7a6ed659ca5b9e32604b1318fb5606354dd36b4ebdce075`.
- Final review identity: `artifacts/task8-development/final-review-source-hold.json`.
  Only `scripts/benchmark.py` and `docs/benchmark-method.md` differ from measured
  source: resume integrity checks/self-check and the empty-ledger wording.
  No first-run transport/scheduler/binary changes occurred after measurement.

## Preserved failed and passing evidence

All logs are under `artifacts/task8-development/` and were never overwritten:

- `trace-red.log`: stub FIFO incorrectly bypassed the global oldest nonfit head;
  `trace-green.log` passed after the feature-only selection seam.
- `trace-expanded.log`: an initial expectation wrongly assumed FIFO HHL's light
  would complete; `trace-expanded-corrected.log` counts its timeout instead.
- `denominator-red.log` accepted a missing outcome in the stub validator;
  subsequent self-check logs reject missing/duplicate/unknown/unfinished records.
- `trailer-red.log`: a2s external watchdog proved old truncated chunk-trailer EOF
  blocked the asyncio loop; later boundary self-check rejects it promptly.
- `resume-red.log`: initial synthetic test fixture used an uncanonicalized macOS
  temporary path (test setup failure). `resume-red-canonical-path.log` then
  reproduces actual acceptance of a changed completed artifact with stale SHA.
- `self-check-review-source.log`: valid resume uses no new arm; stale SHA, missing
  artifact, redirected path and duplicate recorded ID all reject. A guarded dummy
  binary cannot run a product process. This post-measurement branch was unused in
  the original fresh matrix.
- `build-production-final.log`, `build-benchmark.log`, `fmt.log`,
  `clippy-default.log`, `clippy-feature.log`, `test-default.log`,
  `trace-default-final.log`, `trace-feature-final.log`: required Rust checks.
- `full-matrix.log`, `post-matrix-identity-verification.json`,
  `completion-audit.json`: complete matrix and cleanup checks.

No unchanged Rust suite was repeated for the subsequent Python/docs-only change.
SPEC and QUALITY acceptance are the parent's independent next steps, not claimed
by this implementation report.
