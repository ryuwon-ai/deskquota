# Native Task 2 SPEC re-review

**changes_required — FINAL_HOLD; execution_finished=true.** The original seven counterexamples are fixed. One adjacent regression remains in the saved-pending configuration flow, F8 below. No QUALITY review has started. Product runtime ownership is released after cleanup.

## F8 — P2: Save only followed by setup Save and start cannot apply the pending config

Location: `product/src/setup/persist.rs:75–86,118–121`; preview at `product/src/setup/summary.rs:86–91`.

`config_changed` compares the final draft only with the bytes read at this wizard's start. The lifecycle status is reduced to `was_running`, discarding `pending_restart` and the authenticated worker fingerprint. When a previous wizard already saved a change, this new wizard has no additional edit, so it incorrectly selects `on` rather than offering the necessary restart. The summary consequently promises no restart is required and idempotent on, even though the running worker still has the older configuration.

**Direct native release repro**, `probe_saved_pending.py`:

1. Start an owned unlimited-quota configuration and observe authenticated identity.
2. Run actual setup, change the port, and choose Save only. Exit **0**; the existing worker identity is preserved and `pending_restart=true`.
3. Run actual setup again, accept existing defaults, and choose Save and start.
4. Preview says `no restart required by setup edits` and `Save and start uses authenticated idempotent on`. The command exits **1**, `error: restart_required`. `pending_restart` remains true and the saved configuration is not applied.

Observed saved fingerprint: `4d1b9a9e704c0ca0ff56f37334115b6694f736f7d6e11880b52beb62fbe19c58`. Authenticated worker afterward still has fingerprint `2d55166e5eeec0db61652eebdd8e4b9f5a0971e760a5d2ee969d16dd9faf8c61`, PID 24927, and its original nonce/address. No generation request was made. The worker was then stopped through authenticated `off`, and stopped status confirmed.

Evidence: `saved-pending-results.json`, `saved-pending-save-only.pty.txt`, `saved-pending-save-start.pty.txt`, `saved-pending-cleanup.json`. `probe_adjacent.py` also reproduces the same behavior when an owned external edit predates setup, rather than the previous setup doing the save.

**Required fix:** distinguish the saved draft fingerprint from the running worker's authenticated fingerprint in the preview and apply decision. If saved changes are pending, disclose queue cancellation/drain and let the selected Save and start perform the explicitly described restart with readiness verification. If worker and saved fingerprint already match, retain the now-correct idempotent behavior. Keep the snapshot checks and lifecycle race/identity guarantees. This is within Task 2's preview, Save only/pending restart, rerun recovery, and Save and start contract; no Task 3 client patch is needed.

## Original findings: independently rechecked

| Finding | Fresh native observation | Result |
|---|---|---|
| F1 interrupted existing save | Same owned RLIMIT_FSIZE=100 repro exits SIGXFSZ (-25), but the **417-byte original remains byte-identical**. | Fixed on macOS; Windows safety model discussed below. |
| F2 stale preview | External concurrency edit survives; setup exits 1. | Fixed. |
| F3 authentication/endpoints | Accepting all defaults preserves exact original config bytes, Env header/name, Models and CountTokens endpoints. | Fixed. |
| F4 pending intents | Login true, Pi/Codex selections, and shared/separate flags survive real dialog rerun/save. | Fixed. |
| F5 omitted fallback bound | Offline doctor and real setup both exit 0; request-bounded original bytes remain unchanged. | Fixed. |
| F6 unchanged active config | Save and start exits 0 with identical authenticated PID, nonce, fingerprint, address, and start time. | Original unsolicited restart fixed; F8 covers previously saved pending changes. |
| F7 error classes | Malformed doctor/on config and missing inference arguments now exit 2. | Fixed; live malformed on remains operational 1. |

Original-counterexample drivers were copied into this new directory, with only the held binary/output roots and successful request-bounded completion step adjusted. Original scripts/results were not overwritten. `probe-results.json` and `running-noop-results.json` hold fresh results. These observation drivers exit 0 after successful collection; product/spec acceptance is based on recorded fields, not driver exit alone.

Additional independent native checks:

- Metadata-only Models root without output fallback: unchanged save succeeds, no generation endpoint invented.
- Cooperating writer: holding the owned setup lock yields explicit refusal, exit 1, original bytes unchanged.
- Edited macOS config: mode **0640**, owner/group and explicit extended ACL are preserved; inode changes, confirming replacement rather than in-place truncation.
- Alternate supplementary group: replacement explicitly refuses inability to preserve ownership; bytes and group remain intact.
- Snapshot after a forced status delay: owned worker is temporarily SIGSTOPed; the driver waits for actual Apply acknowledgement, edits at **0.108 seconds**, and observes refusal at **0.504 seconds**. The external edit and original worker identity survive, then the worker resumes and is authenticated-stopped. This supports the post-status snapshot path, alongside direct inspection of `write_protected_file` comparing the original draft bytes. The earlier timing attempt in `adjacent-results.json` did not prove that ordering; `status-delay-results.json` is the synchronized evidence.
- Existing live worker with malformed disk edit: on exits 1 with `restart_required`, status still authenticates the same identity, and off succeeds. Invalid config classification has not replaced the live identity recovery path.

## Fresh tests, source inspection, and platform limits

Exact focused command, wrapped in an owned subprocess group with a 360-second deadline and bounded timeout teardown:

```sh
cd /Users/ryuwon/Desktop/ryuwon-project/llm-gateway-research/product
CARGO_TARGET_DIR=/Users/ryuwon/Desktop/ryuwon-project/llm-gateway-research/product/target/native-task2-spec cargo test --locked --offline --lib --test setup_contract --test config_contract --test lifecycle_contract
```

**89 passed, 0 failed**: lib 9, config 19, lifecycle 25, setup 36. The library count includes **3 freshly executed Windows recovery model tests**. See `focused-tests.log` and `focused-tests.json`. This uses locked source compilation in the permitted isolated target and Rust 1.88; no default debug binary is used.

Actual release PTYs use only the held fix executable `product/target/native-task2-spec-fix/release/llmgw`:

```sh
cd /Users/ryuwon/Desktop/ryuwon-project/llm-gateway-research
python3 evidence/native-task2-spec-review/rereview/probe.py
python3 evidence/native-task2-spec-review/rereview/probe_running.py
python3 evidence/native-task2-spec-review/rereview/probe_adjacent.py
python3 evidence/native-task2-spec-review/rereview/probe_status_delay.py
python3 evidence/native-task2-spec-review/rereview/probe_saved_pending.py
```

Those drivers exercised **17 native PTY sessions** total, plus bounded offline/local lifecycle CLI calls. All inference/network tests in the focused Rust suite used owned fixtures. No real/paid provider call, model download/start, current-user client setting, startup registration, Git operation, accounting auditor, or benchmark matrix ran.

The current source diff has 12 changed paths relative to initial Native Task 2. Persistence and explicit network doctor operations are split into `persist.rs` and `network.rs`. The network body is relocated without a new provider/discovery or worker hot-path operation. Linux uses the already-installed rustix fs feature and `lgetxattr` to refuse POSIX ACL replacements it cannot preserve; no libacl dependency was added. This is source-supported, not a Linux runtime claim.

Windows is not treated as safe merely by documentation. [Microsoft's ReplaceFileW contract](https://learn.microsoft.com/en-us/windows/win32/api/winbase/nf-winbase-replacefilew) documents loss of the destination on error 1176 without a backup parameter. The inspected helper creates and syncs **independent protected original and replacement copies**, verifies their content and the source, retains/reports both on ambiguous replacement or failed post-publication verification, and removes recovery copies only after verified success. The current implementation uses an independent candidate copy, not the hard link described at an earlier checkpoint. Three host tests cover injected 1176, stale source rejection before replacement, and verified-success cleanup. Those tests plus code inspection support the recovery decision logic, **not actual Windows replacement/ACL/runtime validation**. Linux and Windows runtime remain UNVERIFIED; the unavailable SDK/aws-lc cross-build was not retried.

The implementer's full 255 tests, fmt/check/clippy, final release build and seven-case first-run PTY logs were read as historical evidence. They were not substituted for this review's fresh 89 tests and 17 native PTYs. No claim is made that the full benchmark or OS/client matrix was independently rerun.

## Evidence chronology and preservation

The initial review README had provisional SHA `825c2757d606fc862db05a418ff37bb4be8423d643474f67f98dc21f869d6d17`. Before my original final response I corrected four source references only: setup `mod.rs:284` → `282–283`, config `validate.rs:232–248` → `214–225`, CLI `:105` → `:106`, and `:253–256` → `:267–270`. I refreshed the original final-hold README hash to **`3b0b0318b9d064ae0a94474895f717c5dcf054f0e2afe2c81cc8371f7f93c573`**. Findings, severities, repros, results and counts did not change. The parent had captured the earlier provisional hash; the implementer later pinned the finalized one. This re-review confirms and preserves the final 3b0 hash. No original artifact was rewritten during re-review.

`before.json` and `after.json` independently hash all **70 current source files** and **148 protected files**, including original review files, fix artifacts, initial held binary/source archive, prior accounting identities and original target/release outputs. **Source drift 0; protected drift 0.** Held fix release remains **`821973c23b30b62270a08b19db3b73399275c06d44f2d0d81526c7426fb91530`**, 9,298,000 bytes. Manifest/archive/final-hold identities match the supplied hashes.

Each driver's cleanup JSON records all owned PTY children waited, workers authenticated-stopped, and scratch removal. Final process inventory has no visible llmgw/cargo/rustc or focused-test executable; no new llmgw temporary directories or rereview scratch remains. Compilation output is retained only in the permitted isolated target. No source file or protected evidence changed.

**FINAL_HOLD; execution_finished=true.** Original F1–F7 fixed; F8 requires correction and another SPEC re-review. Runtime ownership is available for the implementer; QUALITY remains deferred.
