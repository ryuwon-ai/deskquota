# Native Task 2 F8 SPEC re-review

**PASS — FINAL_HOLD; execution_finished=true.** F8 is fixed, the original F6 idempotent behavior is preserved, and the tested adjacent preview/apply boundaries behave as specified. No actionable SPEC finding remains in this review. This is not QUALITY approval or cross-platform deployment approval. Runtime/test ownership is explicitly released after cleanup.

## Direct native verification

All actual CLI sessions used the held fix release `product/target/native-task2-f8-fix/release/llmgw`, SHA-256 **`c890b6a498095bdba572d19b921b1865e48927af6807b767156f2e2f3a9ef1ba`**, 9,316,432 bytes. Fixtures were owned, local, and used unlimited quota to separate readiness from admission hold. No real provider or generation request was used by the independent PTY drivers.

| Scenario | Observed result |
|---|---|
| Exact F8: Save only → rerun setup → Save and start | First save exits 0 and preserves the worker with pending restart. Second exits **0**, applies the saved fingerprint, changes worker identity, and clears pending restart. |
| External edit predates setup | Preview contains both fingerprints, cancellation/drain impact, and desired-fingerprint readiness. Save and start applies the desired config and clears pending restart. |
| Original F6: unchanged saved/running configuration | Exit 0; authenticated PID, nonce, fingerprint, address and start time remain identical. |
| Disk B is pending but desired setup reverts to running A | Preview identifies a matching worker. Original A bytes are restored; exit 0; same worker identity; pending restart clears without restarting. |
| Worker cannot be authenticated | Explicit unverified preview; Save and start exits 1 before config/pending writes; restored control fixture confirms the same worker identity. |
| New worker mismatch after a stopped preview | Save and start exits 1 with the missing restart-preview explanation; config and worker remain unchanged, pending metadata is not written. |
| Disk edit while setup waits for the lifecycle restart lock | Expected-fingerprint error, exit 1; the external bytes survive and the old authenticated worker is **not stopped**. |
| Disk edit while setup waits for the lifecycle on lock | Expected-fingerprint error, exit 1; external bytes survive and state remains **stopped**, with no unexpected worker start. |
| Restart-required preview becomes matching before apply | Exit 0; the already-matching worker keeps its identity; no redundant restart. |

The exact saved-pending repro applied fingerprint **`c13867aa5a5867b16bba35a7df76688cf7c43c67affab065ed453bedd37e62ae`**, with authenticated worker PID 41913 at `127.0.0.1:52360`, and `pending_restart=false`. Both actual wizard sessions and final control status are recorded in `saved-pending-results.json` and the two `saved-pending-*.pty.txt` files. The worker was subsequently authenticated-stopped.

The independent drivers ran **10 actual macOS PTY sessions**: saved-pending 2, unchanged identity 1, and seven adjacent boundary sessions. `boundary-results.json`, `running-noop-results.json`, and their transcripts retain observations. `assessment.json` checks the expected results explicitly; all 11 assertions are true. One reused identity-probe wording flag still searches the old phrase including “by setup edits”; its false value is a stale text selector, not a runtime failure. The assessment checks the current transcript's matching-worker/no-restart wording and passes.

Exact commands:

```sh
cd /Users/ryuwon/Desktop/ryuwon-project/llm-gateway-research
python3 evidence/native-task2-spec-review/rereview-f8/probe_saved_pending.py
python3 evidence/native-task2-spec-review/rereview-f8/probe_running.py
python3 evidence/native-task2-spec-review/rereview-f8/probe_boundaries.py
```

`support.py` copies this review's earlier fixture/PTY definitions into the new evidence directory and selects the held F8 binary. Each driver uses bounded PTY waits and subprocess deadlines, restores any temporarily renamed owned control fixture, authenticates worker shutdown, waits its PTY children, and removes scratch only afterward.

## Source and focused regression verification

The independently generated `source.diff` covers **8 changed paths** relative to the preceding SPEC-fix archive. Code inspection confirms:

- `setup/persist.rs` computes desired bytes' fingerprint and compares it with authenticated worker identity, rather than inferring required restart from the current wizard's disk edits. `RuntimeImpact` distinguishes matching, restart-required, stopped/new, and unverified cases.
- `setup/prompts.rs` renders that typed impact before selection and carries explicit `SaveAndRestart` intent only when restart was disclosed. Apply re-inspects runtime before writing and refuses an unverified or newly unpreviewed mismatch.
- `setup/summary.rs` discloses desired/worker fingerprints, cancellation of queued work, up-to-ten-second active drain and readiness for the desired fingerprint. It does not claim simple on can apply a known pending worker configuration.
- The narrow `lifecycle::on_expected` and `restart_expected` helpers check the intended fingerprint under the existing lifecycle operation lock. Public Native Task 1 entry points continue to use their original behavior. No process override or test-executable workaround was added.
- Apply validates the returned authenticated readiness fingerprint against the intended rendered bytes, rather than merely requiring any fingerprint. Persistence conflict/atomic-publication protection and explicit network doctor implementation were not replaced or bypassed.

Fresh command, run in an owned subprocess group with a 360-second outer deadline and bounded timeout teardown:

```sh
cd /Users/ryuwon/Desktop/ryuwon-project/llm-gateway-research/product
CARGO_TARGET_DIR=/Users/ryuwon/Desktop/ryuwon-project/llm-gateway-research/product/target/native-task2-spec cargo test --locked --offline --lib --test setup_contract --test lifecycle_contract
```

**76 passed, 0 failed**: library 11, lifecycle 25, setup 40. See `focused-tests.log` and `focused-tests.json`. Compilation used the configured Rust 1.88 toolchain and isolated target. The unchanged config-only suite was not needlessly repeated; it passed in the previous independent review. The current setup/lifecycle suites include the affected original preservation and identity contracts.

The implementer's 95-test focused run, 261-test full run, static checks, release build and four-session final PTY log were inspected as **reused evidence**. This review independently ran the 76 affected tests and the 10 actual release PTYs above. Direct integration-test restart would use the test runner as `current_exe`; successful actual binary restart/readiness evidence comes from the PTYs, not from treating that test-runner behavior as product success.

## Identity, cleanup, and limitations

`before.json` and `after.json` independently hash **70 source files** and **271 protected files**, including the held F8 outputs, previous implementation/fix artifacts, original reviews, prior accounting binaries/pilot and original target/release outputs. **Source drift 0; protected drift 0.** The original rereview README remains SHA `dd58b9989eb4b0488f2dffd1270b448f4efe626698fd02e82caf8e8cdd16bab6`; no earlier evidence was rewritten.

Current source manifest remains **`2107b93a752ebcbefbd9f2214c1688f6c527883ba6e061cbc1fbaa80781b11cb`**; archive remains **`682f1dde419f2fa35ea4adadf1816b889e9c2b927ce6820282733b10cbec0b89`**; implementer final-hold remains **`f19162307fc7ae90351f1024b4dc950e13b802c84baaf331c7e4a327c3991a85`**. Final release/debug and all prior protected identities match their before snapshots and supplied hold values.

All driver cleanup receipts confirm PTY children waited, owned workers authenticated-stopped, restored control fixture, and scratch removal. The final scoped process inventory finds no llmgw/cargo/rustc/focused-test processes. No new llmgw temporary directory or rereview scratch remains. Compilation output is retained only in the permitted isolated target.

Runtime verification is macOS only. Linux/Windows runtime and real providers/client integrations/login execution remain unverified. This review did not repeat the unavailable Windows build, benchmark/accounting matrix, model startup/download, real or paid API calls, current-user settings, OS registration, Task 3, Git actions, or QUALITY. The previously reviewed Windows recovery helper was unchanged; no new native Windows claim is made.

**FINAL_HOLD; execution_finished=true; SPEC PASS.** No required work remains in this SPEC review. Runtime/test ownership is released to the parent; this reviewer remains on hold pending explicit follow-up.
