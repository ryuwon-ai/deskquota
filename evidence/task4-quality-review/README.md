# Core Task 4 independent code quality review

Assessment: **FIXES REQUIRED** before Task 5. Reviewed 2026-09-12. This is a development compatibility harness review, not a new production gateway-runtime review. No product file was modified; no Git operation, real Pi/model/API execution, user configuration change, or OS service operation was performed by this reviewer.

Review target: `product/scripts/probe_pi.py`, `product/tests/fixtures/pi-models.json`, the Task 4 runtime contract and artifacts. The accepted Task 3 Rust/Cargo sources remain unchanged. All 30 source files and the binary match `product-task4-pre-review-source-manifest.json`; see `fingerprint-check.json`. Script SHA-256: `52a806e7c6da8417ec43353a3ebddd3164db9e5e8ac2b56fe9992ca64407cfa7`.

## Strengths

- The fixed completion/read fixtures, HTTP observations, Pi event parsing, acceptance predicates, and artifact writing have explicit responsibilities. The script's length alone does not justify a framework, package split, or additional runtime dependency. It imports only Python's standard library.
- Success requires the actual upstream attempt count and route/header observations, valid Pi JSON, no assistant error/aborted event, and the final marker. The read case further checks the exact read ID/path/result and the returned tool message. The gateway-off path uses the same predicates (`scripts/probe_pi.py:584-635`).
- Temporary state, restricted environments, offline/discovery switches, synthetic fixtures, and explicit scope limits support a narrow repeatable compatibility experiment. The artifact intentionally stores structural observations instead of raw bodies, tokens or Pi output (`:502-583`, `:636-650`).
- The independently executed specification review already demonstrates positive and genuine negative behavior with installed Pi. This review did not repeat those successful cases or the unchanged Rust suite without a new concern.

## Critical

None identified.

## Important

1. **Pi subprocess is left alive when the probe is interrupted.**
   - Location: `product/scripts/probe_pi.py:412-421`; ownership context at `:584` and `:652-659`.
   - `run_pi` cleans up only `TimeoutExpired`. A `KeyboardInterrupt` from `communicate()` unwinds the function without terminating/reaping its process. The surrounding `run_flow` finally owns only the gateway and fixture; it cannot recover the Pi handle. The advertised complete owned-process cleanup therefore fails for an ordinary cancellation of the development probe.
   - **Direct reproduction:** loaded the unchanged module with bytecode writing disabled, wrapped `Popen` only to retain the returned handle, and called `run_pi` on a reviewer-owned Python child sleeping for 12 seconds. A timer sent SIGINT only to the reviewer runner after 0.3 seconds. `KeyboardInterrupt` propagated, and the owned child was still running after `run_pi` unwound. The reviewer then explicitly terminated/reaped it: remaining owned process count **0**, temporary directory contents **0**. No actual Pi or user file was involved. See `targeted-checks.json`.
   - Fix: give the owned Pi process unconditional cleanup in a `finally` or equivalent ownership guard, preserving exception propagation and the existing timeout result. Cleanup should terminate/kill when necessary, reap, and close the captured pipes. Validate interruption with a bounded synthetic child in addition to the successful timeout/normal paths.

## Minor

1. **An unavailable optional clone is reported as a known version difference.**
   - Location: `product/scripts/probe_pi.py:84-104`, `:695-697`.
   - `reference_clone()` deliberately returns null version/commit when no research clone is present, but `None != package["version"]` produces `differs_from_installed=true`. That claims evidence of a difference where the standalone product probe has no reference version.
   - **Direct reproduction:** redirected only the module's in-memory `ROOT` to an empty reviewer-owned temporary location and called `reference_clone()`. Result: `version=null`, `commit=null`; the current comparison emitted `differs_from_installed=true`. See `targeted-checks.json`.
   - Fix: retain unknown/null comparison until a reference version is actually available. The clone must remain optional. Source-supported adjacent concern: plain `git -C <clone> rev-parse HEAD` searches enclosing repositories, so only assign a clone commit after confirming the discovered repository is the intended clone. No enclosing-repository case was executed because this review did not initialize or mutate Git repositories.

## Assessment

**FIXES REQUIRED.** Normal positive/negative compatibility is supported by the independent specification execution, and the harness's scope and acceptance structure are sound. The directly reproduced cancellation leak violates the current cleanup contract and should be repaired before Task 5; the optional provenance comparison is a small accuracy correction. Neither finding changes the accepted Task 3 Rust gateway conclusions.

Evidence: `targeted-checks.json`, `fingerprint-check.json`, and the separate `../task4-spec-review/README.md` positive/negative execution evidence. Targeted checks retain only fixed structural observations and hashes; no raw prompts, request bodies, tokens, stdout/stderr, or user data are stored.
