# Core Task 5 spec re-review — SPEC PASS

Reviewed 2026-09-12. No remaining actionable Task 5 spec mismatch was found in this bounded re-review. Both findings in the [initial review](../task5-spec-review/README.md) are resolved. **Product remains HOLD for the separate fresh quality review.** No product source or held binary was edited.

## Independently reproduced results

The exact original six probe functions/helpers were retained as a byte-identical prefix of [tests/review.rs](tests/review.rs); the original failure logs were not overwritten. They now pass against the corrected production library in debug and release. Three additional independent tests cover nine narrow wire cases across Chat, Responses and Messages.

| Independent scenario | Debug | Release |
|---|---|---|
| Chat nonempty choices with usage, no final empty-choices report | Retains 352; pass | Retains 352; pass |
| Chat missing choices with usage | Retains 352; pass | Retains 352; pass |
| Responses completed followed by failed | Retains 365; pass | Retains 365; pass |
| Valid final Chat usage | Debit 23; pass | Debit 23; pass |
| Overflowing usage sum | Retains 352; pass | Retains 352; pass |
| Interleaved late usage/debt/expiry/old-ID ledger control | Pass | Pass |
| Named error before successful reports, all three endpoints | Retains estimate; pass | Retains estimate; pass |
| Named error after success with ordinary JSON type, all three endpoints | Retains estimate; pass | Retains estimate; pass |
| Ordinary named notice containing the word error, all three endpoints | Known debit 23; pass | Known debit 23; pass |

**9 tests passed / 0 failed per profile, exit 0.** These comprise 14 real loopback wire cases and one direct ledger scenario per profile. Every wire case observes one upstream attempt and one cleanup and shuts the gateway down before its result is asserted. The manual clock advances the production ledger startup hold; transport is actual ephemeral IPv4 loopback HTTP. No real provider, model, download, client configuration, OS service or wildcard listener was used.

Exact commands from `/Users/ryuwon/Desktop/ryuwon-project`:

```sh
cargo test --offline --locked --manifest-path llm-gateway-research/evidence/task5-spec-rereview/Cargo.toml --target-dir llm-gateway-research/evidence/task5-spec-rereview/target -- --nocapture
cargo test --offline --locked --release --manifest-path llm-gateway-research/evidence/task5-spec-rereview/Cargo.toml --target-dir llm-gateway-research/evidence/task5-spec-rereview/target -- --nocapture
```

Raw logs: [debug-probes.log](debug-probes.log), [release-probes.log](release-probes.log). The review-only Cargo.lock was copied from the initial independent review; all product shared locked package records, including sources/checksums/dependency edges, match exactly. Both commands compiled the corrected held library in a separate target directory. The reviewer-owned compilation target was removed after completion; the reproducible manifest, lockfile, tests and logs remain.

## Source and RED/GREEN review

- [Chat final report selection](../../product/src/protocol/completions.rs#L48) requires usage before DONE and an empty choices array. Unsupported interim report shapes cannot establish final accounting usage. Null-usage deltas remain compatible with a subsequent valid final report, and explicit error marks usage invalid.
- [Responses completed validation](../../product/src/protocol/responses.rs#L46) rejects contradictory nested status/error and missing usage; [unsuccessful terminal handling](../../product/src/protocol/responses.rs#L95) is sticky across either ordering. Earlier successful usage cannot produce a refund after failed/incomplete/error or a later unsupported completed report.
- Messages explicitly invalidates standard error events. [Shared named-error handling](../../product/src/protocol/mod.rs#L48) and the [fixed-size SSE event-name classifier](../../product/src/transport/stream.rs#L514) retain the standard error signal without requiring a JSON type. Ordinary unknown event names/types remain supported. No large protocol state machine or blanket unknown-event rejection was introduced.
- The changed-file set is confined to five observer/decoder source files, runtime documentation and the new usage-contract test. Admission/config/dependencies and transport forwarding behavior were not changed by this fix.
- Read the actual implementer [usage-red.log](../../product/artifacts/task5-spec-fix/usage-red.log): 15 failing assertions and four passing controls from 19 tests, including Messages error before/after stop and named error before the correction. The [partial fix log](../../product/artifacts/task5-spec-fix/usage-partial-fix-red.log) retains the remaining named-error failure (18 passed / 1 failed). [Named-error RED](../../product/artifacts/task5-spec-fix/named-error-red.log) then fails three targeted cases, and [usage-green.log](../../product/artifacts/task5-spec-fix/usage-green.log) passes all 21. These are implementation-phase evidence inspected by this reviewer, not reruns claimed as independent checks.

The original Task 5 ledger/admission/request-policy review remains applicable because its source inputs are unchanged by this correction. The prior observer defects now satisfy the conservative-unknown accounting rule rather than refunding unsupported usage.

## Evidence boundaries and identities

[identity-before.json](identity-before.json) and [identity-after.json](identity-after.json) compare all **36 held inputs** with `product-task5-spec-fix-source-manifest.json`: **36/36 unchanged before and after review**. Both held binaries also match before and after:

- Debug: `5fe45aca9910a31fb08e1d4dea4cadf7f1a53ba4a4f7b04410401d02e7b71713`
- Release: `3cfd0e778e2dab57d96b6fa2aeb0c05ea1ef71616e3ab7383514f98bcd98f562`

These probes compile the held library with identical shared dependency records; they do not run the held CLI executables or establish their build provenance. Full 132-test/profile regressions, Python five tests, Pi positive/negative flows, and actual held-binary CLI/stream process probes are implementer/parent evidence. This reviewer deliberately did not repeat those broad suites. No performance, real provider quota timing, other-PC consumption, OS suspend behavior, Windows/Linux operation, Task 6–8 completion or native setup claim follows from this pass.

**SPEC PASS for Core Task 5, bounded by the accepted scope and evidence above. HOLD remains; separate quality review is pending.**
