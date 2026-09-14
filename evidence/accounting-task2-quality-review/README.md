# Accounting Task 2 QUALITY review

Verdict: **PASS.** No actionable Critical, Important, or Minor finding in the reviewed Task 2 measurement, independent auditor, supplementary observer, or corrected result report. This accepts the bounded research deliverable; it is not a production/build certification or permission to publish.

## Scope and held identity

Reviewed Task 2 of `docs/superpowers/plans/2026-09-12-accounting-ablation.md`, `reports/accounting-ablation-plan.md`, the actual 293-line independent auditor, 69-line saved-data observer, all ten raw run files and their journals, recorded first-pair gate/resume evidence, current report, and current/historical HOLD evidence. Followed the requesting-code-review template. No Git range exists in this research workspace; source/archive identities substitute only for identifying the artifact under review.

- Product source manifest: `f77ceac838cdf83fb5f7f0d7bd63536a3a9c62ecbecccf932c0a07c911a72304`.
- Both ordinary measured and held binaries: `e16ba39c54fcec9bc5ce4fb4039d551dccdd5cdfd3bec9e0a7c412e58bd4364f`.
- Final pilot manifest: `bfe367592eae5953edf0f1bb64dc808c7864f78ce41b48137f47d9dc9901c41c`.
- Independent auditor: `24f5af55558393ba3bafdb840d7cca5be702a1460c5edad6b51040bd3968a234`.
- Corrected report: `67c360ba0f31093b3b02bc1c1cd8bf1da43836926734788e977dbfe52e222a44`.

## Strengths

- The reader separates pinned schedule inputs from newly measured control outcomes and imports stdlib only. Fixed source/binary/config serialization, runtime fingerprints, full submitted trace, terminal denominator, raw/journal linkage and paired evaluation are independently checked (`scripts/audit-accounting-results.py:106-268`). This is a compact artifact-specific reader rather than an unnecessary general benchmark framework.
- The numeric usage path requires actual u64 integers and independently reconstructs the fixture body size and input/output cost. Fixed completion uses actual end timestamps with the specified inclusive cutoff; malformed values and failures yield an explicit failure result rather than a favorable summary (`scripts/audit-accounting-results.py:140-182,271-289`). Two fresh real-CLI copies demonstrate rejection of flipped cutoff flags and an integer usage value altered by one, after raw hashes were recomputed. Positive originals still pass.
- Fixed and drain outcomes are kept separate, all 500 submissions per arm stay in the denominator, and the six subgroup tables make the remaining long/root 0 timeouts visible. Success-only latency and timing-jitter repetitions are explicitly qualified. Client-observed usage, gateway-reported queue reasons, provider refund, causal scheduler proof, and default-policy decisions are not conflated (`reports/accounting-ablation-results.md`).
- The saved-data observer pins first-pair artifacts, has no gateway execution path and refuses to overwrite existing output. Its report labels sample counts as samples rather than elapsed-time proportions and does not manufacture counterfactual throughput (`scripts/observe-accounting-first-pair-queue.py:17-69`).

## Issues

### Critical
None.

### Important
None.

### Minor
None.

## Directly verified

- Fresh full auditor CLI: exit 0, 10 runs, 5 paired seeds, 1,000 ingress; 58 measurement-source files (excluding `.gitignore`) and fixed ordinary binary identity accepted. The full 59-file HOLD preservation is separate evidence from the held manifest and preservation checks. Evidence: `auditor-cli.json`, `independent-audit.json`, `hashes-before.json`, `hashes-after.json`.
- A second stdlib-only review calculation reconstructed every attempt's serialized fixture byte cost, every completed generation's numeric usage, completed metadata semantics, all failure usage states, rejected-client/mock linkage, scheduled cancellation, exact cutoff and all outcomes. All six actual report subgroup table rows were parsed and checked cell by cell. Fixed completions are 285/290, final completions 397/410, and all five fixed deltas are +1. Evidence: `verify.py`, `raw-recalculation.json`.
- Both new rehashed copied-data negative probes exited 1 for their intended reason (`cutoff flag mismatch`, `usage/fixture mismatch`). Copies and failure results are retained beneath this review directory. Evidence: `negative-cli.json`.
- Seed 1 raw and journal bytes still match their original independently audited first-pair identities, supporting unchanged reuse. Saved queue counts independently match 298/276/283/274 in each arm. Evidence: `first-pair-preservation-and-observation.json`.
- Before/after hashes of 108 protected files are identical, including current/historical report HOLD, original failure records, measured source/binary/raw/journal material. Evidence: `hashes-before.json`, `hashes-after.json`, `verification.json`.

## Recorded evidence and limits

The exact pilot-only and subsequent resume invocations, first-pair correctness gate, pre-resume negative checks, cleanup and full-matrix records were inspected. Live process/port/temp cleanup is supported by existing owner and parent evidence, including the recorded 12 absent owned PIDs; this reviewer did not remeasure running processes. Prior Rust usage regression/source evidence remains prior evidence, not a fresh suite or build attestation. No universal resistance to coordinated fabrication is claimed by a consistency auditor.

No gateway/model execution, build, suite, paid/external API, user configuration, OS registration, product edit, raw edit, Git action or native wizard work occurred. All review writes are in `evidence/accounting-task2-quality-review/`.

## Recommendations and assessment

No fix is required for Task 2 acceptance. Retain the fixed-window and subgroup limitations when reusing these results. Native wizard work can follow the parent's acceptance of this review; unrelated performance/default-policy work remains a separate scope.

**Ready to accept Task 2: Yes.** The recorded experiment and independently recalculated report support the narrow claim, and the auditor's relevant corruption checks work through its real CLI. `execution_finished=true`; **HOLD** remains in effect for the reviewed product artifacts.
