# Native Task5 Q1–Q3 targeted QUALITY rereview

Verdict: **PASS for the bounded QUALITY scope, with disclosed real-OS limits.** Q1, Q2 and Q3 are closed, including the adjacent predictable backup-path collision. No actionable finding remains from this rereview. Execution ownership is released for the same SPEC reviewer's final behavior confirmation before Task6.

## Fresh independent verification

**8 tests, 14 cases, all passed.** Exact commands and exit codes are in `commands.json`; complete outputs remain in `filesystem-probe.log` and `verifier-probe.log`.

- **Rust public API: 5 tests / 6 cases, exit 0.** The exact held default rlib now refuses the original dangling temporary symlink without creating its destination and preserves the link. Regular temporary collision and changed-preview controls preserve existing bytes. Both dangling-symlink and regular-file backup collisions preserve the old registration and collision entry, with no candidate residue. A collision-free registration update produces a regular file with the new executable and can be removed normally.
- **Exact Python verifier, mocked control flow: 3 test methods / 8 cases, exit 0.** The original post-start status failure now calls authenticated `off` with the same exact config, confirms `stopped`, then deletes the fixture. A raised start also triggers cleanup because attempted start is tracked first. Failed `off` and successful `off` followed by still-running status retain the recovery config/home and report the original plus cleanup failure. Failed attested login-on/off observations return 1; both successful controls retain exit 0.

The Python tests mocked every Cargo, manager, plutil and worker call. Their filesystem recovery paths were real, synthetic and owned by this reviewer; after retention assertions, the reviewer removed those mock fixtures. **No actual worker leak or login behavior was executed here.** The Rust public registration probe exercised only owned macOS filesystem resources and never called launchctl.

## Source disposition

Only four source paths differ from the originally reviewed document-boundary version: `src/autostart/mod.rs`, `scripts/verify_native.py`, `tests/autostart_templates.rs`, and `tests/test_verify_native.py`. The exact diff is retained in `delta.patch` and current source identities in `source-identities.json`.

Q1 reuses the existing protected `lifecycle::platform::open(..., true, true)` primitive, writes/syncs through the handle, and refuses existing backup entries with `symlink_metadata`. No new file/service abstraction was introduced. Q2 explicitly owns the temporary directory and conditions deletion on confirmed cleanup once start was attempted, preserving the original failure and recovery location otherwise. Q3 applies the existing acceptance predicate to each observed stage's exit status; preview/incomplete-stage branches were not changed. The adjacent tests cover those narrow changes. The original full-delta QUALITY review remains in the parent directory; unchanged schema, lifecycle, HTTP, quota, fairness, client and architecture paths were not re-reviewed or re-executed merely for counts.

## Identities and preservation

All file paths below are research-relative. This review used Rust 1.88.0 on the macOS host.

- Source manifest, 92 inputs: `product/artifacts/native-task5/quality-fix-v2/source-manifest-final.json` — `8b281a41f48313a2d664d9f94737cbcba3794b10688904ec1e969d675296e304`.
- Archive, 93 members: `product/artifacts/native-task5/quality-fix-v2/source-final.tar.gz` — `12f6c81a44d95879383ffb0883b2b7681dcc203c18c20dcca6cf2621cfe5c875`.
- Held release, 10,011,264 bytes, **not executed by this reviewer**: `product/target/native-task5-quality-fix-v2/release/llmgw` — `88c21502025b0bb0b77db2833845d64bb2c562e345e0c48bc5042651a4ee25a3`.
- Linked default rlib, 50,712,616 bytes: `product/target/native-task5-quality-fix-v2/debug/deps/libllmgw-1750bcafbc9dfb14.rlib` — `0c0bfaf27091f5bc10896015f6d99eaacce21efb11f98ed7c5a51950f409fea7`.
- Cargo.lock unchanged: `b216737fbfe2dc9946ffba69e34e26ce9902c6c87c90e974ab58ee850337e3e8`.

`before.json` / `after.json`: **1,641 available paths, zero drift**. This combines the retained non-live baseline 1,521, current source 92, parent v2 preservation 27 and parent audit. Current source archive has **93 members, zero content drift**, including its embedded source manifest. The original review's failures, implementer RED failures, provisional first quality-fix capture, and initially incorrect cleanup metadata remain preserved. No unrelated Python cache was removed.

`cleanup.json`: zero owned active fixture paths and zero workers. All new files are inside this rereview directory. No product edit, held-target rebuild, Cargo command, current release execution, actual-user config/auth read, real OS registration/query, actual manual service exercise, installed client, package/PATH mutation, model/API call, reference-clone change or VCS operation occurred.

## Evidence limits retained

Implementer fmt/check/clippy/release, Rust templates15, Python13 and the current release's owned Mac plutil/runtime-separation check are held prior evidence, not fresh executions by this reviewer. The earlier full362 run belongs to `57d89…b6b957`; Task4 installed-client checks belong to its older `cc15…` binary. Neither was rerun or relabeled as evidence for this release.

Actual Windows/Linux managers and temporary-OS-account/human-login acceptance remain unverified platform limits, not new software findings. The two missing historical diagnostic logs and two overwritten provisional debug originals remain unavailable with recovery unknown; no reconstruction or blanket historical preservation claim is made. The earlier Mac ACL EINVAL still has unknown exact syscall/phase/post-error state and is not called fixed or transient.

`execution_finished=true`; `release_ownership=released_for_same_SPEC_final_behavior_confirmation`.
