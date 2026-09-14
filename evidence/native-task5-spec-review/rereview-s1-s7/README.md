# Native Task 5 SPEC rereview — S1–S7

Verdict: **CHANGES_REQUESTED**. The original public/CLI defects were substantially corrected, but four residual P2 findings remain in authoritative manager interpretation. The missing real OS account/login environment is **not** a software finding. No QUALITY review or Task6 was started.

This reviewer solely owned bounded product runtime/tests. Product source, held targets, captured earlier outputs, and reference clones remained read-only. Every new output is in this rereview directory. There was no Cargo invocation, held rebuild, real Task Scheduler/systemd/launchctl call, current-user registration, account/login creation, real client/config/auth access, paid API/model call, package install, PATH/profile change, or VCS operation.

## Residual findings

### R1 — P2 / S6: Handle numeric XML character references as valid semantic content

`product/src/autostart/mod.rs:812-819` passes every `GeneralRef` through `quick_xml::escape::resolve_xml_entity`, which accepts only the five predefined named entities. Valid decimal/hexadecimal character references such as `&#38;` and `&#45;` are classified as custom entities, causing an owned task to become unknown/blocked. This affects semantic equality of executable paths, SID fields, and ownership markers serialized with numeric references.

Fresh independent copied-source test `numeric_character_references_are_semantically_equal` replaced `&amp;` with `&#38;` and SID hyphens with `&#45;`; parsing failed with the generic custom-entity error. The unmodified named-entity case passed. The pinned, already installed quick-xml 0.42.0 provides `BytesRef::resolve_char_ref()` in `events/mod.rs:1724` specifically for numeric references, while `escape.rs:881` documents the limited named-entity resolver. Use those existing APIs to distinguish numeric references, permitted named entities, and truly unsupported custom entities, retaining fail-closed malformed-reference handling. No additional parser dependency is needed.

Evidence: `private-probe.log`, `autostart_source_copy.rs`, `source-copy-identity.json`. This executes the exact parser source in a reviewer context, not Task Scheduler on Windows.

### R2 — P2 / S1–S6: Validate every active logon trigger before calling a task current-user scoped

`product/src/autostart/mod.rs:835-844` concludes current-user scope from one collected `trigger_user` and one principal SID. Additional `LogonTrigger` elements without a `UserId` do not create that field, so they are ignored by this check. An owned task with the expected scoped trigger **plus an enabled any-user trigger** is still classified owned/current-user. This hides precisely the cross-user trigger behavior S1 was intended to expose and permits mutation of a task whose actual trigger scope differs from the reviewed contract.

Fresh independent test `extra_unscoped_logon_trigger_is_not_current_user_scoped` appended `<LogonTrigger><Enabled>true</Enabled></LogonTrigger>` to the generated task. The parser returned both owned and current-user scoped; the contract assertion failed. Microsoft documents that an omitted [LogonTrigger.UserId](https://learn.microsoft.com/en-us/windows/win32/taskschd/logontrigger-userid) applies to any user's login. Validate trigger structure/cardinality and each applicable logon trigger, including empty elements, rather than inferring whole-task scope from the presence of one matching field. A small explicit refusal for unsupported multi-trigger definitions is acceptable; a general task framework is unnecessary.

The generated new task itself correctly contains the token SID in both UserId locations. The remaining defect is interpretation of an existing manager definition. Actual Windows execution remains unverified.

### R3 — P2 / S6: Normalize XML parser errors without including foreign XML text

`product/src/autostart/mod.rs:775-777` embeds quick-xml's raw Display error in the user-facing reason. Mismatched-tag errors include element names taken directly from the manager XML. `observe_windows_manager` then stores that text in `ManagerSnapshot::Unknown`, which is displayed in preview/status/doctor. Sanitizing subprocess stderr/stdout elsewhere does not cover this parser path.

Fresh independent test `malformed_foreign_xml_does_not_echo_foreign_content` used a synthetic foreign closing-tag name and confirmed it reappeared in the returned error; the no-foreign-content assertion failed. Return a fixed parse-error category, optionally a safe position/error code, instead of interpolating parser diagnostics containing external names. Retain the captured synthetic failure log unchanged. No actual user credential or actual foreign task content was read or disclosed; this is a reproducible diagnostics-boundary defect, not an observed real credential leak.

### R4 — P2 / S6: Snapshot the effective Linux manager definition and finish removal with a consistent manager view

`product/src/autostart/mod.rs:570-578` queries only `LoadState`, `UnitFileState`, and `FragmentPath`. At `:645-651` it hashes that limited output but gets the registered executable from the current disk fragment. A loaded drop-in can replace ExecStart without changing any selected property, and a pending daemon reload can make the disk fragment differ from the command loaded in the manager. The snapshot can therefore stay identical while the effective managed executable/scope changes; missing/moved-binary reporting and preview rechecks are not authoritative.

A fresh bounded copied-source projection test changed an owned fixture's drop-in while supplying the same three properties that the actual query requests. Both snapshots remained identical, owned, and pointed to the disk executable; the expected change-detection assertion failed. **No drop-in was installed into a real manager:** this is direct helper behavior under a mocked property projection plus source/document reasoning about the actual manager. The systemd [systemctl documentation](https://raw.githubusercontent.com/systemd/systemd/main/man/systemctl.xml) explicitly distinguishes disk files from manager-loaded configuration until reload.

An adjacent ordering issue is source-supported: `remove()` calls disable at `:331-332`, then unlinks the fragment at `:334-335`, with no later daemon-reload. systemctl disable performs its implicit reload before returning, so that reload precedes the file removal. A still-loaded fixture fragment after unlink is currently classified foreign by the helper; the companion projection test documents that behavior. Successful user disable can also leave global enablement, which the official documentation explicitly warns about; the current successful-command path suppresses all manager output.

Observe or explicitly refuse relevant effective command/drop-in/pending-reload/global conditions using small one-shot manager properties. Keep foreign/org/global resources untouched. After changing the local unit file, perform the appropriate user-manager reload/re-observation in an order that preserves the current worker, and surface remaining global influence instead of implying an authoritative completed removal. This does not require a watcher, polling hot path, new supervisor, or generic service framework. Actual Linux execution and precise post-unlink live behavior remain unverified.

## Original findings disposition

| Original | Current bounded result |
|---|---|
| S1 current-user trigger | New rendering fixed: native token SID in trigger/principal, SID changes preview hash. Existing-task scope still needs R2. Windows native token path source-reviewed, not executed. |
| S2 unknown login auth | Runtime fixed: `Auth::Env` is unknown absent login evidence; separately observed empty-env failure remains unavailable. Documentation correction below remains. |
| S3 templates-only manager mutation | Fixed: mock probes for Linux/Windows cannot reach status/CLI/apply; return is render-contract-only and accurately marked. Mac uses an owned LaunchAgents file fixture. Real non-Mac validators/manager execution remain unverified. |
| S4 forced held build target/output reuse | Original forced target defect fixed: explicit external target honored; original held target refused; output open is exclusive. Caller must still choose a new unique target and output. Stock driver was not run here. |
| S5 terminal auth vs empty login fixture | Fixed: explicit terminal environment path is preserved; empty fixture remains isolated; failed manual cycle exits nonzero. Login summary omits identity, and human attestation remains required. |
| S6 authoritative manager state | Missing local sidecar, disabled/unknown/foreign/global base cases and both-absent setup shortcut improved. Residual parser/effective-manager defects R1–R4 prevent acceptance. |
| S7 literal percent path | Accepted bounded correction: Windows percent in either executable or config is blocked before planning/mutation; no wrapper. Literal native Windows delivery remains unverified. |

Required documentation consistency correction: `product/docs/runtime-contract.md:182-185` still states environment-auth configs are reported `configured_but_unavailable` merely because shell auth is not copied. Update it to unknown until observed, and describe render-only non-Mac verifier scope and the explicit Windows percent-path block. The source delta did not update this product contract file. This is a documentation follow-up to the fixed behavior, not an extra runtime finding.

## Fresh verification

| Execution class | Result | Files |
|---|---|---|
| Independent public API tests linked to exact held default debug rlib | **5 passed / 0 failed**, exit 0 | `public_probe.rs`, `public-probe.log` |
| Exact autostart source copied into a reviewer context; independent private parser/state tests | **4 passed / 3 failed**, exit 101 | `autostart_source_copy.rs`, `private-probe.log` |
| Exact source under bounded Linux property projection | **1 observational check passed / 1 contract check failed**, exit 101 | `autostart_linux_source_copy.rs`, `linux-probe.log` |
| Mock-only verifier dispatch/environment/output checks | **10 passed**, exit 0 | `driver_probe.py`, `driver-probe.json` |
| Exact held-release owned macOS fixture | **15 acceptance assertions passed**, exit 0 | `runtime_probe.py`, `runtime-probe.json` |

The private contexts reexport `llmgw::config` from the held rlib. Their native user-identity adapter deliberately panics if called and was never invoked. The copied product autostart source is an exact byte prefix; only independent tests are appended. The compiled Mac platform guards prevent Windows/Linux manager execution. These tests do not claim the Windows native token API, task registration, or Linux manager was executed. Three rustc builds wrote outputs only here; no Cargo build or original source mutation occurred. Exact commands/exits are in `compile-commands.json`, `probe-commands.json`, `linux-commands.json`, and `runtime-probe.json`.

Fresh Mac checks include worker count zero after registration, literal plist argv and plutil validation, wrong hash refusal, authenticated same-nonce worker survival after unregister, manual authenticated stop, registration excluding synthetic auth/name, empty-environment missing-auth run failure, and unknown unobserved login auth while terminal auth is present. Both configs were authenticated-off and observed stopped before fixture removal. The runtime result uses release `7efe…4339`, not the older Task4 client-test binary.

The implementer's **357 Rust**, **13 release template**, **10 Python** passes and templates-only result are retained held evidence, not re-executed here. No unchanged full-suite, installed-client, quota/performance, Windows SDK/aws-lc, or real login matrix repetition occurred.

## Source and preservation

Current source manifest: 92 files, SHA `aa6e92a68f2b7b71805cd8902f17c9990a632bab61c5e3423bfbb3332e66862f`. Current archive: SHA `e12433e4101cf95c62bb221e2386ef664b2c9b09695050c0a392d3089cea2273`, 93 members. Relative to initial Task5, 10 existing source files changed, none added/removed. The only new dependency is pinned `quick-xml = 0.42.0`, default features disabled. HTTP/SSE/quota/accounting/fairness and the internal readiness path remain unchanged; manager queries stay in one-shot CLI planning/status/doctor.

Current release: `product/target/native-task5-spec-fix-v2/release/llmgw`, 10,081,264 B, SHA `7efe265894a0cc750990ae690c69a47b7b8e00ccaef92f7686f7e7b8b81e4339`. Debug SHA `65f70d0b5096f15f8c7dc78814d57a9de2074dd303104bf97a58a47459416473`; linked default rlib SHA `f7c3a06be75bb21dd81531dbf0d520e2a664f1afc46978335353b852f49f4a24`. Cargo.lock SHA `b216737fbfe2dc9946ffba69e34e26ce9902c6c87c90e974ab58ee850337e3e8`.

`before.json` and `after.json`: **1,445 available files, zero drift**, including baseline 1,282, current 92 source files, current/provisional phase artifacts and prior SPEC files. Parent `preserve_spec_fix_available` 68-file set is fully covered. The later parent audit itself was separately captured and rechecked unchanged (`parent-audit-before.json`, `cleanup.json`), yielding **1,446 total available preserved paths**. Archive 93 members, zero content drift. Every new failure log, source copy, and command record remains available; nothing was moved or sanitized.

Preservation gaps remain separate:

- Two older diagnostic logs are unavailable; no Trash browsing, reconstruction, or recovery claim.
- Two provisional debug artifacts were overwritten before this rereview. Their **original** bytes/hashes (`6c3e…` / `15b0…`) are not restored and recovery is unknown. Their current overwritten-after files (`d4d3…` / `25db…`) are preserved and match `preservation-breach-debug-target.json`; they are not falsely substituted for original identities.
- Provisional release `daa491…d1bb`, its source archive/manifest, probes/logs, identity record, and breach record remain preserved. A clean rereview hash comparison does not erase the earlier breach.

`cleanup.json` records authenticated runtime cleanup, zero owned workers, and zero active fixture paths. Compiled probes and passive evidence remain. The earlier unrelated one-off Mac ACL-path EINVAL is still unresolved at its exact syscall/phase/post-state and was neither retried nor claimed fixed.

Actual current-user registration, temporary real OS account, human next-login/on/off cycles, native Windows/Linux execution and Windows console behavior remain **UNVERIFIED**. No support-complete or CI/CD-safe claim is made.

`execution_finished=true`; `release_ownership=released_for_same_implementer_residual_SPEC_fixes_then_same_SPEC_rereview`. Do not start QUALITY or Task6 from this verdict.
