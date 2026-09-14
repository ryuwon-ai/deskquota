# Task 7 SPEC correction — simultaneous follow-on writers

**Implementation DONE; source and final binaries HOLD for SPEC re-review.** This is a missing-coverage correction, not a runtime bug fix. The initial real-socket run passed on the existing implementation. QUALITY and Task8 remain unstarted here.

The [reopened SPEC report](../../../evidence/task7-spec-review/README.md) correctly found that the original resource test had64 partial body owners but only one follow-on writer. The useful deterministic single-writer test is preserved byte-for-byte as a prefix of `tests/resource_bounds.rs`, verified against `product-task7-pre-review-held-source.tar.gz`.

Only source change: append `sixty_four_barrier_released_writers_account_for_every_memory_pressure_outcome` in `tests/resource_bounds.rs`. It uses the existing Tokio Barrier/JoinSet and socket fixture, with four runtime worker threads and no new dependencies/runtime configuration.

The new test first waits for actual server consumption of64×512KiB partial bodies and exact32MiB held ownership. All64 independently running writer tasks then park at a65-party barrier; the coordinator is the final participant. It verifies64 arrivals and zero attempted follow-on writes before opening the barrier. Every task then transmits its final byte and reads a terminal response under a10-second bound. Every socket closes, every task joins, and all64 original contender IDs receive a terminal classification. The first observed protocol memory429 triggers a fresh completable generation request. Successful contender IDs must match the actual upstream captures exactly; rejected IDs must be absent. No hidden repeated wire attempt or unaccounted pending client is accepted.

## Direct execution

| Observation | Initial new case | Final debug focused | Final release focused |
|---|---:|---:|---:|
| Passed tests / failed | 1 /0 | 2 /0 | 2 /0 |
| Original writer tasks / barrier arrivals | 64 /64 | 64 /64 | 64 /64 |
| Follow-on write attempts / successful socket writes | 64 /64 | 64 /64 | 64 /64 |
| Original HTTP200 completions | 63 | 63 | 63 |
| Original protocol429 `gateway_memory_full` | 1 | 1 | 1 |
| Canceled / unexpected / pending original clients | 0 /0 /0 | 0 /0 /0 | 0 /0 /0 |
| Fresh successful generation | 1 | 1 | 1 |
| New-case total data ingress / actual upstream attempts | 65 /64 | 65 /64 | 65 /64 |
| Counted pressure ownership samples | 65 | 65 | 65 |
| Maximum observed held request bytes | 33,554,432 | 33,554,432 | 33,554,432 |
| Final held bytes / active holds / queued tickets | 0 /0 /0 | 0 /0 /0 | 0 /0 /0 |

Each final focused run includes **both** the unchanged deterministic single-writer test and the new barrier test. The original test separately has65 data ingress, upstream1, memory rejection1, canceled partial senders63 and fresh completion1. Thus the two focused tests together have130 data ingress /65 actual upstream attempts per profile; authenticated control reads are excluded. Repeated runs are not additional distinct product scenarios or a performance sample.

At every counted pressure sample ownership is at most32MiB. After each observed memory429 it must also fit the remaining non-rejected contenders' maximum body lengths, confirming that rejected partial ownership is no longer retained. All bodies/holds/tickets ultimately return to zero. There are65 counted pressure observations (full-budget gate plus one after each original outcome), plus startup/drain gates. This is sampled payload ownership with a semaphore-backed source invariant, not a continuous allocation trace, RSS measurement, proof of simultaneous CPU execution, or latency benchmark. The barrier synchronizes the senders; the server can process their resulting memory requests in different orders. One rejection freeing space for63 successes is valid and is not hard-coded as the required rejection count.

## Commands and preserved evidence

Working directory: `/Users/ryuwon/Desktop/ryuwon-project/llm-gateway-research/product`.

```sh
cargo test --locked --test resource_bounds sixty_four_barrier_released_writers -- --nocapture
cargo fmt --all
cargo fmt --all --check
cargo clippy --locked --all-targets -- -D warnings
cargo test --locked --test resource_bounds sixty_four_ -- --nocapture
cargo test --release --locked --test resource_bounds sixty_four_ -- --nocapture
cargo build --locked
cargo build --release --locked
```

All listed commands exited0. Logs: `initial-case.log`, `fmt.log`, `clippy.log`, `debug.log`, `release.log`, `debug-build.log`, `release-build.log`. Parsed outcomes and current41-file identity are in `results.json`.

The first post-test binary identity assertion failed: Cargo's integration-test invocation had replaced the normal binary paths with different artifacts. Their exact hashes remain in `intermediate-binary-identity.json`; dev-dependency feature selection is an explanation inferred from Cargo build behavior, not independently proven here. Normal `--locked` builds restored the exact accepted pre-correction binary hashes. No binary bytes were copied from elsewhere or manually patched.

## Identity and scope

- `tests/resource_bounds.rs` SHA256: `0289bb9e2b00f58d87cc4da7c79d520c08c7424744bb3f72200ab25dc5d9d1a6`
- Final debug binary: `4fb6c03938774b8ca756490ea6c3f624bc5c2bf9e72cdf54c9fb9fb7421609f0`
- Final release binary: `9be8dcf3cb880b41d6662a41eb91d25c27df07e7be6f23d3fae62c81b374b36d`

All40 other held source files, including production Rust, Cargo manifests/lockfile, example, runtime contract, Pi script and template, remain identical. Final binaries match the original held identities and previous Pi/foreground artifacts. No full186-test suite, Pi or transport probes were rerun in this correction; their previous evidence remains historical and unchanged. This correction claims only the focused tests and listed checks above.

Changed non-source artifacts: this supplemental directory and an appended link in `artifacts/task7-results.md`. Original Task7 command/results logs,41-file identity, pre-review source archive and initial/reopened SPEC reports remain preserved. No Git, real client configuration, service, model, provider API or Task8 work occurred.
