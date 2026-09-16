# Cache and circuit protection implementation plan

> For agentic workers: use subagent-driven-development for the isolated cache task and independent spec/quality review; coordinate shared server integration with the primary worker.

**Goal:** Save simultaneous duplicate calls and stop repeatedly contacting failing upstream scopes, preserving native lightweight operation and wire semantics.

**Architecture:** Reuse ExactCache's mutex/Notify with RAII fill ownership. A separate small circuit module owns hashed bounded scopes and per-attempt outcome guards; server and transport call it at validation, dispatch and completion. No extra body parse, dependency or daemon.

**Tech Stack:** Existing Rust, Tokio, serde, SHA-256, reqwest; existing Python loopback benchmark.

## Chunk 1: Implementation and acceptance

- [x] Record clean baseline and review the approved spec against current code and reference patterns.
- [x] In `product/src/cache.rs` add bounded owner/waiter state and cancellation-safe election; adapt `product/tests/cache_contract.rs` with cap3, failure/takeover and deadline regression scenarios. Keep all exact-key and complete-response safety checks.
- [x] Integrate cache wait before admission in `product/src/server.rs`; preserve original queue deadline and stop/disconnect priority. Remove the superseded queued-only waiting path.
- [x] In `product/src/protocol/request.rs`, return model index together with RequestCost and update its callers/tests. In `product/src/circuit.rs`, implement bounded scope state, outcome guard, Retry-After reuse and aggregate snapshot. Add module registration in `product/src/lib.rs`.
- [x] Integrate circuit check before admission and at real attempt start in server/stream; distinguish upstream wait from local delivery timeouts. Add `product/tests/circuit_contract.rs` using current fixtures and minimal deterministic state tests.
- [x] Add concise CLI status, English/Korean README and runtime contract updates describing follower first-token delay and circuit limits.
- [x] Run `cargo test --locked`, `cargo fmt --check`, `cargo clippy --locked --all-targets -- -D warnings`, `cargo build --locked --release --bin llmgw`, using CARGO_TARGET_DIR outside iCloud, build jobs 2 and incremental disabled. Capture exact outcomes. Review spec then quality, fix findings.
- [x] Extend `product/scripts/benchmark_queued_cache.py` to observe followers outside admission without weakening old baseline assertions; run five paired trials and independent nonduplicate/cache-off controls. Record all outcomes and footprint rather than latency-only wins.
- [x] Record report and work-item/loop evidence.
- [ ] Commit the reviewed change as `feat: coalesce exact requests and protect failing upstreams`, fast-forward `develop`, push `origin develop`, and inspect triggered macOS/Windows native CI. No release publication in this scope.
