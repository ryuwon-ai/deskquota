# Final packaged native acceptance

Status: **PASS_WITH_PLATFORM_LIMITATIONS**. The already frozen five-member package was installed into an owned isolated home, reinstalled, resolved through the installer's exact PATH preview in a new child shell, exercised through three installed clients, and measured with the installed package bytes.

## Identities

- Frozen source manifest SHA-256: `3f2c38648167168e2d9d22324cdae7850f1a839a791e42d65c4168e45269f8aa` (101 files)
- Frozen source archive SHA-256: `de54b78c74ce6e29e0b2a3960f07a870086cfe9a319a47b725e04198cb58d967` (101 source members; standalone manifest)
- Package identity SHA-256: `2a2622b32123f6a2d0b43a4963f2a699b77459dd3b76b1d0b88017351cd41613`
- Package SHA-256: `62327038bf36303a7a242bd7ddc822a62036003f647108dab6df5f69e69d3acb` (4,209,368 bytes; five members)
- Package checksum-manifest SHA-256: `a70ab6d1a47d945a516a43cda1cb0a6d223094e6479a8eb2e017e1ef0de79a8a`
- Packaged and installed binary SHA-256: `16de493ce1ec7f034d0fed31db111bd4ddcf2750c813e37fb12a8b2b74f10fa7`

The package, checksum manifest, and package identity created before runtime ownership were read-only throughout this acceptance run. Every packaged document equals the current frozen source, and the installed executable equals the archive member.

## Direct packaged acceptance

- Fresh install and reinstall passed with the exact packaged binary. A fixed restricted PATH did not resolve `llmgw`; a new child shell executing the installer's exact printed PATH line resolved the installed executable and returned `llmgw 0.1.0`. No profile was edited.
- Pi 0.84.2 used OpenAI Chat Completions SSE; model listing, selection, exact tool result and follow-up, gateway-off negative behavior, disconnect, and unrelated-edit preservation passed.
- Claude 2.1.63 used Anthropic Messages SSE; selection, exact tool result and follow-up, gateway-off negative behavior, disconnect, and unrelated-edit preservation passed. Listing was not requested.
- Codex 0.154.0 used OpenAI Responses with WebSocket disabled; selection, exact tool result and follow-up, gateway-off negative behavior, disconnect, and unrelated-edit preservation passed. Its model catalog listing remains unverified because the gateway list is not a Codex catalog.
- Each client's gateway-off fixture observed zero upstream attempts. All client drivers reported control-stop cleanup and reaped temporary directories.

## Fresh descriptive measurements

- Direct setup PID: 2 samples, raw `[{'elapsed_nanoseconds': 0, 'pid': 56179, 'rss_bytes': 4718592}, {'elapsed_nanoseconds': 23766417, 'pid': 56179, 'rss_bytes': 9502720}]`, sampled maximum 9,502,720 bytes. This is a sampled maximum; peaks between observations may be missed.
- Separate timed setup command tree: macOS wait4 peak 9,666,560 bytes, raw line `9666560  maximum resident set size`. This is a different wizard invocation and scope from direct PID sampling.
- Idle worker: ten owned-PID samples, all 10,207,232 bytes.
- Five raw `on` durations in nanoseconds: `[36399167, 44242625, 36665250, 63140292, 37426083]`.
- Five raw `off` durations in nanoseconds: `[45257125, 36686375, 36438916, 37520000, 35646166]`.
- Final authenticated state: `stopped`. The measurement home was removed after authenticated `off` and stopped-state confirmation.

`measure_native` did not instrument native upstream attempts; its loopback upstream and manual no-listing/no-inference setup remain separate from the client gateway-off fixtures above. These values are descriptive observations on an Apple M4 32 GiB development host, not percentiles, low-end evidence, or performance-superiority claims.

## Verification accounting and limits

No Cargo/build, broad Python, quota, full setup, or user-service matrix was rerun. The retained Rust 370 result belongs to the frozen I1 source. The older Python 64 result predates the packaging Q1/Q2 changes; the later focused five-test result covers those packaging changes. Neither is counted as fresh packaged acceptance. The independent I1 PTY SaveOnly result was not duplicated.

Windows/PowerShell, Linux, macOS x64, clean native accounts, low-end hardware, signing/quarantine, and real human-login registration remain unverified. No real user client/config/auth/PATH/profile/registry/autostart/keychain state, model, paid API, cloud action, external package-manager installation, public release, or VCS action was used. The owned installation fixture was removed only after all driver cleanup and stopped-state checks passed.

`execution_finished=true`; `runtime_ownership_released=true`; `owned_workers_remaining=0`.
