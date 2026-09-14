# Core Task 7 independent QUALITY review

**Verdict: CHANGES REQUESTED — one Important issue (Q7-1).** No Critical or Minor issues are asserted. Exploration stopped after establishing the concrete error-transport blocker. This is a bounded Task 7 quality gate, not production readiness or approval to commit/merge.

## Reviewed scope and identity

Read the umbrella/research instructions, research README/HARNESS/work items, runtime contract, Task 7 plan and retry-classification source notes, SPEC re-review outcome, and the requested code-review templates. Reviewed the production delta against `evidence/product-task6-held-source.tar.gz`, including retry timing/classification, Queue/Hold reentry, stream error and ownership transitions, ingress deadline and budget status. `task6-task7.diff` is a filesystem archive diff, not a Git range. The product is not a Git repository; no Git mutation or invented commit range was used. The memory registry search had no relevant hit and no memory facts were used.

`identity-before.json` and `identity-after.json` match all 41 authoritative source entries and each other. Both held binaries remain unchanged:

- debug: `4fb6c03938774b8ca756490ea6c3f624bc5c2bf9e72cdf54c9fb9fb7421609f0`
- release: `9be8dcf3cb880b41d6662a41eb91d25c27df07e7be6f23d3fae62c81b374b36d`

All writes and Cargo build output are confined to this evidence directory. The external package links the actual product library and directly imports the product socket fixture; it does not copy the production algorithm. `lock-comparison.json` verifies all 204 registry package versions/checksums match the product. These hashes do not independently prove held binary build provenance.

## Strengths (source-supported)

- Header cooldown and body replay qualification are separated; malformed explicit timing cannot silently become a missing-header retry, and numeric overflow cannot shorten a wait (`src/admission/retry.rs`). The classifier validates complete bounded JSON before a duplicate-rejecting projection.
- Retry releases the previous Hold and re-enters the sole Queue/Ledger with original timestamps (`src/admission/mod.rs:313`); execution/quota release has one RAII owner. Resource documentation distinguishes payload charges from allocator/RSS and delivery lifetimes from active upstream slots.
- The test-only 64-writer correction and original runtime evidence are explicitly separate. Prior SPEC and 186-test results are historical evidence, not executions by this reviewer.

## Important issue Q7-1

**`product/src/transport/stream.rs:451–453` (also `:284`, `:287–291`): a known pre-header 429 body-read failure is converted to a failing downstream body instead of an HTTP error.**

A small declared 429 response is fully probed before any downstream head is sent. If its Content-Length is 1000 but the peer sends only a short JSON prefix and closes, `probe_rejection` stores `Err(error)` in `prefix.extra`. The coordinator then sends `Ok(downstream)` and chains that already-known error into its body. Hyper encounters the failure before flushing the response and closes the downstream connection without any bytes. The client sees an empty transport failure; it loses both the original 429 context and the gateway's available protocol error envelope. Client transport-retry policies may consequently take a different path (that policy consequence is an inference, not tested client behavior).

This is the newly added probe boundary: the error is known while the gateway can still choose a pre-header HTTP response. The existing `HeadError::Upstream` branch in `server.rs` already provides a 502 `upstream_transport_error` envelope. Return probe failure through that path, with zero replay, retained shared cooldown and the existing unknown-usage cleanup. Do not fabricate a complete 429 body or convert a downstream-started stream failure into successful EOF.

Direct reproduction in `tests/transport.rs`:

| Scenario | Data ingress | Actual upstream HTTP | Downstream outcome | Final active holds |
|---|---:|---:|---|---:|
| Truncated small 429, immediate peer EOF | 1 | 1 | 0 bytes; no HTTP head | 0 |
| Complete unknown-code 429 positive control | 1 | 1 | HTTP429, exact original body | 0 |
| Truncated small 429, gated peer EOF | 1 | 1 | No bytes before EOF gate; 0 bytes after gate | 0 |

The gated test sends the valid head/prefix, verifies the gateway has not emitted any downstream bytes while the upstream remains open, then deliberately closes the upstream and requires an HTTP outcome. It fails deterministically with an empty clean read. This removes any reliance on a client resetting or a simultaneous downstream cancellation. No automatic second attempt occurs. The positive control demonstrates that the fixture/auth/route can deliver an intact 429 correctly.

The immediate scenario was run once initially, then again with the two confirmation scenarios. Total reviewer execution: **4 test executions across 3 unique scenarios; 4 ingress and 4 actual upstream HTTP attempts; 1 intact HTTP429 and 3 empty downstream outcomes; 0 pending tests.** No library-only pseudo-HTTP counters are mixed in. Initial/confirmation Cargo exits were 101 because the assertions detected the product issue, not because a test timed out.

## Commands, failures and limits

Cwd for commands: `/Users/ryuwon/Desktop/ryuwon-project/llm-gateway-research`.

```sh
cargo test --manifest-path evidence/task7-quality-review/Cargo.toml --offline --target-dir evidence/task7-quality-review/target --test transport -- --nocapture
cargo test --manifest-path evidence/task7-quality-review/Cargo.toml --offline --locked --target-dir evidence/task7-quality-review/target --test transport -- --nocapture
```

The first package compile failed because the reviewer omitted direct `socket2`, which the imported product fixture uses. It ran no tests and is preserved in `harness-compile-failure.log` (exit101). After adding exact `socket2 = 0.6.5`, the first command produced `debug.log` (one failing product assertion). The locked confirmation command produced `debug-confirmation.log` (one pass, two failing product assertions). `summary.json` preserves separate denominators and outcomes. A read-only Python SHA256 pass captured identities before/after; another parsed both Cargo.lock files to compare registry entries. The archive was read without extraction into product paths.

No release-profile execution, full 187-test run, broad suite repetition, Task6 binary execution, paid/real provider, persistent configuration change, model/service installation, real user data, performance/RSS or OS compatibility proof is claimed. Attribution to the new probe boundary is source-supported plus current-library reproduction; it is not an executed before/after Task6 comparison. Existing immediate stream-error behavior outside this new probe is not broadened into an additional finding.

All reviewer Cargo commands and loopback fixtures have completed; there are no reviewer-owned running test/build sessions. Source and binary HOLD remains intact. Return to the sole implementer for this bounded correction, then re-run these scenarios against the corrected held source and complete the same-reviewer quality re-review.
