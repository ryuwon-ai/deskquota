# Core Task 3 independent code quality review

Assessment: **FIXES REQUIRED**. One Important build/test issue and one Minor documentation issue. No Critical issue identified.

This is the quality review after the current Task 3 SPEC PASS in `../task3-spec-rereview/README.md`; the earlier spec FAIL is historical. The held debug binary SHA256 is `6128b81969201b3bd4fb4b3af0b56e6f4da8ee5c085efdd411c73f520d6d15df`. Reviewed current files are fingerprinted in `reviewed-source-sha256.json`; `../product-task2-source-manifest.json` supplies the previous boundary, not a reconstructable Git diff.

## Strengths

- Upstream lifetime is concretely owned by a supervised worker. The execution guard is declared before the HTTP future/body, so reverse local drop order closes the upstream resource before releasing the slot. One RAII terminal guard covers the normal/error/deadline/abort paths. Stop aborts and joins the connection tasks and workers before returning.
- Delivery and observation are separated from protocol rewriting. The 64 KiB delivery semaphore follows owned copied Bytes even after they leave the mpsc channel; 16 KiB copying avoids retaining oversized upstream backing allocations. SSE retained storage uses a fixed allocation rather than unbounded event/line vectors.
- The separate terminal oneshot correctly makes missing completion or explicit failure an HTTP body error after queued payload is exhausted. A full payload channel cannot silently erase the failure signal.
- Protocol observers are small endpoint modules; the larger transport file contains real ownership, bounded delivery, and framing responsibilities rather than speculative retry/fairness features. Existing Tokio, Hyper, Reqwest, Bytes, and socket2 APIs are used with explicit features and a locked manifest.
- Tests use real loopback connections with independently gated first data, terminal data, and HTTP EOF. They distinguish RST from valid request half-close, and inspect downstream framing on interruption. Documentation appropriately excludes provider compute-abort proof, real-provider compatibility, Windows acceptance, and RSS/performance claims.

## Issues

### Important: Release integration targets cannot compile

Locations: `product/src/server.rs:336`, `product/tests/stream_lifetime.rs:71`, `product/tests/stream_lifetime.rs:916`.

`server::testing` is gated on `debug_assertions`, but the integration test helpers and shutdown test refer to it unconditionally. `cargo +1.88.0 check --locked --release --all-targets` directly failed with exit 101 and two E0433 errors because the module is absent. This prevents the stream regression suite from compiling against optimized builds; debug success does not cover that path. The profile-dependent public test seam should be made deliberate and consistent, with release tests still exercisable rather than silently dropping the entire suite. Validate both debug and release all-target compilation after the change. `GatewayHandle.metrics` at server.rs:142 is also only used by that seam and currently emits a release dead_code warning; align its lifetime/gating with the chosen test support design.

Evidence: `release-all-targets.log`.

Scope distinction: `cargo +1.88.0 check --locked --release --lib --bins` **succeeded**, with the dead_code warning. The observed defect is release integration-target compilation, not a demonstrated failure to build the production binary. Evidence: `release-product-check.log`.

### Minor: The 1 MiB aggregate payload statement uses the wrong lifetime

Location: `product/docs/runtime-contract.md:103-110`; ownership evidence: `product/src/transport/stream.rs:51-69`, `237-242`, `340-366`, `402-409`.

The per-response 64 KiB bound is supported, but multiplying it solely by active execution concurrency does not establish a process-wide 1 MiB queued-payload bound. After upstream EOF the worker releases its execution slot while its `DeliveryReceiver` and owned `QueuedBytes` can remain in a downstream response. Another worker can then run while the previous response still retains delivery bytes. Those bytes still count in the global metrics counter. Clarify that 1 MiB describes the contribution of at most 16 currently executing workers, and explicitly include completed responses with outstanding downstream delivery in any aggregate bound. No new global budget or RSS experiment is required for this documentation fix.

This is source-supported lifetime analysis, not a measured global peak or an unbounded-memory claim; the separate ingress connection cap remains present.

## Validation and limits

Directly performed here: source/manifest/lock/runtime-contract/test inspection, release all-target check (FAIL), release lib/bin check (PASS with warning), source and held-binary fingerprints. Read the independent 83-test spec re-review and existing behavioral RED/GREEN evidence; did not redundantly rerun the full debug suite or the parent's foreground CLI probes. No product source edits, Git commands, real LLM/API calls, user configurations, or OS service changes occurred. Only review evidence and Cargo build artifacts were written.

Proceed to Task 4 after the Important issue is fixed and independently rechecked; the small documentation correction should accompany it.
