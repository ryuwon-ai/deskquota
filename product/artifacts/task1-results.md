# Task 1 reproducible results

All inputs are the synthetic configuration embedded in
`tests/config_contract.rs` or `examples/fixture.toml`. No API or model server was
contacted.

## TDD evidence

- RED: `cargo test --test config_contract` compiled and ran 14 tests against the
  loader stub; 0 passed and 14 failed for the expected missing behavior. Full
  output: `task1-red.log`.
- GREEN: `cargo test --test config_contract` ran the implemented contract; 14
  passed and 0 failed. Full output: `task1-green.log`.

## Initial verification

The exact commands and output are in `task1-verification.log`:

```sh
cargo fmt --all -- --check
cargo check --locked
cargo clippy --locked --all-targets --all-features -- -D warnings
cargo test --locked
cargo run --locked -- --help
cargo run --locked -- --version
cargo run --locked -- doctor --config examples/fixture.toml
```

All commands completed successfully under `rustc 1.88.0`. The full test run had
14 integration tests and no unit or doc tests. The doctor command only parsed
and validated the local fixture.

Direct dependencies used in this task are Clap 4.6.6, Serde 1.0.228,
`toml_edit` 0.25.15, URL 2.5.8, and SHA-2 0.11.0. Cargo resolved them under the
package's Rust 1.88 toolchain without an MSRV error. Async, HTTP, UI, Python,
Node, and Redis dependencies were not added.

## Specification review fixes

The specification review added regression coverage for error redaction and the
Task 2 fixture boundary.

- RED: `cargo test --test config_contract` ran 16 tests; 12 passed and 4 failed.
  The failures demonstrated scalar and unknown-key disclosure, missing safe
  syntax location reporting, and incomplete upstream Debug redaction. Full
  output: `task1-spec-fix-red.log`.
- GREEN: the same command ran 16 tests; 16 passed and 0 failed after errors were
  reduced to safe field/error classes plus numeric locations and upstream Debug
  was fully redacted. Full output: `task1-spec-fix-green.log`.
- `examples/fixture.toml` now uses the credential-free loopback base
  `http://127.0.0.1:18080/team/v1?api-version=fixture` with auth mode `none`.

The final post-fix check repeated formatting, locked dependency checking,
Clippy with warnings denied, the full 16-test suite, and offline fixture doctor.
All passed; exact commands and output are in
`task1-spec-fix-verification.log`.
