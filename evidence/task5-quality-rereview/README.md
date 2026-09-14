# Core Task 5 quality re-review — READY FOR TASK 6

Reviewed 2026-09-12. The Important Messages ordering finding from the [initial fresh quality review](../task5-quality-review/README.md) is resolved. **READY FOR TASK 6 within the accepted scope.** No remaining actionable finding was found in this narrow correction review; the original full-boundary review remains applicable to the unchanged implementation. This is Task 5 quality acceptance, not production, native-setup, performance or provider-compliance acceptance. Reviewer remains **HOLD** after this verdict.

## Independent verification

The original five tests and their helpers are a **byte-identical prefix** of [tests/review.rs](tests/review.rs), confirmed in [comparison.json](comparison.json). Their original failing logs remain unchanged in `task5-quality-review/`. Two additional tests cover three independent wire cases using SSE event names without a JSON `type`, exercising the adjacent event-name fallback rather than merely duplicating the implementer's typed events.

| Independent check | Debug | Release |
|---|---|---|
| Messages start → stop → delta | Pass: original 352 reservation retained | Same pass |
| Messages stop → start → delta | Pass: original 352 reservation retained | Same pass |
| Normal start → delta → stop control | Pass: debit 2 | Same pass |
| Qualified usage followed by post-stop named start/delta usage, JSON type omitted (two cases) | Pass: original 352 retained | Same pass |
| Post-stop named start/delta without usage, ordinary notice and repeated stop | Pass: known debit 2 retained | Same pass |
| 20 contenders, 16 provisional slots, clock advance, 8 prestart RST cancellations | Pass: 20 cleanups / 12 attempts / 0 active / 0 held TPM, twelve reservations retained | Same pass |
| 8,192 retained entries, expiry with old active identity, fresh request and late `u64::MAX` usage | Pass: debt retained, no overflow, cross-settlement or duplicate cleanup | Same pass |

**7 tests passed / 0 failed per profile, exit 0.** Each profile covers 26 actual loopback ingress requests, 18 upstream attempts, eight prestart cancellations and one direct ledger scenario. The single-response Messages helpers check status, exact SSE body bytes, one attempt and one cleanup, then shut down before asserting final debit. The concurrent control also completes gateway shutdown. Manual monotonic time advances only the ledger seam; transport and task scheduling use real ephemeral IPv4 loopback sockets. These are correctness probes, not a load or latency benchmark.

Exact commands from `/Users/ryuwon/Desktop/ryuwon-project/llm-gateway-research`:

```sh
cargo test --offline --locked --manifest-path evidence/task5-quality-rereview/Cargo.toml --target-dir evidence/task5-quality-rereview/target -- --nocapture
cargo test --offline --locked --release --manifest-path evidence/task5-quality-rereview/Cargo.toml --target-dir evidence/task5-quality-rereview/target -- --nocapture
```

Raw results: [debug-probes.log](debug-probes.log), [release-probes.log](release-probes.log). The review Cargo.lock was copied from the original independent review; all **205 shared package records**, including versions, sources, checksums and dependency edges, exactly match product Cargo.lock. The separate reviewer-owned compilation target was removed after the runs. Manifest, lockfile, tests and logs remain reproducible.

## Narrow code and regression review

The held-file comparison confirms exactly three changed inputs from the reviewed HOLD: `src/protocol/messages.rs`, the existing `tests/task5_usage_contract.rs`, and `docs/runtime-contract.md`. Admission, request inspection, stream ownership, other observers, configuration and dependencies are unchanged.

The eight-line runtime correction checks `terminal` before assigning counters in either usage-bearing `message_start` or `message_delta`. A post-stop usage event now sets the existing sticky invalid flag and returns, preventing both late qualification and mutation of already qualified usage. Non-accounting events are unaffected. The fix addresses the observed ordering loss without a broader protocol state machine or forwarding changes.

Read the implementer evidence in `product/artifacts/task5-quality-fix/`: `usage-red.log` contains four accounting assertion failures and two passing controls; `usage-green.log` contains the same six passing tests; `usage-suite-green.log` reports all 27 usage tests passing. The new regression cases cover both original bad orderings, post-stop updates to previously qualified cached usage, valid ordering and ordinary post-stop events. These are inspected implementer results, distinct from the independent seven-test/profile reruns above. The implementation fixture's 335 estimate and this review's explicit-cap 352 estimate are intentionally different request byte counts and are not conflated.

## Identities, limits and assessment

[identity-before.json](identity-before.json) and [identity-after.json](identity-after.json) compare all **36 held source inputs: 36/36 unchanged before and after this re-review**. Both held binary hashes also match their expected current identities:

- Debug SHA-256: `4578b4492177512d653b53bcd2a85a6fd71f35be686900d76d6706f1058b36da`
- Release SHA-256: `8011efa55a45c3223353c484aba43d7889c8f18257656bbe606ce98226ae1c1d`

The independent tests compile the held library. They do not run or replace held CLI executables or establish build provenance. No product source/test/doc edit, real provider/model call, download, client configuration, OS service, wildcard/non-loopback listener, external message or Git operation occurred. All temporary sockets belong to these synthetic probes.

The full 138-test/profile regressions, final Pi positive/negative flows, prior unchanged Python five-test evidence and current held-CLI process checks remain implementer/parent evidence; this reviewer did not repeat or claim those runs. No new concern required a broader product-suite repetition. Task 6 queue selection/fairness, Task 7 retries/cooldown/global resource bounds, Task 8 performance, native setup, real providers and Windows/Linux remain outside this verdict.

**READY FOR TASK 6.** Both original counterexamples now conservatively retain the reservation, the added event-name fallback checks pass, and ordinary-event and joint-ownership controls remain intact. No remaining Critical, Important or actionable Minor finding in this narrow re-review. **HOLD after verdict.**
