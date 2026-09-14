# Accounting ablation Task 1 — implementation HOLD

**DONE_WITH_CONCERNS / HOLD for fresh SPEC, then fresh QUALITY review.** Product
source edits and runtime are stopped. The full 300-second accounting pilot is
Task 2 and was not started.

## Scope

Changed product files:

- `scripts/benchmark.py`
- `scripts/benchmark_http.py`
- `docs/benchmark-method.md`

Added product files:

- `scripts/benchmark_accounting.py`
- `tests/test_benchmark_accounting.py`

The accepted Rust source, `Cargo.toml`, `Cargo.lock`, and native release binary
were not changed or rebuilt. No dependency, user client setting, OS registration,
real provider/model request, paid API call, Git operation, or external message was
performed.

## Direct verification

`unit-final.log` records 19 passing accounting tests. These cover the alternating
two-arm schedule, five-window CLI boundary, pilot checkpoint and full resume,
mode/seed/window mismatch, partial pair and second-arm failure, forged accounting,
TOML path/model/quota/config metadata, complete denominator and exact cutoff,
usage types/range/order/linkage, paired summaries, and repeated cancellation while
an actual owned gateway child is starting.

`self-check-final.log` records `self-check PASS` for the existing HTTP, quota,
payload, trailer, fixed-window, orphan, duplicate, redirected, missing, modified,
and incomplete-journal checks.

Final held-source smoke command:

```sh
python3 scripts/benchmark.py --mode accounting --smoke \
  --binary target/native/release/llmgw --seeds 1 \
  --output artifacts/accounting-ablation/task1/smoke-final.json
```

`smoke-final.json` is `completed_smoke`, with one complete pair and
`smoke_validation_only_no_efficiency_conclusion`. Each arm has 20 submitted and
20 terminal rows: 17 completed and three scheduled cancellations. Reserved had
19 observed mock attempts; actual had 20. This timing-dependent count is recorded,
not forced. Each arm has 15 valid generation usage observations, two completed
metadata `not_applicable` observations, and three cancelled `not_observed`
observations. Both launch the same ordinary 8,697,040-byte binary SHA256
`e16ba39c54fcec9bc5ce4fb4039d551dccdd5cdfd3bec9e0a7c412e58bd4364f`
with no features. The generated TOML and parsed snapshot say `reserved` for
`production_rr` and `actual` for `production_actual`.

The earlier `../smoke.json` and its run directory are preserved as an intermediate
successful smoke. Its source identity predates final documentation and test
changes, so it is not relabelled as final evidence.

## RED/GREEN and provenance

- `red-initial.log`: missing accounting mode/module/config behavior failed.
- `red-usage-provenance.log`: missing usage observation, run validation, and
  accounting identity behavior failed.
- `red-resume-pilot.log`: unused benchmark identity and accounting main/resume
  behavior failed before integration.
- `unit-final.log`, `self-check-final.log`, and `smoke-final.log`: final GREEN.
- `red-startup-resume.log`: preserved intermediate all-green checkpoint despite
  its early working filename; it is not represented as RED.

`source-manifest.json` freezes 59 product source files and matches the final smoke
identity and accepted native binary. Manifest SHA256:
`eb30edc170d8bc3f933c3f95962ef6103f948f8a6a65c1e981a1464963af69cc`.
`source-hold.tar.gz` is 171,423 bytes, SHA256
`20a03d866f66a24bd2ff1335172f8c6ff44c5bb594037b25d17786d8cd2f96b3`.

The accepted Native Task 1 archive remains SHA256
`d84f5b1ee94fc2a57a697b92007e2b2715168735a97ef33c2209f4e01f6d5c33`.
The original Task 8 pilot remains SHA256
`9e6cbbb57bfaac2149eb39efaf43169dbf8e47cf708075cf83b83716615854f6`.
Final inspection found no owned `llmgw`/benchmark process and no owned benchmark
temporary directory.

## Limits

The smoke proves only bounded loopback harness behavior and provenance. It does
not establish a reserved-versus-actual efficiency difference. Client-observed
usage is fixture response evidence, not direct proof of the gateway's internal
refund; the separately audited, source-identical Rust contracts remain that
basis. The five-seed 300-second matrix, independent parent auditor, real provider,
actual clients, and other operating systems remain unverified in this task.

