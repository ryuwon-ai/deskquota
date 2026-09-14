# Native Task 4 QUALITY Q1/Q2 rereview

**Verdict: QUALITY PASS. Q1 and Q2 are closed.** The same quality reviewer independently reran the retained counterexamples and bounded adjacent checks against the new held release/rlib. No new actionable quality issue was established. Final same-SPEC confirmation remains a separate next step; this is not Task5 acceptance.

## What the source correction does

- **Q1:** `product/src/cli.rs:431,456–469` retains the displayed runtime action, validates the same reviewed client plan, and obtains authenticated status after confirmation. A changed action is refused with client files unchanged. `cli.rs:495–535` confirms the desired fingerprint through the fresh status or the existing activation helpers before the client write. Config-changing apply also gets a new status check. The unbounded human wait no longer lies between the only status check and client write.
- **Q2:** Pi and Codex attach an internal owned-provider precondition when preparing a reconnect. `clients/mod.rs:363–377,391–394` revalidates it before activation/application. `config_patch/apply.rs:85–99,131–149` loads the journal without first resetting a retired status and checks the precondition while holding the existing journal lock and resource lock. Only after a successful check can it publish an applying journal. The journal lock remains held across apply; each resource write still uses its existing resource lock. This prevents the cooperating same-journal disconnect from retiring ownership between the protected check and apply.

The fix changes ten existing source paths, with no additions/removals. It adds no dependency, service, watcher, resolver, runtime interpreter requirement, or HTTP/quota/accounting/fairness hot-path change. The small runtime-action enum and internal owned-key requirement directly encode the two missing preconditions. Claude scope code changes only the private check-type name; its canonical/Git checks remain intact. The existing nonblocking CLI concentration observation remains; no broad refactor was required to close these bugs.

## Independent fresh execution

`execution.json` records exact compilation/test commands, exits, assertion counts, and cleanup. `readiness_probe.py` is a new copy of the original reviewer PTY scenario, adjusted only for the new held path and the required safe outcome. It is not the implementer's correctness driver.

| Check | Actual result |
|---|---|
| Q1 original stop-during-confirm scenario | Authenticated running before confirmation → owned `off` → confirmed stopped → user yes → setup exit **1**, no client config, journal `not_connected`, no client-applied report. Config digest unchanged. |
| Q1 healthy control | Setup exit **0**, client applied, post-confirm worker fingerprint exactly equals the reviewed/config digest; owned worker then stopped for cleanup. |
| Q2 retained Pi reconnect after journal retirement and exact old-byte recreation | Fresh prepare, retained snapshot check, and retained apply all refuse; client **and retired journal** bytes unchanged. |
| Q2 retained Codex reconnect after the same retirement sequence | Same refusal and byte-preservation result. |
| Normal reconnect/disconnect | All three native profiles reconnect to the new model and then restore without conflicts or a retained synthetic token. |
| Initial protected-journal publication denial | Client/journal unchanged; permission restored; disconnect retry succeeds. |
| Client-resource publication denial | Client unchanged; `RestorePartial` recorded; permission restored; retry preserves unrelated settings and removes managed values. |
| Explicit lock controls | Four denials: journal and resource lock for each Pi/Codex reconnect. Client/journal bytes remain unchanged; releasing the locks permits reconnect and restore. |
| Cooperating apply/disconnect race | 16 races, eight per client. In this execution disconnect won all 16 and retained apply refused; all final journals were restored and token values absent. No claim that every scheduler interleaving or an apply-winning concurrent schedule was observed. Normal and post-unlock controls separately cover successful apply. |

The two PTY drivers exit **0** because their correctness assertions pass; the refused setup's product exit **1** is expected. Seven separately executed Rust tests passed, zero failed. Their source/observed loop branches contain **89 executed `assert!`/`assert_eq!` checks**, excluding `unwrap` expectations and assertions inside the product. This count is derived from source and the recorded race branches, not instrumentation of the gateway. Four lock denials and 16 race instances are subcases, not extra named tests.

Fresh probe compilation uses Rust 1.88.0, the held default llmgw rlib, and the held fs4 dependency. Outputs exist only in this rereview directory. The five original probe bodies are retained, with a stronger journal-byte assertion in the retired-plan checks. Fixture paths now include a unique timestamp suffix and every named test executes serially in its own allowlisted temporary environment. The implementer's earlier PID-only/default-parallel fixture collision remains preserved and is not counted as an independent product failure or hidden by this rerun.

## Held evidence audited, not rerun

`source-evidence-audit.json` independently checks the source/archive and current artifact identities. The held full locked test log has 16 successful result rows totaling **339 passed, 0 failed**. The held release client-profile log has **24 passed, 0 failed**. These are implementer runs, not fresh reviewer full-suite execution. All three held installed-client reports have `passed=true` and bind to the current release SHA below; none was rerun by this reviewer. The held Python7 result, fmt/check/Clippy/build records remain implementer evidence.

The new actual-client evidence now belongs to the same final release for Pi 0.84.2, Claude 2.1.63, and Codex 0.154.0. Earlier runs remain preserved separately. Mac execution does not establish Windows/Linux runtime compatibility; Codex catalog listing remains unverified. No historical performance/accounting matrix, new latency/RSS measurement, Windows SDK/aws-lc cross-build, or installation/OS registration was attempted.

## ACL concern retained unchanged

The earlier single protected-journal ACL-path `EINVAL` still has no confirmed precise syscall, phase, or original post-error state. The failed fixture cleanup removed that state. The fresh normal and permission-denial recovery passes above do **not** reproduce or explain that error. No retry or error masking was introduced, and no claim of an ACL fix is made. The bounded recovery evidence is compatible with leaving this platform observation disclosed while closing the two separately reproduced deterministic defects.

## Preservation and cleanup

- **88 current source files** match their manifest and archive; the archive has 89 entries including its embedded manifest.
- `before.json` / `after.json`: **1,221 protected files, zero drift**. This replaces only the legitimately changed live-source baseline while preserving all prior held sources/binaries, earlier failed logs, original reviewer holds/probes, latest fix artifacts, and parent audit. Parent-owned mutable tracking/reports are excluded.
- Source manifest SHA-256: `69fe932ae90f46abf65305be8fd0197d4c4a0e6bdc68cceb6ff63b38406ed20b`.
- Source archive SHA-256: `85d184adc9ad10e859a6988436a843bbd19ce41a250c743413d2466be5a12d3c`.
- Held release SHA-256: `cc15eb4172148a681faba2d4e2932e94a4853fc303de61d234b7a016c05b6a90`.
- Held default rlib SHA-256: `8651b48f5c771b351a33e266bf6ea9116eb45267879e6ca8137c4f5ae33bd632`.
- Implementer FINAL_HOLD SHA-256: `363fc08a5c3debf18d7ff7c6e68e6abe280e22bce9e4fd02896fd37f614a0fd7`.

No Cargo/default-target/held-target build or product source mutation occurred. The PTY used a synthetic version-only client executable and no installed-client inference. Tests used only owned temporary paths with allowlisted HOME/USERPROFILE/XDG/native/temp environment. No real client/auth configuration, credentials/keychain/login, external API, model operation, package installation, OS registration, or VCS mutation was touched. Raw token/client bytes and PTY prompt transcripts were not persisted. All owned workers were authenticated-stopped, test/PTY subprocesses reaped, and temporary fixtures removed.

`execution_finished=true`; `FINAL_HOLD`; product runtime/test ownership **released**. Proceed to the same SPEC reviewer's final confirmation.
