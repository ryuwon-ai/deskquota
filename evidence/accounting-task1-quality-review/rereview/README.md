# Accounting Ablation Task 1 — QUALITY rereview

**PASS for the held QUALITY-fix source. Q1 and Q2 are resolved; no remaining
Critical, Important or Minor finding from this review. `execution_finished=true`
and HOLD.** This reviewer does not begin Task 2 or make product changes.

## Scope and source basis

This is the same reviewer's focused rereview of the whole-task QUALITY findings
after the implementer's Q1/Q2 corrections. The earlier full five-file review,
original counterexamples and failure artifacts remain intact. The reviewed
skills/template and Task 1 scope are unchanged.

The independently derived `independent-source.diff` confirms exactly three files
changed from the SPEC-fix hold: `product/scripts/benchmark_accounting.py`,
`product/tests/test_benchmark_accounting.py`, and
`product/docs/benchmark-method.md`. All 59 current source files match the new
manifest and archive. The runner, HTTP helper, Mock/workload, Rust/Cargo and
ordinary-vs-benchmark startup boundary did not change in this correction.

- Source manifest SHA256:
  `f77ceac838cdf83fb5f7f0d7bd63536a3a9c62ecbecccf932c0a07c911a72304`
- Source archive: 174,816 bytes, 60 members, SHA256
  `e057f32e5eb37c44eaa489fe15ab0f9846fde283261b7f9c690a4eefb4d5c9c4`
- Ordinary native binary SHA256 remains
  `e16ba39c54fcec9bc5ce4fb4039d551dccdd5cdfd3bec9e0a7c412e58bd4364f`.

## Finding closure

| Finding | Current implementation | Same-trigger direct result |
|---|---|---|
| Q1: persisted runtime evidence bypassed resume validation | `benchmark_accounting.py:264–271` requires both initial and final status and reuses `validate_runtime_config` for each. | Original accounting contradiction, fingerprint contradiction and reserved-to-actual relabelling copies all reject in actual `resume_preflight`, before paired evaluation or any new arm. |
| Q2: NaN terminal became a drain completion | `benchmark_accounting.py:288–304` rejects nonfinite start/duration/ended values and nonfinite or overflowing cutoff arithmetic. | Original rehashed NaN terminal copy rejects with `terminal ended_s must be finite`; it no longer produces the incorrect -1 paired delta. |

The solution follows the bounded recommendations: reuse the existing worker
check, add a small finite-number predicate, and align successful fixture status
with the ordinary worker's recorded structure. It adds no generalized protocol
engine, compatibility path, new dependency or product runtime mechanism.

## Direct verification

Executed from the research workspace:

```sh
PYTHONDONTWRITEBYTECODE=1 python3 evidence/accounting-task1-quality-review/rereview/probe_adjacent.py
```

The driver runs `replay_original.py`, which preserves the original probe's seven
case triggers and old, hash-verified smoke inputs; only its new output/root path
is adjusted. These historical copies test semantic rejection in isolation and
are not adopted into a current measurement session. No prior source identity is
silently reused by a resumed full benchmark.

`probe-resume.json` and `probe.log` record:

- Unchanged original smoke pair: accepted, delta 0.
- Runtime accounting contradiction: rejected with initial-status accounting
  mismatch.
- Runtime fingerprint contradiction: rejected with initial-status fingerprint
  mismatch.
- Reserved run relabelled actual with regenerated declared TOML: rejected with
  initial-status fingerprint mismatch.
- Nonfinite terminal NaN: rejected.
- Equal-valued float usage and false finite cutoff flag: still rejected.

`adjacent-results.json` records **26 additional focused cases**:

- Both initial and final status: missing, null, mismatching accounting and
  mismatching fingerprint reject (8 cases).
- Start, duration and terminal time: NaN, positive/negative Infinity and bool
  reject (12 cases).
- Finite float sum overflow and large-integer-to-float cutoff conversion
  overflow reject (2 cases).
- Exactly-at-cutoff and immediately-after-cutoff finite completion cases remain
  valid (2 cases).
- Both current held smoke raws pass current validation; independently read raw
  hashes and saved initial/final accounting/fingerprint agree (2 cases).

The current held smoke's entire paired evaluation recomputes identically to its
manifest. These are **direct artifact/pure-validation observations**, not a new
smoke execution. The original report's architectural/usage/pilot strengths remain
applicable; no additional concrete regression was found in the small correction.

## Retained execution evidence and limits

The implementer's actual logs were read: 29 unit tests, self-check and one final
accounting smoke passed. Each smoke arm recorded 20 submitted, 17 completed,
3 scheduled cancellations and 20 mock attempts. These counts are retained
implementer runtime evidence, not this reviewer's newly executed tests and not
required fixed attempt counts. The new smoke manifest SHA256 is
`e1931ecd62031d39b38c163d1330de73036f7d2266aacaf7d8f11d5ba147e0fd`.

The original focused RED and intermediate assertion-expectation failure remain
preserved. Routine full suites/smoke were not repeated because original-trigger
and adjacent copied-artifact checks resolve the remaining review questions.
No full quota matrix, performance advantage, actual provider behavior or internal
refund proof is claimed. Client-received usage remains a separate observation.

## Integrity, cleanup and assessment

`before.json` records 59 held source identities and 161 preserved evidence/artifact
files, including prior QUALITY/SPEC evidence, original pilot files, retained
binaries and the new hold. `final-integrity.json` confirms every recorded SHA and
byte count is unchanged during this rereview; whole build trees were not hashed.

Owned cleanup: zero gateway/model children, zero network listeners and zero
temporary runtime directories were created. All copied fault fixtures, scripts
and logs were written only under this new `rereview/` directory and are retained.
No product source edit, build, Rust/Pi suite, old measured-binary execution,
real/paid API call, model download/start, user config/autostart/OS registration,
Git action or Task 2 execution occurred.

**Ready for the parent's next gate? Yes — QUALITY PASS for this exact held
source.** Both original findings now reject their original counterexamples, the
positive controls and finite timing boundary still work, and no new scoped
quality issue remains. **`execution_finished=true`; reviewer HOLD.**
