# Native Task5 independent QUALITY review

Verdict: **CHANGES REQUESTED — Q1, Q2, Q3 (P2)**. Review and execution are finished. Runtime/test ownership is released to the same implementer for these bounded corrections. No Task6 work was started by this reviewer.

## Actionable findings

### Q1 — Refuse a dangling temporary symlink before writing registration bytes

`product/src/autostart/mod.rs:299–303` uses a predictable `.<label>.<pid>.tmp`, tests `Path::exists()`, then writes with `fs::write`. `exists()` follows links and returns false for a dangling symlink. The write follows that link, creates its destination, and the later rename publishes the symlink as the registration file.

**Direct verification:** the exact held public rlib, on macOS, returned `Ok(())`, created a synthetic unrelated file outside the reviewed registration target, and left the final target as a symlink. This required only a pre-existing dangling scratch symlink; there was no concurrent race. Both endpoints were reviewer-owned. The safe-behavior assertion fails in `filesystem-probe.log`. A regular temporary-file collision and a target changed after preview are refused and preserved (positive controls pass).

This violates the exact-target/file-ownership contract and leaves a registration that the next snapshot rejects as non-regular. Do not infer a cross-user privilege escalation from this fixture. Replace the check-then-write with exclusive protected creation and write through the returned handle. The existing `lifecycle::platform::open(path, true, true)` already implements exclusive creation, Unix no-follow/private-file validation and Windows CREATE_NEW/reparse/ACL validation. `config_patch/storage.rs:264` and `setup/persist.rs:381` demonstrate its use. Reuse that established primitive and keep collision/recovery behavior bounded; no new service/storage framework is needed.

### Q2 — Attempt authenticated cleanup before deleting a started fixture's state

`product/scripts/verify_native.py:333–342` starts a manual worker, but the only authenticated `off` is on the straight-line success path. If the following `status`, identity access, autostart removal, or preservation check raises, the surrounding `TemporaryDirectory` exits and deletes the config/state without first attempting worker cleanup.

**Direct mocked control-flow verification:** inject a status exception immediately after a simulated successful manual `on`. The exact script invokes only `['autostart on', 'cli on']`, never `cli off`, and its real reviewer-owned temporary directory is deleted. The cleanup assertion fails in `verifier-probe.log`. All worker/manager/Cargo/plutil calls were mocked. **An actual leaked worker was not started or observed here.** The detached worker outliving the deleted control/config files is the consequence inferred from this control flow and the existing lifecycle contract.

Put authenticated cleanup in a bounded `finally` path once start has been attempted. Stop only the owned fixture using its exact config/identity, and retain the files needed for recovery if cleanup cannot confirm stopped. Preserve the original failure plus cleanup failure evidence. This is a failure-path defect in the authorized Mac fixture, separate from the unavailable real-login exercise.

### Q3 — Fail an attested login observation whose acceptance check is false

`product/scripts/verify_native.py:482–507` computes `verified` for the on/off login observation stages, records `actual_login_verified=false`, but then unconditionally returns exit code 0.

**Direct mocked control-flow verification:** with explicit execution, temporary-account confirmation, a timezone-bearing timestamp, and human-login attestation all supplied, (a) login-on with worker stopped/registration registered and (b) login-off with worker running/registration disabled each return 0 despite failed acceptance. Both regression assertions fail. Matching running/registered and stopped/disabled controls return 0 and pass. This finding concerns an explicitly executed **failed attested observation**, not an intentionally incomplete or preview-only stage. No actual manual service exercise or human login occurred.

Return nonzero when either observation's existing `verified` predicate is false while preserving its recorded evidence. The manual-cycle branch already uses this pattern at line 481.

## Scope reviewed and positive source conclusions

The Task4 held archive and current manifest identify 15 changed/new files (`delta-files.json`); `delta.patch` retains the modifications to pre-existing files. Reviewed the complete new autostart module, verifier, template/verifier tests, CLI setup/status/doctor dispatch, Win32 current-token SID code, lifecycle caller boundary, config-patch protected-write patterns, dependency delta, and runtime documentation. The research/root AGENTS, research README/HARNESS, work-item entrypoint, approved native design, existing OS-pattern research and requesting-code-review skill supplied the review contract. There is no `.git` in research/product; no Git operation was used. The absent product README and one exploratory wrong directory name are noted in `cleanup.json`.

**Source-supported, not freshly executed:** no autostart manager query was added to lifecycle readiness polling or HTTP/SSE/quota/fairness paths. OS commands use argv APIs; on/off remain register/unregister operations. macOS is file-only; Windows templates retain current-user trigger/principal and continuous-service settings; Linux observe-or-refuse and disable/unlink/reload/reobserve boundaries remain intact. The CLI separately confirms saved-core registration and reports unobserved environment login auth as unknown. Existing dependency changes are confined to quick-xml 0.42.0. No additional substantive finding emerged from this bounded review; no general XML-schema or service abstraction expansion is requested.

## Fresh checks and identities

- One `rustc` compilation linked the exact held default rlib and wrote only to this directory; exit 0. Rust public API probe: **3 tests, 2 pass, 1 expected regression failure; exit 101**.
- Exact verifier script loaded with Python bytecode disabled and mocked OS/runtime calls: **3 test methods, 3 failure records** (two failed login subcases plus cleanup), with two successful login subcases passing; exit 1.
- Overall independent cases: **8 = 4 positive controls + 4 failing cases**, supporting three findings. The failures are retained, not recategorized as passing tests.
- No Cargo command, unchanged full suite, current release execution, worker, network mock, model/API call, installed client, SDK cross-build, OS manager registration/query, or real-user config/auth inspection was performed.

Current source manifest (92 inputs): `b0c0ade22ce352c278c4019f8420ef24d16b6870839a716af37bd9e7aee2f801`.
Current source archive (93 members): `12919fe62ec6cc438ceb00b85bbd817f4e60aa19bd005576cc1b74efc5242282`.
Release (10,011,952 bytes; **not executed here**): `cae45cd6da245b00885b33a1e1e0df97647d9e518089724993d1c7e2e7f3107b`.
Linked default rlib (50,593,880 bytes): `cfda152fdcdb24bc4f18f25538c944f45b2cf086df89e804af99b75c49d4e330`.
Cargo.lock: `b216737fbfe2dc9946ffba69e34e26ce9902c6c87c90e974ab58ee850337e3e8`.
Compiler observed: Rust 1.88.0; macOS 26.5.1 arm64. Full paths, lengths and hashes are in `source-identities.json`; exact test commands/exits are in `commands.json`.

## Preservation and limits

Before/after: **1,577 available paths, zero drift**. Current archive: **93 members, zero content drift**, including embedded `source-manifest.json` mapped to the held final source manifest. The initial generic archive check treated that metadata member as a nonexistent live root file; its diagnostic is preserved in `archive-check-attempt1.json` rather than described as source drift. All product source/docs/artifacts/held targets and reference clones remain untouched. Every owned fixture directory was removed; no worker was started or left. Retained probe source/binary/log files are evidence, not active fixtures.

The current implementer focused11/fmt/check/clippy/release passes are held prior evidence. The earlier full Rust362/Python10/release13/Mac fixture results belong to `57d89c8e…b6b957`, not the current release. Task4 installed-client evidence belongs to its older `cc15…` binary. None was rerun or relabeled.

Actual Windows/Linux managers, current-user registration, temporary-OS-account/human-login acceptance and Windows console behavior remain unverified platform limits, not additional software findings. Historical gaps remain: two unavailable diagnostic logs and two overwritten provisional debug originals, recovery unknown. No restoration/reconstruction or blanket historical preservation claim is made. The prior Mac ACL EINVAL's exact syscall/phase/post-state remains unresolved; this review did not retry it or claim it fixed/transient.

`execution_finished=true`; `release_ownership=released_to_same_implementer_for_Q1_Q2_Q3`.
