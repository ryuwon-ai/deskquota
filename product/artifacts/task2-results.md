# Task 2 results — socket HTTP transport

Date: 2026-09-12

## Scope implemented

- Added the shared loopback HTTP/1 server used by the library and
  `llmgw run --config PATH`.
- Added exact root/method/endpoint/model routing, base-prefix and lossless-query
  joining, decoded-key conflict rejection, selective single-pass JSON inspection
  with byte-for-byte body forwarding, and pre-upstream rejection for unsupported
  Responses background requests and upgrades. Duplicate top-level `model` or
  `background` fields, including escaped-equivalent keys, reject before upstream.
  The observer validates the full body as UTF-8 without copying it, so malformed
  bytes inside skipped unknown values or nested keys also reject before upstream.
  Query input is forwarded only when the exact composed bytes survive the URL
  serialization required by Reqwest.
- Added one reused Reqwest 0.13.5 client with redirect, library retry, and
  ambient proxy use disabled; connect timeout is 10 seconds, total request
  timeout is 30 minutes, and idle pool size is 16 per host.
- Added request/response hop-by-hop scrubbing, Host regeneration, gateway
  credential stripping, and explicit `forward`, `env`, and `none` auth behavior.
- Added distinct data/control credentials, authenticated local health/status/
  stop endpoints, Origin rejection, bounded non-payload counters, and startup
  loading from protected state files. Unix opens use `O_NOFOLLOW` and validate
  permissions on both the resolved path and opened file descriptor.
- Added actual receiver limits: 32 KiB Hyper parser buffer, 128 headers,
  10-second header timeout, 128 accepted connections, 30-second body read,
  8 MiB per body, and incremental 32 MiB stored-body semaphore.
- Body chunks merge into one permit. `Bytes::from_owner` ties that permit to the
  backing allocation until the last `Bytes` clone is dropped.
- The public server entry point rechecks that `Config.listen` is loopback before
  credential validation or binding, including for directly constructed configs.

## TDD evidence

The first compile-capable stub had no listener and produced 10 expected
ConnectionRefused failures (`task2-red.log`). The stronger primary RED used a
real ephemeral loopback listener returning HTTP 501: all 10 required contracts
failed on their expected HTTP statuses (`task2-http-red.raw.log`). A later
hardening cycle showed 17 passing wire cases and one failing duplicate-token
case, then rejected duplicate credentials and returned the suite to green
(`task2-wire-hardening-red.raw.log`).

The specification rereview then exposed silent query reserialization. The
config RED accepted `trace=a'b`, and the socket RED forwarded it as
`trace=a%27b`; an empty request query was also silently dropped. Raw failures
are in `task2-spec-fix-red.log`. The implemented boundary rejects such inputs
before upstream while preserving verified `%27`, `+`, and lowercase `%2b`
encodings. Targeted GREEN evidence, including fixed-gzip passthrough, is in
`task2-spec-fix-green.log`.

The quality review then reproduced two policy bypasses. An actual socket RED
forwarded a request with duplicate top-level inspected JSON keys, and a separate
pre-bind RED accepted a directly constructed wildcard listen address. Raw
failures are in `task2-quality-fix-red.log`. The selective visitor now detects
plain and escaped-equivalent `model`/`background` duplicates while skipping
unknown values, and public startup rejects non-loopback addresses before any
bind. Targeted and full-wire GREEN output is in
`task2-quality-fix-green.log`.

The follow-up quality review reproduced invalid UTF-8 being skipped inside an
unknown string value and an unknown nested object key. The actual socket RED is
in `task2-utf8-fix-red.log`. The body is now validated once as borrowed UTF-8
before the same selective JSON observation, without copying or a second JSON
parse. Targeted GREEN output is in `task2-utf8-fix-green.log`.

Final wire command:

```text
cargo test --locked --test wire_contract
```

Initial result: 23 passed, 0 failed (`task2-green.log`). These are real TCP gateway and
synthetic upstream fixtures except the explicit startup-file and reserved-header
startup cases. No Router one-shot tests are used.

## Full verification

The following commands ran in order and exited successfully; raw output is in
`task2-verification.log`.

```text
cargo fmt --all -- --check
cargo check --locked
cargo clippy --all-targets --locked -- -D warnings
cargo test --locked
cargo run --locked -- --help
```

The post-spec-fix verification is recorded separately in
`task2-spec-fix-verification.log`. The post-quality-fix verification is in
`task2-quality-fix-verification.log`, followed by the UTF-8 regression
verification in `task2-utf8-fix-verification.log`. The final result is 50 passed: 1
body-owner unit test, all 16 original Task 1 configuration tests plus 2 Task 2
validation tests, and 31 wire/startup contract tests. CLI help lists only `run`,
`doctor`, and generated `help`.

Resolved direct versions include Rust 1.88-compatible Axum 0.8.9, Reqwest
0.13.5, Tokio 1.49.0, Hyper 1.8.1, and Bytes 1.11.1. Reqwest default features
remain disabled; only `rustls`, `stream`, and `http2` are enabled. No
compression, WebSocket, UI, Redis, Node, or Python runtime dependency was added.

## Evidence boundary and remaining work

Directly verified here: synthetic loopback request/response bytes, query/path,
auth and reserved headers, no redirect/retry, local control isolation, model and
route rejection, 8 MiB declared-body rejection, one-byte chunked ingress,
32 KiB parser rejection, token-file preconditions, final-clone permit lifetime,
fixed gzip status/header/compressed-byte preservation, duplicate top-level
inspected-key rejection including escaped equivalents, direct public-start
loopback enforcement, invalid UTF-8 rejection in ignored and nested JSON
positions, compilation, lint, format, and test status.

Arbitrary raw queries are not supported. A literal apostrophe, an empty query,
or any other query that `url::Url` would reserialize is rejected safely before
upstream; clients must explicitly percent-encode such characters. This avoids
silent request alteration without replacing the complete maintained TLS/client
stack in Task 2.

Implemented from code and documentation but not performance-tested here: the
128-connection cap, 10/30-second receiver timeouts, 30-minute request deadline,
and concurrent 32 MiB saturation. The later resource suite is still required
for peak RSS and many simultaneous partial requests.

Task 3 still owns downstream disconnect drain, upstream EOF-owned active slot,
SSE observer limits, terminal cleanup races, and stronger stop behavior. Task 5
and later still own configured execution concurrency, quota accounting, queue
limits, fair round robin, cooldown, and gateway-owned retry. Actual LLM calls,
user client configuration, OS service setup, Windows ACL/runtime validation,
and performance claims remain unverified.
