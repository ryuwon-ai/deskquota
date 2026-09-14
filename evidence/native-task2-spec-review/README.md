# Native Task 2 independent SPEC review

Result: **changes_required**. Status: **FINAL_HOLD**. `execution_finished=true`.

This is the fresh SPEC review, not QUALITY review. No product source, tests, documentation, reference clone, user configuration, Git state, old artifact, or protected executable was changed. Runtime ownership has ended after cleanup.

## Findings

### F1 — P1: interrupted reconfiguration destroys the existing config

`product/src/setup/mod.rs:677–679` truncates the existing file before writing replacement bytes. The atomic temporary-file branch applies only when the destination does not exist. A failed write or interruption can leave a partial TOML config and no intact original. This violates safe application of an existing configuration; deferring the general client ConfigPatch framework to Task 3 does not make destructive core-config saves acceptable.

Direct repro: `probe.py`, `partial_write`. An owned PTY child has `RLIMIT_FSIZE=100`; it edits only the port of a valid 417-byte config and chooses Save only. The native process exits by SIGXFSZ (`-25`), leaving the existing config **100 bytes** long. No real file or filesystem quota was changed. Use same-directory protected temporary output and atomic replacement for existing files, preserving permissions/ACL; failure before replacement must preserve the original bytes. No general client patch framework is required to fix this.

### F2 — P1: applying a stale preview silently overwrites a concurrent edit

`product/src/setup/mod.rs:574–592` never compares the current file to the draft's original bytes/fingerprint. The inode check at `:668` only covers changes between stat and open, not edits since the wizard loaded its draft.

Direct repro: `probe.py`, `concurrent_edit`. Pause the native wizard at Apply, externally change `concurrency = 3` to `4` in the owned fixture, then accept default Save only. Exit is **0**, and the wizard restores stale `3`, silently discarding `4`. Compare the original snapshot immediately before applying and require a new preview on conflict. Also serialize cooperating setup writers. The requirement is detectable-conflict protection, not an impossible CAS guarantee against every external editor race.

### F3 — P1: accepting existing defaults changes authentication and removes enabled routes

`product/src/setup/prompts.rs:184–189` offers hardcoded `authorization` and `LLMGW_UPSTREAM_AUTH`, even when the existing `Auth::Env` has a different header and variable name. Separately, `:142–170` rebuilds the first root's endpoints from only three generation choices; `SetupDraft::connection` at `product/src/setup/mod.rs:282–283` replaces the list, discarding valid `models` and `messages/count_tokens` endpoints before the later “Keep all existing models and routes unchanged?” question.

Direct native PTY repros: `probe.py`, `auth_defaults` and `endpoint_defaults`. Accept every displayed default and Save only. Both exit **0**. The first loses `x-api-key` / `SPEC_EXISTING_AUTH` and saves the hardcoded reference. The second loses both Models and CountTokens endpoints despite accepting “Keep all existing models and routes unchanged?”. A simple one-model/Responses/Auth::None control does preserve exact bytes, so this is a concrete reconfiguration gap rather than blanket serialization failure. Populate authentication defaults from the existing variant and preserve endpoints the generation selector does not edit.

### F4 — P2: saved login/client intents are cleared on the next native setup

`product/src/setup/prompts.rs:383` always defaults login to false, and `:390–395` never initializes client selections from `draft.clients`. Loading pending metadata in `SetupDraft::from_loaded` does not preserve it through the real dialog.

Direct repro: `probe.py`, `pending_defaults`. Start with valid pending metadata containing `login_requested=true` and clients `[pi,codex]`. Accept defaults and Save only. Exit is **0**; login becomes **false** and clients become **[]**. Shared-other-PC and separate-input/output flags remain true. Retain the loaded values as rerun defaults, with false/empty only for first setup. The existing scripted pending test checks loading the draft, not the real adapter defaults that overwrite it.

### F5 — P2: setup rejects an already-valid request-bounded model

`product/src/setup/mod.rs:381–387` requires every configured model to have a fallback output bound, although core validation accepts omitted bounds (`product/src/config/validate.rs:214–225`) and the core contract uses the explicit request cap first. `product/src/setup/summary.rs:123–125` also assumes the optional fallback always exists.

Direct repro: `probe.py`, `request_bounded`. The owned config has one model without `max_output_tokens` and unlimited quota. Offline doctor exits **0**. Native setup accepts “Keep all existing models and routes unchanged?” but fails at summary with **exit 1**, “each model requires a nonzero output bound.” Original bytes remain intact. Preserve this valid existing configuration and disclose that reservation relies on request caps; do not require replacing models or silently inventing a provider bound. A newly entered fallback can still require a nonzero value.

### F6 — P1: unchanged Save and start restarts a worker after saying no restart is required

`product/src/setup/mod.rs:604–609` restarts every non-stopped existing worker, regardless of whether configuration changed. The corresponding unchanged summary at `product/src/setup/summary.rs:88–89` says “no restart required by setup edits.” The native flow never discloses queue cancellation or active-request drain for this case.

Direct repro: `probe_running.py`. Start an owned unlimited-quota worker, accept every setup default, then choose Save and start. Setup exits **0**. The fingerprint remains identical, but authenticated identity changes from **PID 99899** to **99902**, with a new nonce. Captured preview says no restart is required and contains no cancellation/drain disclosure. Both workers were cleaned up; no inference request was made. Reuse authenticated `on` for the unchanged running configuration, or explicitly preview/select restart and its actual impact before applying. In-flight cancellation impact is source-supported by the existing restart implementation; this probe deliberately used an idle worker.

### F7 — P2: invalid configuration/required inference arguments return operational exit 1

`product/src/cli.rs:106` propagates config parsing errors, and `:267–270` checks missing inference arguments inside execution. `product/src/main.rs:5–8` maps both to failure code 1. The agreed CLI contract reserves **2** for invalid arguments/unconfigured/invalid config and **1** for operational failures.

Direct repros: `probe.py`, `invalid_cli` and `missing_inference_args`. Malformed owned TOML gives **exit 1** for both `doctor` and `on`; `doctor --inference` without model/output bound also gives **exit 1**. These cases perform no provider calls. Enforce required inference arguments during CLI validation and classify configuration/argument failures separately, while retaining operational network/auth failures as 1.

## Verification and scope

Fresh independent execution:

```sh
cd /Users/ryuwon/Desktop/ryuwon-project/llm-gateway-research/product
CARGO_TARGET_DIR=/Users/ryuwon/Desktop/ryuwon-project/llm-gateway-research/product/target/native-task2-spec cargo test --locked --offline --test setup_contract --test config_contract --test lifecycle_contract
```

The command ran in an owned subprocess group with a 360-second outer deadline and bounded termination on timeout. It completed normally: **74 passed, 0 failed** — config 19, lifecycle 25, setup 30. See `focused-tests.json` and `focused-tests.log`. This freshly compiles the reviewed source in the isolated target, including the configured Rust 1.88 toolchain and locked dependencies. No default `target/debug` evidence was used.

```sh
cd /Users/ryuwon/Desktop/ryuwon-project/llm-gateway-research
python3 evidence/native-task2-spec-review/probe.py
python3 evidence/native-task2-spec-review/probe_running.py
```

These independent drivers ran **8 actual macOS PTY sessions**, plus bounded offline/control CLI calls, across **10 named scenarios**. The counterexample drivers exit 0 when observation collection succeeds; that is **not** a product/spec PASS. Results are in `probe-results.json`, `running-noop-results.json`, and per-case `.pty.txt` files. Synthetic loopback fixtures contain no credential values, provider prompts, or user data. The only generated-request tests were the reviewed owned fixtures inside setup_contract, not real or paid APIs.

| Contract group | Evidence and assessment |
|---|---|
| Typed drafts, ordered scripted flow, presets, nonzero quota, concurrency 1–16, invalid typed URL/proxy | Source inspection and fresh setup tests; covered. |
| Non-TTY, cancel/back, bare existing on, relative config, save-only vs authenticated readiness | Fresh setup/lifecycle tests; native rerun PTYs add the failures above. Original 7-case first-run PTY log is historical/reused only. |
| Existing config bytes/comments/inline tables, retry/accounting/cancel settings | Narrow scripted controls pass; native adapter preservation fails F3/F4, valid missing fallback fails F5. Safe save/conflicts fail F1/F2. |
| Offline doctor, GET-only listing, one explicit generation, failed/empty/error/null Responses evidence | Fresh setup tests and inspected CLI/response validator; no catalog/pricing/telemetry/update path added. Invalid-input exit contract fails F7. |
| Env entire header value, explicit proxy/loopback bypass, merged CA, no retry/redirect | Fresh setup proxy/TLS fixtures plus source inspection of pooled `transport/upstream.rs`; disabled implicit proxies and startup-built CA/client retained. |
| Pending metadata vs actual clients/autostart | Minimal metadata is separate from worker config, with no hot-path read or credential value. Actual client patch/autostart tasks remain deferred. Native rerun fails F4. |
| Preview paths/URLs, auth reference, estimated/shared quota, unverified capabilities, reservation fallback/fairness explanation | Present in source and covered in narrow summary test. Running-worker impact fails F6. Summary currently displays only the first configured model/root; no claim is made that all multiroot details are previewed. |
| Native dependency/runtime/OS boundaries | Rust executable; command-only Dialoguer 0.12 with default features disabled; no mandatory Python/Node runtime. Actual macOS exercised. Windows/Linux runtime remain UNVERIFIED. |

Source comparison: baseline archive/manifest to current hold yields **25 changed paths (16 modified, 9 added)**. `source.diff` was generated without Git or extraction into the product. Additional transport/config changes support the exposed proxy/CA option; adjacent test changes supply the new fields. No unrelated product feature or policy change was identified. Parent/research roots and product have no `.git` directory.

Reused evidence is explicitly bounded: the implementer's 246-test all-targets result, fmt/check/clippy, release build, initial setup RED, and prior PTY suite remain historical claims backed by their logs. The RED log contains missing `llmgw::setup` / `config::parse` compiler errors; I did not revert source or independently repeat historical RED. Current review independently ran the 74-test subset and release-binary counterexamples, not the whole benchmark/full-suite matrix.

## Identity, cleanup, and hold

`before.json` / `after.json` independently hash **all 68 source files** and **12 held/protected entries**, including the nine parent-listed identities and the existing `target/release/llmgw*` files. **Source drift 0; protected drift 0.** Current held release SHA-256 remains `8b33e86bcc7a612316dfb37a686fa768d3371d8fdcd8da7a88919ed5bcf4d56b`. Both protected accounting executables remain `e16ba39c54fcec9bc5ce4fb4039d551dccdd5cdfd3bec9e0a7c412e58bd4364f` (8,697,040 bytes). Source hold manifest/archive/final-hold hashes and accounting pilot/prior archive hashes match the supplied values.

`probe-cleanup.json` records all seven first-driver PTY children waited and scratch removal. `running-cleanup.json` records authenticated off, stopped status, its PTY child waited, and scratch removal. The suite completed; final process inventory finds **no visible llmgw/cargo/rustc processes**, no remaining `llmgw-setup-*` temporary directories, and no review scratch directory. All retained new evidence is under this review directory; new compilation output is only `product/target/native-task2-spec`.

The implementer's unquoted-heredoc incident is not independent proof that default debug output was uncontaminated. This review did not execute that output or retry the unavailable Windows build. No model start/download, startup registration, current-user client patch, real provider call, Git operation, accounting auditor, or benchmark rerun occurred.

**FINAL_HOLD — execution_finished=true.** Required next action: fix the seven SPEC findings and obtain SPEC re-review before starting QUALITY.
