# Q7-1 commands and scope

Working directory: `/Users/ryuwon/Desktop/ryuwon-project/llm-gateway-research/product`.
Only synthetic owned loopback fixtures; no provider/client config/service/Git changes.

- Verified all 41 authoritative sources against `artifacts/task7-spec-fix/results.json`; copied it to `before-identity.json` before edits.
- `cargo test --locked --test retry_contract pre_head_rejection_body_failure -- --nocapture` → `red.log`, exit 101; runtime baseline, behavioral assertion: received 0 bytes instead of HTTP 502. One POST ingress / one actual upstream, zero response bytes. This stopped before subsequent framing/accounting variants.
- Minimal stream fix, `cargo fmt --all`, same focused test → `green.log`, exit 101. First Content-Length variant passed (2 ingress/2 upstream); chunked fixture had a hardcoded incorrect chunk length and failed its pre-EOF gate (one ingress/one upstream). Preserve this intermediate failure; it is not a passing GREEN result.
- Corrected fixture to calculate chunk length. `cargo fmt --all`; same focused test → `green-corrected-fixture.log`, exit 0: four variants, 8 data ingress/8 upstream, four 502/four 200, no replay.
- `cargo fmt --all -- --check`, `cargo check --locked --all-targets`, `cargo clippy --locked --all-targets -- -D warnings` → preliminary `fmt.log`, `check.log`, `clippy.log`, all exit 0.
- Added terminal-error/EOF metric assertions; `cargo fmt --all`.
- Final `cargo fmt --all -- --check` → `final-fmt.log`.
- Final `cargo check --locked --all-targets` → `final-check.log`.
- Final `cargo clippy --locked --all-targets -- -D warnings` → `final-clippy.log`.
- `cargo test --locked --all-targets -- --nocapture` → `debug.log`.
- `cargo test --release --locked --all-targets -- --nocapture` → `release.log`.
- Normal `cargo build --locked` and `cargo build --release --locked` → `debug-build.log`, `release-build.log` after integration tests.
- For each profile: `python3 scripts/probe_pi.py --binary target/<profile>/llmgw --output artifacts/task7-quality-fix/pi-<profile>-positive.json`; negative adds `--gateway-off` and uses `pi-<profile>-negative.json`. Negative exit 1 is expected; positive must exit 0. Logs use same basenames.

Final outcomes and hashes are recorded in README.md and identity.json, rather than inferred from command names. Full-test logs include the retained 64-writer barrier case. Historical Task7/Task7 SPEC evidence is not overwritten.
