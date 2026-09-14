# Accounting ablation Task 2 — MATRIX_HOLD

Status: **MATRIX_HOLD.** `execution_finished=true`; owned runtime is zero.

The exact resume command exited 0. The final manifest is
`completed_accounting_matrix`, with 10 runs / 5 pairs and
`matrix_complete=true`. Seed 1 raw and journal bytes were reused unchanged; the
resume created only the remaining eight runs.

At the fixed 300-second cutoff, reserved completed 285/500 and actual completed
290/500. Every seed's actual-minus-reserved difference is +1. Generation
completions are 255 versus 260; metadata completions are 30 versus 30. This is a
descriptive result from five seeded-timing repetitions of one fixed synthetic
fixture. It is not five independent real workloads, and the same aggregate
length count can contain different completed request IDs.

The complete terminal denominator is preserved. Reserved ended with 397
completed, 75 cancelled, 3 rejected and 25 timeout; actual ended with 410
completed, 75 cancelled and 15 timeout. Post-cutoff drain alone was reserved
112 completed / 3 rejected / 20 timeout and actual 120 completed / 10 timeout.
Observed mock attempts were 400 and 410, with 3 and 0 mock 429s respectively;
neither arm had an additional attempt beyond the first for an ingress.

Direct runtime checks passed for raw hashes, exact within-pair submitted traces,
config-only accounting difference, status provenance, 1,000 unique terminal
outcomes, attempt linkage, cutoff classification, payloads, numerical usage,
and cleanup. Source 59, frozen usage-basis 8, archive and ordinary binary remain
on HOLD. The measured 8,697,040-byte executable was copied after cleanup to
`artifacts/accounting-ablation/task2/held-llmgw`; source and copy SHA256 are both
`e16ba39c54fcec9bc5ce4fb4039d551dccdd5cdfd3bec9e0a7c412e58bd4364f`.

All benchmark and gateway PIDs are gone. No `llmgw` listener, benchmark/Pi
Python, Cargo/Rust compiler, matching temporary directory, or owned runtime
remains. The parent full five-pair independent audit is the next gate; no report
or Native wizard work starts before that result.
