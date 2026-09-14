# Q7-1 same-reviewer QUALITY re-review

**Verdict: QUALITY PASS. Q7-1 is resolved; no remaining Critical, Important or Minor finding is asserted in this bounded review.** This closes the previously requested Task 7 quality correction. It is not a production-ready, merge, performance or cross-OS verdict.

## Scope and code assessment

The initial `../README.md`, `../summary.json` and failure logs remain unchanged. Compared authoritative SPEC-fix and quality-fix manifests: exactly three source files changed (`changed-files.json`): `product/src/transport/stream.rs`, `product/tests/retry_contract.rs`, and `product/docs/runtime-contract.md`. Read the correction and implementer command/identity reports directly.

The correction gives `probe_rejection` a `Result<RetryPrefix, reqwest::Error>` result. A transport error returns through `HeadError::Upstream` before any successful downstream head is sent. `RetryPrefix.extra: Option<Bytes>` can now retain only a successfully read overflow chunk, removing the invalid error-as-ordinary-prefix state. The existing server error mapping produces protocol 502 `upstream_transport_error`; the existing WorkerGuard remains the cleanup owner. Header cooldown is still applied before the probe, and the error branch exits before replay eligibility. Complete/oversized/encoded paths remain in their previous branches. The runtime contract now states this pre-header error boundary.

This is a narrow repair using the existing error path and guard. No new fallback, configuration, dependency, general abstraction or unapproved feature was introduced. The 64-writer test and all other product sources match the previous HOLD.

## Independent execution

An external package in this directory links the actual product library. Its copied reviewer test scenarios retain the original positive/immediate/gated reproductions and strengthen the negatives to require exact HTTP502 and JSON `error.code = upstream_transport_error`, one actual attempt, zero active holds, one cleanup and zero held TPM. Only the reviewer test files were copied; production algorithms were not copied. The original reviewer failures remain intact in the parent evidence directory.

The product-authored integration-test entry directly references actual `product/tests/retry_contract.rs`. Only `pre_head_rejection_body_failure_returns_502_keeps_cooldown_and_unknown_debt` is selected; the other 18 tests are filtered out. That one Rust test runs four Content-Length/chunked × Reserved/Actual variants. The fixture TOML in `examples/` is byte-identical to the product's synthetic public example, for the test's relative read path.

| Direct check | Debug | Release |
|---|---:|---:|
| Independently designed reviewer tests | 3 passed | 3 passed |
| Product-authored focused test | 1 passed (4 variants) | 1 passed (4 variants) |
| Data ingress / actual upstream HTTP requests | 11 / 11 | 11 / 11 |
| HTTP502 / original HTTP429 / fresh HTTP200 | 6 / 1 / 4 | 6 / 1 / 4 |
| Replay / pending outcomes | 0 / 0 | 0 / 0 |
| Command exits | 0, 0 | 0, 0 |

The three independent cases use real loopback sockets and the production library server: complete unknown 429 retains exact raw body; both immediate and gated incomplete 429 now produce the exact 502 envelope. The gated case confirms no downstream bytes before upstream EOF, then a complete HTTP502 instead of the original zero-byte close. Both negative cases end with starts1/cleanup1/active0/held0.

The four directly executed product variants additionally use known RPM/TPM and the same admission ManualClock. They check original failed POST debit and unknown usage debt, zero retained request bytes, one upstream-error terminal and no response-EOF terminal. A fresh other-root request remains queued at t64 and completes HTTP200 at t65 after the original 5-second cooldown. Per variant: two complete upstream HTTP requests, no replay, final starts2/cleanup2/active0/held0. The first is an actual captured POST body, the second is the fresh GET; TCP accepts are not counted as attempts by assumption. Virtual admission time is not a latency result.

Across both profiles this review executed **8 Rust tests (6 independent reviewer test executions + 2 product-authored executions containing 8 variants), 22 data ingress and 22 actual upstream HTTP requests**. Outcomes: 12 HTTP502, 2 original HTTP429, 8 fresh HTTP200, 0 extra retries, 0 pending. No historical suite or parent/native probe counts are included. Control/status reads are excluded. Raw outcomes are in four per-profile logs and `summary.json`.

## Commands and preserved harness failures

All Cargo targets were `evidence/task7-quality-review/target`, outside `product/target`. Initial reviewer transport debug run used `--manifest-path evidence/task7-quality-review/rereview/Cargo.toml --offline --target-dir evidence/task7-quality-review/target --test transport -- --nocapture` from research cwd, resolving the external package lock offline.

The following commands ran from this directory:

```sh
python3 identity.py identity-before.json
cargo test --offline --target-dir ../target --test product_retry_contract pre_head_rejection_body_failure_returns_502_keeps_cooldown_and_unknown_debt -- --nocapture
cargo test --release --offline --locked --target-dir ../target --test transport -- --nocapture
cargo test --release --offline --locked --target-dir ../target --test product_retry_contract pre_head_rejection_body_failure_returns_502_keeps_cooldown_and_unknown_debt -- --nocapture
python3 identity.py identity-after.json
```

The first attempt to compile the imported product integration test (using `--locked`) failed before running any test: the reviewer manifest omitted direct `reqwest` and `httpdate` needed by that source. `harness-product-test-compile-failure.log` preserves exit101 and diagnostics. Exact product versions were added, offline resolution updated the local package entry, and the selected test passed. `lock-comparison.json` verifies all 204 registry package versions/checksums match product Cargo.lock. No package upgrade or product dependency change occurred. The debug transport run predates those direct declarations but resolves the same product transitive versions. The release runs used the final locked manifest.

One read-only `sed product/src/transport/stream.rs` command from this deeper cwd returned `No such file or directory`; immediately using `../../../product/...` succeeded. No product modification or test execution resulted from that typo. There were no product assertion failures during this re-review. The implementer's own RED and intermediate fixture failure remain separate historical evidence and are not added to this execution denominator.

## Final identity and limits

`identity-before.json` equals `identity-after.json`: all 41 source SHA256 values match `product-task7-quality-fixed-source-manifest.json`, and both binaries match the quality-fixed debug/release manifests:

- debug: `ec3742d6eb0acd618cc1e1ce9d0ca1581e5a17d286a727b9c3267eeef276f2fc`
- release: `48a9232ae5ffa012ddd4caeae90ee6f2e6094755b0815c68d472e7b69a8316d4`

The reviewer did not execute or rebuild the held product binaries. All linked-library tests compiled only in the reviewer target. Hash equality is an identity check, not independent source-to-binary build provenance. All reviewer Cargo sessions, test runtimes and owned loopback fixtures have terminated. No reviewer process remains active, and source/binary HOLD is intact.

The implementer's full188 tests/profile and Pi evidence were read as prior reports, not re-executed or added to reviewer totals. Parent foreground probes are separate. This limited re-review does not establish full-suite results, RSS or performance targets, real provider quota compliance, Windows compatibility, native installation behavior or paid client operation.
