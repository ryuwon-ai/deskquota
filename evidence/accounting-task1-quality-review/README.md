# Accounting Ablation Task 1 — fresh QUALITY review

**Assessment: With fixes / QUALITY not passed. One Important issue, one Minor
issue, no Critical issue. `execution_finished=true`; HOLD before any fix or Task 2.**

This is the fresh whole-task quality review after SPEC round 2 PASS. It follows
the `subagent-driven-development` quality gate and the standard
`requesting-code-review/code-reviewer.md` review template. It is not a repeat of
the four-file SPEC correction review. No product implementation was changed.

## Review basis

- Reviewed Task 1 of `docs/superpowers/plans/2026-09-12-accounting-ablation.md`,
  approved `evidence/accounting-plan-review-round2.md`, applicable root/workspace
  AGENTS and README/HARNESS entry points, all five affected source files and the
  retained SPEC rereview evidence.
- No Git repository/worktree was created or used. The native baseline archive
  SHA256 is `d84f5b1ee94fc2a57a697b92007e2b2715168735a97ef33c2209f4e01f6d5c33`.
  The HEAD archive SHA256 is
  `8fa679bc16b651912e6ac68d3babc8d5958903782db12278633ad9195be5e255`.
  `whole-task.diff` was independently derived from the native baseline archive
  versus actual held source, with all 59 HEAD file hashes checked.
- The complete delta is `product/scripts/benchmark.py`,
  `product/scripts/benchmark_http.py`, new `benchmark_accounting.py`, new
  `product/tests/test_benchmark_accounting.py`, and
  `product/docs/benchmark-method.md`. Rust, Cargo, Mock server behavior, workloads,
  ordinary binary and old benchmark binaries are outside this delta.
- The earlier 26-unit/self-check/smoke PASS was inspected as **retained evidence**,
  not relabelled as this reviewer's execution. It was not repeated solely to
  refresh counts. This reviewer ran the seven focused copied-artifact probes
  below, without launching a gateway or a model.

## Strengths

- Arm selection, configuration construction, provenance rules and paired
  statistics are localized in one Python module; the existing runner retains
  orchestration and owned-process lifecycle. This is a reasonable separation
  for the fixed two-arm experiment. No framework, new dependency, Rust change
  or speculative abstraction is needed on the evidence reviewed.
- Written TOML is parsed and hashed from actual bytes before ordinary startup;
  authenticated worker fingerprint/accounting is checked before return.
  Startup owns the acquired child through guarded cleanup, including repeated
  cancellation. The separate baseline example retains its health-only boundary.
- Full-schedule pilot/resume identity, exclusive artifact paths and preflight
  validation prevent silent adoption of incomplete/orphan results. One-seed
  checkpoint and duplicate-seed behavior have explicit tests.
- Numerical usage remains a client observation with exact integer/u64 checks;
  valid payload completions are preserved when usage auditing fails. The mock
  response/workload remains unchanged, and the documentation does not claim
  that received usage alone proves an internal refund.
- Fixed-cutoff counts and post-cutoff drain completions are separated. The full
  terminal denominator, root/length results, scheduled cancellations and actual
  attempt counts are retained without favorable-result filtering.
- Tests cover substantive boundaries: written-file disagreement, runtime
  startup mismatch, cancellation ownership, resumed first-pair reuse, rehashed
  usage, exact cutoff and invalid CLI combinations. File/line counts alone are
  not a maintenance finding.

## Issues

### Critical

None found in the reviewed Task 1 delta.

### Important — Q1: persisted runtime evidence is not checked on resume

**Location:** `product/scripts/benchmark_accounting.py:252–259` and its return at
`:330`, invoked by `product/scripts/benchmark.py:438–453`. The existing runtime
check at `benchmark_accounting.py:167–174` is only called during live startup at
`benchmark.py:232–233`.

**Trigger and direct observation:** Starting from copied, hash-verified final
smoke artifacts, change only the actual run's
`initial_status.admission.accounting` to `reserved`, or only its stored
`initial_status.identity.fingerprint` to a mismatching digest, and re-register
the copied raw file's SHA. Both `resume_preflight` and `paired_summary` accept
the contradictory run. The probe also copies the reserved run into the actual
slot, updates its arm/ID and generated declared actual TOML metadata, and leaves
its recorded initial/final runtime state reserved. That copy is accepted as a
complete pair too. The regenerated config passes structural checks, while the
already-recorded worker evidence directly contradicts it.

**Why it matters:** The new live startup guard establishes actual worker
configuration, but that evidence is lost from the persisted-run validation
contract. A mislabelled or inconsistently rewritten saved run can pass the
preflight gate and enter an ablation comparison even when its own runtime
evidence says it used the other policy. This is an internal consistency gap,
not a request for cryptographic attestation or protection against an attacker
rewriting every piece of evidence. The original unmodified smoke pair passes
and has consistent runtime evidence; no live wrong-policy execution was
observed in this review.

**Bounded recommendation:** Reuse `validate_runtime_config` against required
recorded initial/final ordinary-worker status in persisted accounting-run
validation, rejecting missing or contradictory evidence before any resumed
new arm. Keep the separate benchmark executable boundary unchanged. Add a
focused copied-artifact regression for these contradictions and include the
real status shape in successful resume fixtures; no new protocol or provenance
framework is needed.

**Evidence:** `probe-resume.json` cases `runtime_accounting_mismatch`,
`runtime_fingerprint_mismatch`, and `reserved_run_relabelled_actual`, plus their
preserved per-case copied manifests/raws. Each reports `accepted=true` and two
resumed runs despite the displayed contradictory runtime fields.

### Minor — Q2: a nonfinite terminal timestamp becomes a drain completion

**Location:** `product/scripts/benchmark_accounting.py:276–289`; the result is
counted at `:399–405`.

**Trigger and direct observation:** In a copied actual run, replace one completed
outcome's `ended_s` with `NaN`, set `within_measurement=false`, recompute its
summary, and re-register the raw SHA. Python's JSON writer/reader accepts this
nonstandard value. `type(ended) is float` passes, and `NaN <= cutoff` is false,
so resume and paired aggregation accept it. The actual-minus-reserved completed
count changes from **0 to -1**, treating an invalid time as a post-cutoff
completion.

**Why it matters:** The explicit timing validator silently assigns a measurement
category to an invalid numeric value. This is a persisted-data robustness gap;
the ordinary `time.monotonic()` execution path was not observed to produce NaN,
so this is Minor rather than a demonstrated live benchmark failure.

**Bounded recommendation:** Require finite start, duration and terminal times
before cutoff arithmetic, with a small NaN/Infinity negative fixture. Strict
JSON serialization can additionally fail early, but no broad schema engine or
timestamp-attestation system is required.

**Evidence:** `probe-resume.json`, case `nonfinite_terminal_nan`, with preserved
copied raw/manifest and the accepted derived delta of -1.

## Focused verification

Executed from the research workspace:

```sh
PYTHONDONTWRITEBYTECODE=1 python3 evidence/accounting-task1-quality-review/probe_resume.py
```

| Case | Actual result | Interpretation |
|---|---|---|
| Unchanged saved smoke pair | Accepted; delta 0 | Positive control |
| Saved runtime accounting contradicts actual config | Accepted | Q1 |
| Saved runtime fingerprint contradicts TOML hash | Accepted | Q1 |
| Reserved raw relabelled actual with regenerated declared config | Accepted | Q1 |
| One nonfinite terminal time | Accepted; delta -1 | Q2 |
| Equal-valued float usage | Rejected with u64 integer error | SPEC fix remains effective |
| False cutoff flag with unchanged finite timestamp | Rejected with cutoff mismatch | Existing timing check remains effective |

The probe calls the real current `resume_preflight` and `paired_summary`; it
does not mock their validators or claim that fault-injected data is a measured
result. All fixture changes are confined to this new evidence directory.

Source inspection found no additional concrete regression in the new usage
parser, ordinary-vs-benchmark startup branching, pilot loop or pairing formula.
General inherited harness cleanup/transport behavior was examined in context;
this review does not introduce unrelated Rust/Pi or legacy-binary findings.

## Assessment and HOLD

**Ready to proceed to Task 2? No — with the Q1 fix and focused rereview.** The
implementation is cohesive and no contradiction was found in the unmodified
retained smoke evidence, but
the promised strict resume gate should not accept contradictory runtime policy
evidence before the longer experiment starts. Q2 is a small adjacent robustness
improvement, not proof that current recorded measurements are corrupt.

Owned cleanup: the reviewer spawned **zero gateway/model children**, opened
**zero network listeners**, and created **zero temporary runtime directories**.
All copied fixtures, logs and review scripts are deliberately retained here.
`final-integrity.json` checks the 59 held sources and the retained artifact hashes
from the prior SPEC inventory, including old raw results and binaries, without
hashing whole build trees.

The first reviewer integrity script compared an artifact's SHA string against
the inventory's `{sha256, bytes}` object rather than its `sha256` member. Its
false mismatch output/assertion is retained as
`initial-audit-schema-error-final-integrity.json` and its initial summary. The
corrected audit checks SHA and byte count: **59 sources and 91 retained artifacts
match**, with no mismatch. This was a review-script error, not a product failure.

No implementation fix, product-source edit, build, old measured binary execution,
300-second quota run, Rust/Pi suite, paid/real API, model download/start, user
configuration/autostart change, OS registration, Git action or Task 2 was done.

**`execution_finished=true`. Reviewer HOLD; parent/implementer must not infer
QUALITY PASS from the earlier SPEC PASS.**
