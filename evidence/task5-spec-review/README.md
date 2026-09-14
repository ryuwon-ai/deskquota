# Core Task 5 independent spec review — CHANGES REQUESTED

Reviewed 2026-09-12. Product source and binaries remain **HOLD**. No product changes were made.

## Actionable findings

### 1. [P2] Require the supported final Chat usage report before Actual settlement

**Directly reproduced in debug and release.** A Chat SSE event with nonempty `choices` and numeric usage, followed by `[DONE]` and successful HTTP EOF, reduces a 352-token reservation to 2. The same occurs when `choices` is missing entirely. Neither stream includes the documented final `choices: []` usage report, so the request must remain usage-unknown and retain 352 until its original expiry.

The [usage extraction](../../product/src/protocol/completions.rs#L38) accepts any event containing the two usage counters; [finish](../../product/src/protocol/completions.rs#L94) checks only a terminal marker and the invalid flag. It never establishes the supported final report shape. This contradicts the [final Chat contract](../../product/docs/runtime-contract.md#L307) and Task 5's requirement that missing, partial or unsupported usage must preserve the reservation. Require the supported final report before publishing known usage, and cover nonempty/missing/malformed choices with real-wire tests. Keep the normal null-usage delta and valid final empty-choices report working.

Although the observer originated in Task 3, Task 5 now passes its result through [Hold cleanup](../../product/src/admission/mod.rs#L150) into [Actual settlement](../../product/src/admission/quota.rs#L228). The new consequence is an incorrect 350-token refund and extra admission budget, not merely inaccurate telemetry.

### 2. [P2] Invalidate contradictory Responses terminal outcomes before settlement

**Directly reproduced in debug and release.** A `response.completed` event with usage 1+1 followed by `response.failed` and successful HTTP EOF reduces a 365-token reservation to 2. Contradictory terminal states cannot establish supported successful final usage, so the reservation must stay 365.

[Responses terminal handling](../../product/src/protocol/responses.rs#L84) only sets `terminal` for failed/incomplete and leaves `completed` and earlier usage intact. [Finish](../../product/src/protocol/responses.rs#L101) subsequently returns that stale successful usage. Mark conflicting completed/failed/incomplete terminal sequences conservatively unknown, in both orders, while preserving legitimate repeated consistent observation behavior. Add real-wire regressions for both failed and incomplete conflicts. The Task 5 impact is an incorrect 363-token refund; the forwarding bytes themselves still pass through.

## Independent verification

The six reviewer tests in [tests/review.rs](tests/review.rs) link the held production `llmgw` library. Only the clock seam advances startup/window time; requests and synthetic SSE responses traverse actual ephemeral IPv4 loopback HTTP sockets. No real model/API call, download, persistent client configuration, wildcard listener, OS service, Git operation, or product edit occurred.

| Probe | Debug | Release |
|---|---|---|
| Valid final Chat usage | Pass: debit 23 | Pass: debit 23 |
| Usage sum overflow | Pass: full estimate 352 retained | Pass: full estimate 352 retained |
| Independently interleaved ledger expiry/debt/old-ID scenario | Pass | Pass |
| Chat nonempty choices with usage | Fail: debit 2, expected 352 | Fail: debit 2, expected 352 |
| Chat missing choices with usage | Fail: debit 2, expected 352 | Fail: debit 2, expected 352 |
| Responses completed then failed | Fail: debit 2, expected 365 | Fail: debit 2, expected 365 |

Every wire case observed one upstream attempt and exactly one cleanup; shutdown completed before its result was asserted. Both profiles report **3 passed / 3 failed, exit 101**. Failures are the required counterexamples, not fixture startup or compile errors.

Exact commands, run from `/Users/ryuwon/Desktop/ryuwon-project`:

```sh
cargo test --offline --manifest-path llm-gateway-research/evidence/task5-spec-review/Cargo.toml --target-dir llm-gateway-research/evidence/task5-spec-review/target -- --nocapture
cargo test --offline --locked --manifest-path llm-gateway-research/evidence/task5-spec-review/Cargo.toml --target-dir llm-gateway-research/evidence/task5-spec-review/target -- --nocapture
cargo test --offline --locked --release --manifest-path llm-gateway-research/evidence/task5-spec-review/Cargo.toml --target-dir llm-gateway-research/evidence/task5-spec-review/target -- --nocapture
```

The first command generated the review-only Cargo.lock offline. Its entire shared package records (version, source, checksum, dependencies) match product Cargo.lock. The second and third commands used that lock. Output is retained in [debug-probes.log](debug-probes.log), [debug-locked-probes.log](debug-locked-probes.log), and [release-probes.log](release-probes.log). The dedicated compilation target was removed after both runs; manifests, lockfile, reviewer tests and logs remain reproducible.

## Source-only review and evidence limits

Code inspection found no additional Task 5 mismatch in joint quota/capacity ownership, nonexpiring unsent holds, explicit monotonic ledger timestamps, known startup hold, start/expiry notify registration, 8192-entry conservative retention bound, nonwrapping IDs, RPM commitment/nonrefund, explicit/default output-cap precedence, byte preservation, selective media policy, cache category summation, checked arithmetic, expiry/debt rules, or the runtime/test-clock boundary. This is a bounded source review, not an exhaustive absence-of-bugs claim.

Reviewed existing RED/GREEN evidence distinguishes eight original ledger assertion failures and wire/parser/start-notify discovery failures from deliberately named `*-omission-red.log` sensitivity checks. The Pi warmup fixture correction and preserved failed artifact are documented separately. Prior Rust/Python/Pi regressions and held CLI process probes are parent/implementer evidence; this reviewer did not rerun or claim those suites. Tasks 6–8, queue fairness, retries/cooldown, throughput/RSS, real providers, other PCs, OS suspend semantics and Windows/Linux remain outside this verdict.

## Held identities

[identity.json](identity.json) independently compares all 35 source inputs with `product-task5-pre-review-source-manifest.json`: **35/35 unchanged**. Both held CLI binaries also remain unchanged:

- Debug SHA-256: `a5edaa0c559a043272119e4b21493a2e6cd91889045df25e6f9eea816de47b76`
- Release SHA-256: `392142c6c5fab00fe3ea3d1299717f03204de8bc5af9717c7a8cb1f60e2d9b80`

The probes independently compile the held source with identical locked shared dependency records; they do not run the held CLI executables or establish binary provenance. This review authorizes no source fixes and no version-control operation. **CHANGES REQUESTED; HOLD for implementer correction and fresh review.**
