# Core Task 6 independent quality review

**READY FOR TASK7, limited to Task 6 quality. Product returned HOLD.**

No actionable Critical, Important, or Minor issue was found in the frozen Task 6
change after source inspection and three independent discriminating tests in
debug and release. This is not whole-product release approval, a performance
result, or authorization to change the held product.

## Strengths

- `product/src/admission/queue.rs:26`, `:115`, `:153`: the new queue owns the
  existing ledger, so head eligibility and actual reservation cannot race with a
  different quota owner. The selection loop makes progress only by removing an
  admitted ticket. Root FIFO, RR cursor, and sticky barrier state have one small,
  explicit owner rather than separate asynchronous schedulers.
- `product/src/admission/mod.rs:174`, `:188`, `:261`: notification registration
  precedes state inspection; there is no mutex held across an await. All result
  cell access follows queue-lock then ticket-lock order. The returned Hold is
  created after releasing the queue lock. Cancellation directly removes a pending
  ticket or its admitted-unpolled reservation, avoiding reentrant Hold destruction.
  Manual clock mutation releases its own lock before notifying and never takes
  the queue lock. No inverse lock path was found in callers.
- `product/src/admission/queue.rs:90`, `:145`, `:163`: original enqueue age and a
  non-reused ticket sequence make barrier identity clear. Bypass increments occur
  only on actual resource bypasses; fit RR turns do not accumulate false debt.
  Every queued deadline is pruned before selection, including nonheads. Past age
  thresholds are excluded from future timer deadlines, preventing immediate
  self-wake loops after a barrier forms.
- `product/src/config/validate.rs:328`, `product/src/server.rs:275`,
  `product/src/admission/mod.rs:218`, `product/src/metrics.rs:168`: typed public
  configuration also bounds the 16 root identities. Queue memory, status labels,
  and the 64 pending result cells are bounded independently of arbitrary session
  headers. Typed status preserves wide quota sums as decimal strings and uses the
  existing authenticated control boundary without request-level labels.
- The change has no added dependency, worker service, external state store, or
  speculative policy layer. The new production file is focused on scheduling.
  Tests exercise the real Admission/Queue/Ledger and socket callers, not a second
  scheduler model. Runtime documentation distinguishes exact synthetic costs,
  HTTP estimation, provisional admission, actual starts, and future Task 7/native
  work.

## Issues

### Critical

None found.

### Important

None found.

### Minor

None found. No speculative optimization or formatting preference is raised as a
Task 6 acceptance issue.

## Direct independent verification

The three tests in `src/lib.rs` import the unchanged production library using a
path dependency. They use synthetic exact costs and either ManualClock or paused
production Tokio time. They open no sockets and make **zero actual HTTP attempts**.
The raw race log's word `ingress` refers to library `acquire` calls only.

| Independent test | Observed result in each final profile |
|---|---|
| `parallel_cancel_release_start_and_snapshot_preserve_joint_ownership` | Four runtime threads, eight rounds, 16 initial holds and 64 waiting tickets per round. A barrier releases cancellation of 32 tickets, completion/start of the other 32, initial Hold drops, repeated snapshots, and clock notifications together. All 256 surviving waiters complete, 256 are canceled, eight overflow calls reject, and eight refills succeed: **656 library acquire calls**, **328 start calls** total. Active never exceeds 16; final active/held are zero; starts, retained debit entries, RPM and TPM equal the known completed work. Each refill adds exactly one cleanup. |
| `full_ledger_and_full_queue_have_no_repeated_idle_wakes_and_reclaim_all_holds` | 8,192 started/completed debit entries plus 64 waiters across 16 roots. Exactly 64 age-timer wakes, **zero additional wakes for the next ten simulated seconds**, and 64 quota-expiry wakes. One poll admits 16 joint provisional holds; dropping all futures releases those **16 exactly once** and removes 48 pending tickets. Starts stay 8,192, cleanups move from 8,192 to 8,208, active/held/retained become zero, and a full-budget refill succeeds. |
| `simultaneous_quota_expiry_and_queue_deadlines_cannot_start_expired_work` | A previously started RPM debit expires at precisely the 64 waiting tickets' deadlines, after an age barrier has formed. All 64 return Deadline before any new reservation; starts/cleanups remain one, all debit/held/active fields become zero, and a new full-capacity request succeeds. |

The race's cancellation cleanup count is intentionally not a single fixed number:
some canceled tickets can already have obtained a provisional reservation before
their future is dropped. Every round checks the legitimate 48–80 cleanup range,
zero residual active/held state, and exactly 40 starts/40 retained debit entries
before refill. Raw logs preserve each observed count. This is not claimed as an
exhaustive interleaving proof. The saturated test supplies a separately controlled
exact cleanup count for the admitted-unpolled transition.

There are **three unique tests**, each passing in both final profiles, with no
assertion failures, ignored cases, fixture repairs, or product edits. The initial
debug run also passed; its log is retained rather than added to the unique test
count. After formatting only reviewer-owned source, final debug and release ran
with `--offline --locked`. The lock retains every product package, version,
source and checksum; the only extra package is this review crate.

The final tests perform 8,979 library acquire calls and 8,521 library start calls
per profile across all three tests. These are policy/lifecycle observations, not
HTTP throughput, latency, provider quota compliance, or useful model completions.

## Source-supported assessment and limits

Inspected the nine changed existing files and both new files, plus actual
protocol route, stream worker, shutdown, and public configuration callers. The
root/research instructions, README, HARNESS, work items, reviewed design §5–7,
Core Task 6 plan, runtime contract, implementer report, and completed SPEC PASS
were read. The prior 161-test/profile reports and parent's held-binary Pi,
fairness, and stream checks remain separately attributed evidence; this reviewer
did not rerun or add them to its independent count.

The queue has at most 16 heads, 64 pending tickets, and the pre-existing 8,192
ledger entries. Each head check currently scans that bounded ledger, and shared
notifications may cause several waiters to reevaluate. Source inspection and the
saturated timer probe establish finite work and no persistent idle wake loop in
that scenario, **not** a latency or CPU budget. Task 8 should measure root count
and retained ledger occupancy in its planned performance comparison. No new
optimization or Task 6 framework is requested without measurements.

Task 7 retry/cooldown and aggregate retained-response/RSS work, native CLI,
cross-OS execution, actual provider behavior, workload performance, and universal
bounded waiting are outside this quality verdict. No user configuration, model,
API, service/startup setting, secret, reference clone, or Git state was touched.

## Identity and reproduction

`identity-before.json` and `identity-after.json` verify all **38 source files**
against the two parent manifests and the implementer manifest, and both held
binaries:

- Debug: `bc451f4bdfdac9c1f5c0e1de6200b17092ced8d2ab7871f0cf4e787524a89b56`
- Release: `65ca882cf0f299ccc85ba1651500193956dce441951eacdb45e17a231c4c896c`

Compared with accepted Task 5, nine existing files changed and only
`product/src/admission/queue.rs` and `product/tests/fairness_contract.rs` were
added. Product manifests and locks are unchanged. Fingerprints identify reviewed
files and binaries; they do not alone prove held-binary build provenance.

Run from the research root:

```sh
python3 evidence/task6-quality-review/identity.py identity-reproduced.json
CARGO_TARGET_DIR="$PWD/evidence/task6-quality-review/build" cargo test --offline --locked --manifest-path evidence/task6-quality-review/Cargo.toml -- --nocapture
CARGO_TARGET_DIR="$PWD/evidence/task6-quality-review/build" cargo test --release --offline --locked --manifest-path evidence/task6-quality-review/Cargo.toml -- --nocapture
```

`debug-initial.log`, `debug.log`, `release.log`, `run-summary.json`,
`lock-comparison.json`, the reviewer manifest/lock/probe, and identity files are
retained. Only this review's disposable `build/` directory was removed after
recording final executable hashes. Product targets were never rewritten.

## Assessment

**READY FOR TASK7 — Task 6 quality only.** The new scheduling state is bounded and
cohesive, ownership is explicit, and the focused race/timer/deadline counterexamples
passed in both profiles. No Task 6 edit is required. **HOLD returned to the parent**
for its final acceptance and next-task decision.
