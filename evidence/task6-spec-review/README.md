# Core Task 6 independent spec review

**SPEC PASS — product remains HOLD for the separate quality review.**

Fresh reviewer compared the accepted Core Task 6 plan, design §5 and supplied
clarifications to the actual Queue, Admission, Ledger, configuration, HTTP route,
server, worker, control and metrics callers. No Task 6 omission, contradictory
policy, or unrequested framework/dependency/policy was found. This verdict does
not accept the entire product or Task 7/native experience.

## Direct independent execution

Eight independent tests pass in **debug and release**, with unchanged product
sources and a separate Cargo target. Tests call the production public library;
no independent scheduler model or product test-cost header is substituted.

| Test | Observed contract |
|---|---|
| `nonhead_expiration_reclaims_full_queue_without_head_progress` | A root FIFO nonhead expires at 2 seconds while a live head stays blocked; the total 64 waiting ceiling rejects the 65th ticket and admits a replacement into the reclaimed queue position. All canceled waiters start zero attempts; only the initial provisional hold cleans up. |
| `admitted_unpolled_cancel_retains_no_quota_and_does_not_rewind_cursor` | Another waiter drives the selected ticket to provisional admission without polling its owner. The hold survives beyond 60 seconds, its cancellation releases exactly once, and the next root starts with original wait start/deadline retained. Final active=0, starts=1, cleanups=3, TPM debit=80, held=0. |
| `two_full_sixteen_root_rounds_do_not_create_false_bypass_barrier` | Two queued rounds across all 16 roots preserve actual-admission RR cursor order (roots 1…15,0 twice) without treating fit RR turns as resource bypasses. |
| `younger_blocked_trigger_protects_oldest_fit_and_survives_trigger_removal` | Eight actual admissions bypass a younger TPM-blocked head. The older currently fitting root becomes the selected barrier; canceling the younger trigger while the eighth hold is active does not let the late next-cursor root jump ahead. |
| `production_clock_age_timer_and_quota_expiry_preserve_fifo_original_age` | Paused Tokio production clock permits a small request before 5 seconds, blocks it after age 5 seconds, preserves second FIFO entry age after head cancellation, and releases the heavy at quota expiry. |
| `idle_waiter_is_woken_by_production_age_timer_without_external_notification` | A counting waker sees no wake before the age timer, sees the timer wake after 5 seconds, then sees no repeated wake for 10 seconds after rescheduling the aged barrier. This discriminates against past-deadline timer spinning. |
| `invalid_roots_and_impossible_cost_cannot_create_a_queue_share` | Direct public config rejects root count 17, duplicate, empty, unsafe, non-ASCII and overlength IDs. Admission rejects invalid index and impossible total cost without an active hold. |
| `owned_wire_metadata_shares_provisional_rpm_and_safe_authenticated_status` | Independent owned TCP upstream plus real server HTTP: data token cannot read control status (401), Origin is rejected (403), provisional models hold reports active=1/RPM debit=0 and blocks second root via shared RPM. Both metadata requests debit TPM=0. Releasing prestart gate produces exactly one upstream attempt; advancing the synthetic quota clock produces the second. Arbitrary child header creates no extra root status entry. |

Seven tests are deterministic library/policy checks (including two using paused
production Tokio time). The eighth uses actual HTTP sockets with explicit manual
quota time and prestart gate. It starts **2 upstream HTTP attempts per profile**.
These attempts are not exact-cost generation or performance evidence. No real API,
model, persistent config, login, startup service, or reference clone was touched.

The first six tests also passed before adding the independent timer-wake and wire
checks. Their initial log is preserved. There were no assertion failures, fixture
repairs, or product edits in this review. Missing optional product README/AGENTS
paths during discovery were not treated as product failures; root and research
instructions, README, HARNESS, work-items and runtime contract were read.

## Actual code compliance map

Paths below are relative to `product/`.

- `src/admission/queue.rs:47`: validated roots, impossible-total-cost rejection,
  global 64 waiting tickets and checked non-reusing queue sequence.
- `src/admission/queue.rs:90`: sticky (root, sequence) barrier; any 8-bypass/5-second
  trigger selects oldest original timestamp among **all** current valid heads.
  Capacities are immutable and impossible costs are rejected before insertion.
- `src/admission/queue.rs:115`: all queued deadlines, including nonheads, are
  removed before selection; each selection considers each root head once; another
  selection occurs only after admission progress. Fit probes do not reserve.
- `src/admission/queue.rs:145`: only non-fit other-root heads gain bypasses;
  actual joint admission updates the cursor. A barrier pauses other admissions.
- `src/admission/mod.rs:158`: the single Queue owner contains the existing Ledger;
  enabled notification registration closes the wake race; waiters await notify
  or the next quota/age/deadline timer.
- `src/admission/mod.rs:188` and `:261`: take reservation ID out of the ticket,
  construct Hold outside the queue lock; Ticket Drop cancels the stored ID using
  the existing owner directly. No RAII Hold is dropped under its owner's mutex.
  Original enqueue/deadline survives transition to Hold.
- `src/admission/quota.rs`: existing rolling accounting, provisional holds,
  actual-start RPM, fixed 8,192 storage, wide sums and non-wrapping IDs remain in
  the same ledger. Candidate checking is split from joint `admit`, with no second
  quota owner. No quota constants, tokenizer or HTTP estimation behavior changed.
- `src/server.rs:275`, `src/config/validate.rs:328`,
  `src/protocol/mod.rs:132`, `src/server.rs:671`: both config parsing and public
  typed server start bound root identities; data route supplies registered index;
  every endpoint uses the same admission path and error mapping.
- `src/transport/stream.rs:159`: unchanged worker starts Hold at the first poll of
  the upstream send. Existing EOF-owned cleanup/drain path is unchanged.
- `src/admission/mod.rs:218`, `src/metrics.rs:168`, `src/control.rs:19`: bounded
  typed root queue status, provisional active, reason/barrier/estimate/quota modes,
  decimal-string wide sums, and existing control credential/Origin restrictions.

The required heavy→light→heavy, heavy→heavy→light, continuous-small/8-bypass,
5-second, cap1 nonpreemption, root+15-child, overflow/RST, deadline and metadata
traces exist in `tests/fairness_contract.rs` and were inspected for actual use of
the production owner or TCP fixture. The initial heavy-light trace releases
provisional holds; the heavy-heavy-light and continuous-small traces explicitly
start and finish heavy holds. These have different evidence meanings and are not
combined into an HTTP completion/performance claim.

The existing 161-test logs per profile were independently parsed: counts
5/0/19/22/12/32/27/44 total 161 with no FAILED entries, recorded in
`implementer-log-audit.json`. Those are inspected implementer execution logs,
**not** an independently rerun 161-test suite. Parent's held-binary fairness,
stream and Pi probes remain separate evidence. Independent new probes here cover
8 unique cases per profile, not 16 unique cases.

## Identity and reproduction

`identity-before.json` and `identity-after.json` each verify all **38 sources**
against the two parent manifests and implementer manifest, plus both held binaries:

- debug: `bc451f4bdfdac9c1f5c0e1de6200b17092ced8d2ab7871f0cf4e787524a89b56`
- release: `65ca882cf0f299ccc85ba1651500193956dce441951eacdb45e17a231c4c896c`

Comparison to accepted Task 5: nine existing files differ and only
`src/admission/queue.rs`, `tests/fairness_contract.rs` are added. Product Cargo.toml,
Cargo.lock, transport, protocol estimation and usage observer sources are unchanged.
No Git repository was assumed or Git mutation executed.

From the research root:

```sh
python3 evidence/task6-spec-review/identity.py identity-reproduced.json
CARGO_TARGET_DIR="$PWD/evidence/task6-spec-review/build" cargo test --offline --locked --manifest-path evidence/task6-spec-review/Cargo.toml
CARGO_TARGET_DIR="$PWD/evidence/task6-spec-review/build" cargo test --release --offline --locked --manifest-path evidence/task6-spec-review/Cargo.toml
```

`Cargo.lock` started from the product lock and was resolved offline for this
review package. `lock-comparison.json` confirms every product package/version/source
is identical; only `task6-spec-review` itself is added. `run-summary.json` records
both final logs and reviewer executable hashes. The held product binaries were
never overwritten or executed by these new library probes. The review's disposable
`build/` was removed after execution; manifest/lock/test sources/logs are retained.

## Limits and handoff

No finding requires a Task 6 edit. Retry/cooldown, native identity-aware status CLI,
global RSS/slow-reader retention, cross-OS behavior, real provider quota accuracy,
performance superiority and general bounded waiting remain outside this verdict.
The eight focused tests are not an exhaustive race proof. Unchanged fingerprints
identify reviewed files; they alone do not prove held-binary build provenance.
**SPEC PASS, HOLD: continue to the fresh independent quality review.**
