# Native Task 6 final-v2 evidence

Status: **DONE_WITH_DISCLOSED_OS_LIMITATIONS**. The frozen source, fresh release build, packaged binary, installer smoke, three installed-client fixtures, and bounded native observations all passed on the macOS arm64 development host.

## Immutable identities

- Cargo.lock SHA-256: `b216737fbfe2dc9946ffba69e34e26ce9902c6c87c90e974ab58ee850337e3e8`
- Source manifest SHA-256: `1981823429bc99e1cb776b9a007a8ea7828ae9cd2337b7d97569f442a5d95e33` (101 files, including both installer sources)
- Source archive SHA-256: `0b9e0a9ab118327898f3fc01d650d1248686c8b1efe1f0a5be8557b62859a3c9`
- Release/package binary SHA-256: `cf7c436c442c426a6e9a1d485a5261fe5e5a05c8b131b130e679e00eec2c646b`
- User package SHA-256: `902aa7e58f7292c82ba1777065398060c393d331652709a544919f6410e6e902`
- Package checksum-manifest SHA-256: `a28df418af2bf719a95cfd99cd5ef756f7b29630fb8ca7e3afda93fe9ff1ca5e`

The source archive embeds the exact `source-manifest.json`. All 101 archived source members match both the manifest and frozen live bytes. The user package contains only `llmgw` and four required user documents.

## Direct verification

- `cargo fmt --check`, locked all-target check, locked all-target clippy with warnings denied, locked full Rust tests, and locked release build passed in the fresh `target/native-task6-final-v2` target. Rust: 365 passed.
- The bounded Python fixture suite passed 64 tests, including wrong hash, missing artifact, invalid archive entries, destination failure preserving an existing executable, reinstall, hostile PATH text, local loopback HTTP release fixture, and static Windows recovery boundaries.
- Fresh local install and reinstall produced executable SHA `cf7c436c442c426a6e9a1d485a5261fe5e5a05c8b131b130e679e00eec2c646b`. Absolute execution returned `llmgw 0.1.0`. The restricted PATH control did not resolve `llmgw`; a new child shell executing the installer's exact printed line resolved the installed path and returned the same version. No profile was changed.
- The same packaged binary bytes passed Pi 0.84.2 (OpenAI Chat Completions SSE), Claude 2.1.63 (Anthropic Messages SSE), and Codex 0.154.0 (OpenAI Responses with WebSocket disabled). Pi listing/selection/tools passed; Claude selection/tools passed and listing was not requested; Codex selection/tools passed while catalog listing remains unverified.
- All client fixtures confirmed gateway-off behavior with zero upstream attempts and reported temporary-directory cleanup. The measurement driver authenticated `off`, confirmed stopped state, and removed its owned home.

## Native observations

- Direct setup PID sampling: 1 raw sample at 3,588,096 bytes. This is one observation, not a peak claim; activity between samples may be missed.
- Separate timed setup command-tree observation: 8,863,744 bytes from macOS wait4, with the raw line retained.
- Idle worker: 10 owned-PID samples, each 10,010,624 bytes.
- Five raw `on` times (ns): [52597584, 39765042, 36031375, 39029250, 37298709].
- Five raw `off` times (ns): [43829917, 35699250, 35822709, 35600542, 34258666].

These are descriptive observations from an Apple M4 32 GiB development host. They are not low-end, percentile, or superiority evidence. The two setup memory values come from separate wizard invocations and different scopes.

## Code and policy boundary

The only runtime Rust change from accepted Task 5 is the first-run non-TTY diagnostic string in `src/cli.rs`. Source review therefore found no setup hotpath file watch, discovery loop, secret helper, or telemetry addition. Installers do not modify profiles or the Windows user PATH registry. No public URL or public one-liner was added.

## Unverified

PowerShell was unavailable, so `install.ps1` has source/static fixture evidence only. Windows x64/arm64, Linux x64/arm64, macOS x64, signing/quarantine, Windows PATH registry behavior, clean native accounts, low-end hardware, and real human-login registration remain unverified. No real user client/config/auth state, profile, registry, autostart registration, credential, model, paid API, cloud resource, or public release was used.

## Evidence map

- `final-identity.json`: toolchain, lock/source/archive/binary identities and test totals
- `source-manifest.json`, `source.tar.gz`, `archive-verification.json`: frozen source proof
- `package-identity.json`, `llmgw-macos-arm64.tar.gz.sha256`: package identity
- `install-smoke.json`: install/reinstall and PATH child-shell proof
- `client-pi.json`, `client-claude.json`, `client-codex.json`: exact installed-client observations
- `native-measurement.json`: raw wizard, idle RSS, and lifecycle observations
- `runtime-change-audit.json`, `platform-support.json`: source boundary and support limits
- `preservation-before.json` and `preservation-final.json`: prior evidence preservation
- `provisional-attempts.json`: preserved pre-freeze identities and honest failed-attempt accounting
- raw quality logs: `fmt.log`, `check.log`, `clippy.log`, `rust-test.log`, `release-build.log`, `python-tests.log`, `static-installer-check.log`
