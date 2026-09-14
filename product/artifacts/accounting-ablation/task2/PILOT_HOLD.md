# Accounting ablation Task 2 first pair — PILOT_HOLD

Status: **PILOT_HOLD.** `execution_finished=true`; owned runtime is zero. The
remaining eight runs have not started.

The exact pilot-only command completed with exit code 0. The checkpoint is
`accounting_pilot_completed`, with 2 completed runs / 1 completed pair out of
10 planned runs / 5 planned pairs, and `matrix_complete=false`.

Both arms used the ordinary empty-feature binary SHA256
`e16ba39c54fcec9bc5ce4fb4039d551dccdd5cdfd3bec9e0a7c412e58bd4364f`,
the same submitted schedule, and normalized settings differing only in
`accounting` (`reserved` versus `actual`). The fixed-time completed delta is
actual minus reserved = 1. This is one-pair synthetic evidence and is not a
correctness gate or a multi-seed efficiency conclusion.

Direct runtime verification found complete unique 100-request terminal
denominators in each arm, terminal and unique attempt linkage, exact cutoff
classification, valid completed payloads, and valid usage classification. All
observed numerical generation usage matched the linked mock body and submitted
actual ratio. Metadata usage was `not_applicable`; cancellations, rejections,
and timeouts were `not_observed` as applicable. Client-observed usage is not
proof of gateway internal refund behavior.

Source, archive, binary, and frozen usage-basis identities remain on HOLD. The
benchmark PID 35122 and gateway PIDs 35133 and 37549 are gone. No matching
`llmgw` process, listener, benchmark Python process, or `llmgw-bench-*` temporary
directory remains.

The parent reported the independent first-pair audit and five forged-copy
negative cases PASS. This runtime owner will not resume until the parent
explicitly accepts cleanup and sends the continuation instruction.
