# Native Task5 residual SPEC fix evidence

Status: **DONE_WITH_CONCERNS**. Execution is finished and product ownership is released only for the same targeted SPEC rereview. QUALITY and Task6 have not started.

## Changed product paths

- `src/autostart/mod.rs`
- `tests/autostart_templates.rs`
- `docs/runtime-contract.md`

`Cargo.lock` did not change. Its complete SHA-256 is `b216737fbfe2dc9946ffba69e34e26ce9902c6c87c90e974ab58ee850337e3e8`.

## Residual fixes

- R1: Windows manager XML now resolves valid decimal and hexadecimal character references through quick-xml, preserves predefined named entities, and rejects invalid or custom references even outside inspected fields.
- R2: One shared shape check requires exactly one Task root, RegistrationInfo, Triggers section with one LogonTrigger, Principals section with one Principal, Actions section with one Exec, and Settings section. Missing `Exec/Command` is unknown. Current-user ownership still requires both trigger and principal SID equality.
- R3: quick-xml parse failures use a fixed reason and do not expose foreign element names or raw task XML.
- R4: Linux observes `LoadState`, `UnitFileState`, `FragmentPath`, `DropInPaths`, and `NeedDaemonReload` in one `systemctl --user show --all` snapshot. Exact local fragments are accepted only with no drop-ins and no pending reload. Off orders disable, owned unlink, daemon-reload, and one re-observation; surviving global/foreign or unknown influence is reported without stopping the worker.
- Shared query correction: `0x8004130F` remains unknown/blocked because Microsoft defines it as missing Task Scheduler account information. Only `0x80070002` (file not found) is treated as absent for the exact `/TN` query.
- Documentation now distinguishes unobserved login auth (`unknown`) from an observed empty-login failure, limits non-macOS templates-only verification to rendering, and records the Windows percent-path refusal.

Primary references used for the narrow manager behavior: [Task Scheduler constants](https://learn.microsoft.com/en-us/windows/win32/taskschd/task-scheduler-error-and-success-constants), [schtasks query](https://learn.microsoft.com/en-us/windows-server/administration/windows-commands/schtasks-query), [Windows system error codes](https://learn.microsoft.com/en-us/windows/win32/debug/system-error-codes--0-499-), and [systemctl](https://www.freedesktop.org/software/systemd/man/latest/systemctl.html).

## Direct verification

- Focused private manager snapshot tests: 10 passed, 0 failed.
- Public `autostart_templates`: 13 passed, 0 failed.
- `cargo fmt --all -- --check`: passed.
- `cargo check --locked --all-targets`: passed.
- `cargo clippy --locked --all-targets -- -D warnings`: passed.
- `cargo test --locked`: 362 passed, 0 failed across 17 result rows.
- `cargo build --locked --release`: passed.
- Python verifier tests: 10 passed, 0 failed.
- Release templates-only fixture: passed; macOS owned temporary plist validated by `/usr/bin/plutil`; worker separation and missing-login-auth failure checks passed.

The release fixture records `actual_user_registration_changed=false` and `actual_login_verified=false`. It called neither launchctl nor a real Windows/Linux manager.

## Frozen identities

All paths below are relative to `/Users/ryuwon/Desktop/ryuwon-project/llm-gateway-research/product`.

- Release binary: `target/native-task5-residual-spec-fix/release/llmgw` — `10011952` bytes — `57d89c8e22bbc22f10bc16590dba5cf0b998fb463f18e9bad243e3fa37b6b957`
- Debug binary: `target/native-task5-residual-spec-fix/debug/llmgw` — `30629672` bytes — `b35a1b27b28b4893972f59b7e4f19665809a2332059e9e4672db7d77bfd5908c`
- Default rlib: `target/native-task5-residual-spec-fix/debug/deps/libllmgw-1750bcafbc9dfb14.rlib` — `50593184` bytes — `79f129f0b7cdb57d76bced7d8baa7bb13af4fb8a80ff681bd30b0e008f9eaa11`
- Source manifest: `artifacts/native-task5/residual-spec-fix/source-manifest-final.json` — `16268` bytes — `5689f03f07026fec4e32b0ba9e2ec1591371a19cc55d053543fa92e454983787`
- Source archive: `artifacts/native-task5/residual-spec-fix/source-hold-final.tar.gz` — `307898` bytes — `f481877252ac94abef42f49b2d71a86d09816bac0960257832042815cee6cc0d`
- Release fixture: `artifacts/native-task5/residual-spec-fix/templates-only-release-final.json` — `3719` bytes — `90742cdbcdc4cd831175e8bd37a335eea05fbc59be6fc20ed21aa1987c44a22d`

The source manifest uses `/Users/ryuwon/Desktop/ryuwon-project/llm-gateway-research` as its path base and contains 92 `product/...` rows. Current source drift is 0. The release binary hash equals both the manifest and fixture binary hashes.

## Preservation and limits

- Rechecked 1391 retained available files: 0 drift.
- Cleanup and privacy check: `passed=true`; no owned temporary fixture directory or residual fixture worker remains.
- Windows and Linux template behavior is source/fixture verified only. Real Task Scheduler, systemd user-manager, and multi-login acceptance remain unverified.
- The earlier two unavailable diagnostic logs and two overwritten provisional debug originals remain historical gaps with unknown recovery. Nothing was reconstructed and there is no blanket all-artifacts-unchanged claim.

Failed and corrected attempts remain immutable, including the original RED, compile failures, the stale foreign-fixture correction, and the first final clippy failure. Successful final evidence uses distinct filenames.
