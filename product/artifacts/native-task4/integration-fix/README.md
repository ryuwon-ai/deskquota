# Native Task 4 integration fix — FINAL HOLD

Status: `DONE_WITH_CONCERNS — FINAL_HOLD`

`execution_finished=true`; product writer/runtime/test ownership is released for a fresh SPEC review and then fresh QUALITY review. This phase implements only the approved native Pi, Claude Code, and Codex client profiles and their reversible CLI/setup integration. Earlier Native Task 4 attempts, failed captures, and provisional final logs remain preserved.

## Implemented result

- `llmgw connect pi|claude|codex` prepares a concrete client-file preview bound to the desired gateway fingerprint, route, model, protocol, client selection, and ConfigPatch hash. Noninteractive apply requires that exact hash before any route activation or worker restart.
- Missing fixed routes use the existing `SetupDraft`/lifecycle path. Authenticated readiness precedes every client mutation; failed restart and SaveOnly leave client bytes unchanged.
- Interactive setup treats gateway and client files as separate resource stages. It reuses the same in-memory reviewed client plan after confirmation and defaults to the actually resolved native client directory, including native environment overrides.
- Explicit client locations that differ from the current native environment print the exact `PI_CODING_AGENT_DIR`, `CLAUDE_CONFIG_DIR`, or `CODEX_HOME` needed for later launches. llmgw does not mutate the future shell environment.
- Pi owns one `providers.llmgw` object. Claude owns only its selected settings keys and a reviewed public `ANTHROPIC_API_KEY=llmgw-local-only` client-startup placeholder. Codex owns one `model_providers.llmgw` object in its separate named profile. Automatic connection under upstream `auth=forward` is refused.
- ConfigPatch protects before-images, redacts local-token objects, uses stable adjacent locks and same-directory replacement, refuses unsafe native TOML values before writes, preserves unrelated edits, and reports disconnect conflicts. Restored journals do not reauthorize a recreated provider object.
- Claude project-local settings require both explicit confirmations plus actual read-only Git tracked/ignore checks and private path checks.

## Test evidence

The final source state passed:

- Full locked debug suite: 332 passed, 0 failed across 16 test-result rows (`full-test-final3.log`).
- Focused release client profiles: 17 passed, 0 failed (`client-profiles-release-final3.log`).
- Python driver boundaries: 7 passed, 0 failed (`python-focused-final3.log`).
- `cargo fmt --check`, locked all-target `cargo check`, and locked all-target Clippy with warnings denied: passed.
- Final locked release build: passed after the release-focused test executable, so the held executable is the ordinary final release build.
- Native setup PTY: passed, including selected Pi, authenticated-readiness-before-client-write, independent preview/confirmation, native environment default, disconnect, and cleanup.

Installed-client probes all match release binary SHA-256 `c7258f5109882596b90d33d479187093aa2ef47dd42fb64e47e7395ddfe06355`:

- Pi 0.84.2: picker/list, selection, Chat Completions inference, exact read-tool call/result and follow-up request, header stripping, gateway-off failure with upstream 0, disconnect, and actual reload passed.
- Claude Code 2.1.63: generated profile without any driver auth environment, selection, Messages inference, exact `Read` call/result and follow-up request, placeholder/auth-header stripping, gateway-off failure with upstream 0, disconnect, and actual reload passed. The preserved pre-fix control exited 1 with upstream 0 when neither a driver value nor generated placeholder existed.
- Codex 0.154.0: malformed named-profile negative control, profile load, selection, Responses inference, exact `exec_command` read/result and follow-up request, header stripping, gateway-off failure with upstream 0, disconnect, and actual reload passed. Catalog listing remains unverified because the Responses route is not a Codex catalog.

## Frozen identities and preservation

- Final source manifest: `source-manifest-final3.json`, 88 files, SHA-256 `b2318a335b58914eb3f44dac22f004ffa0e680e2889869047c3a4b520f6adbb1`.
- Final source archive: `source-hold-final3.tar.gz`, 278,283 bytes, SHA-256 `3cf0aa370b05defa3d5cf8304bc6a1b9e7935a8bd13d21589573b4e601bb62c3`.
- Task 3-relative changes: 19 files (`changed-files-final3.json`).
- Release binary: 9,932,352 bytes, SHA-256 `c7258f5109882596b90d33d479187093aa2ef47dd42fb64e47e7395ddfe06355`.
- Debug binary: SHA-256 `c10251a7512e99a4e8c63d87997402989844f0341345cc964daa45eafb03ef47`.
- Default-feature debug rlib: SHA-256 `3e94daf4d97416dcd004788ce20659706de1a705fbe2ae8f9035e8ca7e6ac560`.
- `Cargo.lock` and `rust-toolchain.toml` retain the accepted Task 3 hashes.
- All 426 Task 2 protected files and 357 Task 3 additional protected files match with zero drift. The final Task 3 SPEC hold remains `2effc0f1adc2eef043b5f6a2519f04c87ecc1749656c8560ef2d66eaacf268d1`. The established 40 benchmark raw files and 10 accounting result raw files remain present; no historical matrix was rerun.
- Source remained stable after capture, archive membership matched all 88 source inputs, no owned process or fixture directory remained, and the bounded artifact privacy scan found no generated credential value.

## Limits

Actual runtime evidence is macOS only; Windows and Linux remain unverified. Claude model discovery is unsupported for 2.1.63 and was not requested. Codex catalog listing is unverified. Claude and Codex gateway-off controls hit the bounded eight-second timeout with zero upstream calls and are recorded only as expected failures. No new latency or RSS claim follows from the binary size, and historical performance/accounting audits were not rerun. The gateway does not claim control over every client network operation, telemetry, or update behavior.
