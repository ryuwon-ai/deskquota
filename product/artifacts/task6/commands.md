# Task 6 commands and evidence

Working directory: `/Users/ryuwon/Desktop/ryuwon-project/llm-gateway-research/product`.
No Git repository was created and no Git mutation was run. Baseline source manifest
`../evidence/product-task5-final-source-manifest.json` verified 36/36 matching files
before edits. The research parent owns its scripts, evidence and probes.

Behavior RED (all compile; failures are assertions, not missing symbols):

- `cargo test --locked --test fairness_contract`: `red-initial.log` (3 failed); `red-traces.log` (7 failed / 3 existing behavior passed).
- Same suite after control tests: `red-control-boundary.log` (missing status data, unbounded public root config, and the trace scheduling issue retained).
- `cargo test --locked --test fairness_contract normal_sixteen_root_rr_is_not_a_resource_bypass`: `red-normal-rr.log` (root 9 loses its normal turn).
- `cargo test --locked --test fairness_contract bypass_trigger_selects_oldest_valid_head_even_if_it_was_current_fit`: `red-oldest-eligible.log`.
- `cargo test --locked --test fairness_contract chosen_barrier_persists_until_its_own_admission_or_removal`: `red-sticky-barrier.log`.
- `cargo test --locked --test fairness_contract admitted_owner_retains_original_wait_start_and_queue_deadline`: `red-original-timing.log` (120s != original 60s).

Initial test access exposes the real pre-queue Admission under a hidden library
fixture API, accepting root/wait parameters before enforcing them. No simulator
was substituted. The initial wait-timing accessor exhibited current-time behavior
before Hold retained original timestamps. Subsequent GREEN logs preserve each
iteration, including test mistakes and the status key-order regression.

Final implementation checks (all pass):

- `cargo fmt --check` → `final-fmt.log`
- `cargo check --locked --all-targets` → `final-check.log`
- `cargo clippy --locked --all-targets -- -D warnings` → `final-clippy.log`
- `cargo test --locked --all-targets` → `final-debug-tests.log`
- `cargo test --locked --release --all-targets` → `final-release-tests.log`
- `cargo build --locked` → `final-debug-build.log`
- `cargo build --locked --release` → `final-release-build.log`

Only a stale Task 6 future-tense documentation sentence was corrected after these
checks; Rust sources were unchanged. `source-manifest.json` captures the final
38 source/contract/fixture files and both binary hashes. This fingerprint by
itself is not a build proof; the preceding build/test logs and matching Pi binary
hashes supply the associated execution evidence.

Pi commands and actual/expected exit codes are in `pi-commands.json`. They use
`python3 scripts/probe_pi.py --binary target/{debug,release}/llmgw --output
artifacts/task6/pi-{profile}-{positive,negative}.json`, with `--gateway-off` for
negative controls. Their existing fixture explicitly chooses RPM unlimited / TPM
unknown; the example's known quota startup hold remains unchanged.
