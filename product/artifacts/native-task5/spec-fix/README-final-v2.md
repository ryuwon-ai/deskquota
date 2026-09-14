# Native Task 5 SPEC-fix implementation HOLD

Status: **DONE_WITH_CONCERNS**. S1–S7 are implemented in the frozen source represented by the v2 manifest. Product writer/runtime/test ownership is released for the same fresh SPEC rereview only.

## Implemented fixes

- **S1:** Windows plans obtain the current process token SID through the existing native lifecycle module. The same SID is emitted in `LogonTrigger/UserId` and `Principal/UserId`, included in the preview hash, and required in both manager fields before an existing task is treated as owned/current-user scoped.
- **S2:** Unobserved environment-backed login authentication reports `unknown`. The empty-login fixture separately records its observed missing environment reference as `configured_but_unavailable`; terminal environment availability is not treated as login proof.
- **S3:** `--templates-only` returns after pure Rust render contracts on Linux and Windows and cannot dispatch `systemctl` or `schtasks`. On this macOS host it used only an owned temporary `HOME` LaunchAgent file and read-only `/usr/bin/plutil`; it never called `launchctl`.
- **S4:** Verifier Cargo work requires an explicit caller target, rejects the held `target/native-task5`, and reports the exact target and binary. Output creation is exclusive. Final verification used the fresh `target/native-task5-spec-fix-v2`.
- **S5:** Manual stages inherit the explicitly executing terminal environment without writing it to evidence. Empty-login fixtures retain their allowlisted environment. A failed manual on/off cycle returns nonzero, and login observations omit authenticated identity/nonce data.
- **S6:** One-shot planning captures both the local sidecar and authoritative manager state. Apply rechecks both snapshots before mutation. Missing managers and query failures remain unknown/blocked; foreign/global registrations remain blocked; owned enabled, disabled, surviving, missing-binary, and moved-binary states are distinct in status/doctor. Setup skips unregister only when both snapshots are absent. Raw task XML and manager stderr/stdout are not serialized into previews or diagnostic reasons.
- **S7:** Windows executable or config paths containing `%` are rejected before planning or mutation until native Task Scheduler argument delivery is proved on Windows. No shell, VBS, Startup-folder, or alternate-supervisor fallback was added.

Windows scheduled-task XML is parsed with pinned `quick-xml 0.42.0` and no default features. This is the only new runtime dependency; it replaces unsafe string matching with namespace-aware semantic parsing for ownership, principal, trigger, Settings/Enabled, and executable fields. UTF-8 and BOM-marked UTF-16 task output are decoded before parsing. The parser rejects malformed, ambiguous, unexpected-namespace, DTD, CDATA, and custom-entity definitions.

## Fresh verification

All commands below used the frozen v2 source. Cargo used `/Users/ryuwon/Desktop/ryuwon-project/llm-gateway-research/product/target/native-task5-spec-fix-v2`.

- `cargo fmt --all -- --check`: pass
- `cargo check --locked --all-targets`: pass
- `cargo clippy --locked --all-targets -- -D warnings`: pass
- `cargo test --locked`: **357 passed, 0 failed** across 17 test binaries
- `cargo build --locked --release`: pass
- `cargo test --locked --release --test autostart_templates`: **13 passed, 0 failed**
- `python3 -m unittest tests/test_verify_native.py`: **10 passed, 0 failed**
- Release `verify_native.py --templates-only`: pass; source probe binary hash matches the release

The macOS release fixture observed stopped → autostart on with zero worker processes, running → autostart off with the same authenticated worker identity, authenticated cleanup to stopped, unobserved login auth as unknown, and an empty-login run failing specifically for its missing synthetic environment reference. Registration contained neither the synthetic value nor its environment-reference name. The owned temporary fixture root was removed.

The retained RED/failure evidence includes missing S1/S2 APIs, unsafe verifier target/dispatch/manual-environment contracts, missing login-summary behavior, initial XML compile/parser failures, formatting output, and the first full-suite failure caused by a stale pre-S2 assertion. That assertion was corrected to require unknown login availability while preserving the separately observed terminal environment failure.

## Frozen identities

Path bases are explicit: source-manifest rows beginning `product/` resolve from `/Users/ryuwon/Desktop/ryuwon-project/llm-gateway-research`; artifact, target, and Cargo.lock rows below resolve from `/Users/ryuwon/Desktop/ryuwon-project/llm-gateway-research/product`.

- Release binary: `target/native-task5-spec-fix-v2/release/llmgw` — `7efe265894a0cc750990ae690c69a47b7b8e00ccaef92f7686f7e7b8b81e4339` (10,081,264 bytes)
- Debug binary: `target/native-task5-spec-fix-v2/debug/llmgw` — `65f70d0b5096f15f8c7dc78814d57a9de2074dd303104bf97a58a47459416473` (30,624,696 bytes)
- Current default debug rlib: `target/native-task5-spec-fix-v2/debug/deps/libllmgw-1750bcafbc9dfb14.rlib` — `f7c3a06be75bb21dd81531dbf0d520e2a664f1afc46978335353b852f49f4a24` (50,583,920 bytes)
- Cargo.lock: `Cargo.lock` — `b216737fbfe2dc9946ffba69e34e26ce9902c6c87c90e974ab58ee850337e3e8` (52,949 bytes) (64-character SHA-256)
- Source manifest: `artifacts/native-task5/spec-fix/source-manifest-final-v2.json` — `aa6e92a68f2b7b71805cd8902f17c9990a632bab61c5e3423bfbb3332e66862f` (16,250 bytes) — 92 source files, current mismatch count 0
- Source archive: `artifacts/native-task5/spec-fix/source-hold-final-v2.tar.gz` — `e12433e4101cf95c62bb221e2386ef664b2c9b09695050c0a392d3089cea2273` (304,575 bytes) — 93 file members (92 sources plus embedded manifest)
- Templates-only probe: `artifacts/native-task5/spec-fix/templates-only-release-final-v2.json` — `cff4878d9ceaf27a74e0f578e5a31af38bb35ac505e345836b34cbf6caddfaf6` (3,707 bytes)
- Toolchain: rustc 1.88.0 (`6b00bc388`), cargo 1.88.0 (`873a06493`), aarch64-apple-darwin, macOS 26.5.1 (25F80)

Relative to the held pre-fix Task5 source archive, 10 existing source files changed, none were added or removed. HTTP/SSE/quota/accounting/fairness/default behavior was not changed.

## Preservation disclosure

The held baseline in `preservation-final-v2.json` covers 1,282 prior available files after excluding 92 mutable live source paths: **0 drift**. This includes the fresh SPEC hold and accepted prior releases.

A separate preservation breach occurred after the provisional SPEC-fix identities were captured: `target/native-task5-spec-fix/debug/llmgw` and its identified default rlib were rebuilt after the final setup edge fix. `preservation-breach-debug-target.json` records their exact before/after paths, sizes, hashes, and associated source manifests. Recovery of the original two debug artifacts is unknown; they were not restored or reconstructed. The provisional release binary remains byte-identical at `daa4916b4daed7d464686f3b8bd249027c4cbdf92f634815ed8fe1b85cd4d1bb`, and its templates-only result, source manifest, source archive, logs, and identity record remain available. This report does not claim every provisional artifact stayed unchanged.

The two older diagnostic logs disclosed before this fix remain unavailable. No Trash scan, recovery, reconstruction, sanitization, or deletion was attempted. Every captured SPEC-fix failure log is retained.

## Unverified limits

Actual current-user registration, a temporary real OS account, human next-login/on/off cycles, Windows Task Scheduler argument execution and console behavior, and Linux user-manager execution remain **UNVERIFIED**. Windows compilation was not repeated because the unchanged host lacks the previously identified SDK/aws-lc C headers. The templates-only fixture is source/file/runtime-separation evidence, not actual login evidence.

The earlier one-off macOS journal ACL-path `EINVAL` remains unresolved at its exact syscall, phase, and post-error state and was not retried or masked. Core performance superiority remains unproven. No real client/auth/config was read or written; no paid API, model, package install, PATH/profile change, actual login registration, or VCS operation occurred.

`execution_finished=true`; `release_ownership=released_for_same_SPEC_rereview_only`.
