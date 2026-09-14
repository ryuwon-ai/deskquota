# Accounting Ablation Task2 SPEC Report Fix HOLD

Status: `TASK2_SPEC_REPORT_FIX_HOLD`  
Execution finished: `true`  
Owned runtime: `0`

The sole Task2 SPEC finding was resolved in the report only. The reviewed report
revision had SHA-256
`361a340ef367dffba18da82449b5987bb6fcb3563b55bb0ac98bb62b683e9f4f`.
The corrected `research/reports/accounting-ablation-results.md` has SHA-256
`67c360ba0f31093b3b02bc1c1cd8bf1da43836926734788e977dbfe52e222a44`.

The report now states, for every length and configured root subgroup, the
per-arm submitted denominator, fixed 300-second completed/cancelled/timeout
counts, post-cutoff drain completed/rejected/timeout counts, and final
completed/cancelled/rejected/timeout/error counts. Fixed rejected/error, drain
cancelled/error, and final error are explicitly accounted as zero.

The retained raw results show that final timeout reduction is confined to short
requests: short is `10 / 0` and long remains `15 / 15`, in reserved/actual
order. Actual's 15 final timeouts all belong to root 0. The report explicitly
does not treat drain outcomes as fixed-window throughput or fairness evidence.

Independent report-revision verification passed all 13 checks at
`task2-spec-report-fix-verification.json`, SHA-256
`5a33311f179ff2a18084bcb3ebc788db77f1ccbca350cc61015ed4a5b4af67a0`.
It recomputed the subgroup outcomes from all 10 raw files, reconciled fixed plus
drain to each final submitted denominator, checked zero outcomes and report
links, and found no mismatch in the review snapshot except the authorized
report revision.

The manifest remains SHA-256
`bfe367592eae5953edf0f1bb64dc808c7864f78ce41b48137f47d9dc9901c41c`.
The source manifest remains SHA-256
`f77ceac838cdf83fb5f7f0d7bd63536a3a9c62ecbecccf932c0a07c911a72304`.
The original and held binaries remain executable, 8,697,040 bytes, SHA-256
`e16ba39c54fcec9bc5ce4fb4039d551dccdd5cdfd3bec9e0a7c412e58bd4364f`.

Historical evidence was preserved. In particular, `TASK2_HOLD.md` remains
SHA-256
`0e12c32dea05e44a73fb01fdc40e1bde2a54ce2e3a5dc2bae1fa0bbed1d5d57c`
and `task2-final-verification-reread.json` remains SHA-256
`7c0bfd7f938257be6dc38542e34f93527392c60b845ea8dc4b30d4367234acb4`.

No product source, raw result, journal, manifest, binary, runtime, build, test,
Pi, user configuration, OS registration, Git, or Native wizard action was
performed. Task2 remains on HOLD for the same SPEC rereviewer, followed by a
fresh QUALITY review.
