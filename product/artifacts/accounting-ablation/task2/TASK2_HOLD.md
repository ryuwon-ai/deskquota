# Accounting Ablation Task2 HOLD

Status: `TASK2_HOLD`  
Execution finished: `true`  
Owned runtime: `0`

The full accounting matrix completed with the exact resumed command and reused
the accepted seed-1 pair without launching it again. The final manifest is
`completed_accounting_matrix`, with 10 runs, 5 paired seeds, 1,000 ingress, and
SHA-256
`bfe367592eae5953edf0f1bb64dc808c7864f78ce41b48137f47d9dc9901c41c`.

The independent five-pair audit passed. Its evidence is
`research/evidence/accounting-ablation-parent-audit.json`, SHA-256
`632c41e4e2365a9ec99b4f9f825dcd5893196bfc74cbaf04d354549257280241`.
The independent PID and binary hold check also passed at
`research/evidence/accounting-ablation-parent-runtime-hold-check.json`, SHA-256
`4c6fefb46e8a464fd68fdbe2876b32165946362ba87153423b1381afbae5a033`.

The result report is `research/reports/accounting-ablation-results.md`, SHA-256
`361a340ef367dffba18da82449b5987bb6fcb3563b55bb0ac98bb62b683e9f4f`.
It leads with the paired 300-second completion deltas `[+1,+1,+1,+1,+1]`,
retains all terminal outcomes and 429s, separates drain and success-only
latency, reports roots, lengths, attempts, and usage provenance, and limits the
claim to one fixed synthetic fixture repeated with timing-jitter seeds.

The final reread verification passed all 16 checks at
`task2-final-verification-reread.json`, SHA-256
`7c0bfd7f938257be6dc38542e34f93527392c60b845ea8dc4b30d4367234acb4`.
It freshly verified the 59-file source hold, 8-file usage basis, source archive,
manifest, report requirements and links, independent audits, recorded PIDs,
matching processes, listeners, prior temporary-directory cleanup, and both
binary identities. The first final-verification attempt is preserved at
`task2-final-verification.json`, SHA-256
`6d1de157d2988d74bc55595fdeaf6c58cdde13b8876a4fd70526599568ea9c69`;
its sole failure was a report checker looking for the English word `success`
while the report uses the Korean phrase for final successful requests. The
reread corrected only that checker and repeated the full verification.

The ordinary measured binary and exclusive held copy are both executable,
8,697,040 bytes, SHA-256
`e16ba39c54fcec9bc5ce4fb4039d551dccdd5cdfd3bec9e0a7c412e58bd4364f`.
The 59-file source manifest remains SHA-256
`f77ceac838cdf83fb5f7f0d7bd63536a3a9c62ecbecccf932c0a07c911a72304`;
the 174,816-byte source archive remains SHA-256
`e057f32e5eb37c44eaa489fe15ab0f9846fde283261b7f9c690a4eefb4d5c9c4`.

No product source, Rust, dependency, workload, configuration rule, or measured
binary changed. No build, test, Pi, real API, model, user configuration, OS
registration, Git, or Native wizard action was performed. The earlier
`MATRIX_HOLD.md` and `matrix-verification.json` bytes remain unchanged as
gate-time evidence, including their intentionally pending parent-audit field.
