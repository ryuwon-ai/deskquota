# Native Task5 final SPEC behavior confirmation

Verdict: **PASS_WITH_DISCLOSED_REAL_OS_LIMITATIONS**. No remaining actionable SPEC regression was found in the four-file QUALITY delta. This closes final SPEC behavior confirmation after the independent QUALITY Q1–Q3 PASS. Task6 was not started by this reviewer.

## Scope and evidence

The accepted document-boundary source archive was compared byte-for-byte with the current source92 manifest. Exactly four paths changed: `product/src/autostart/mod.rs`, `product/scripts/verify_native.py`, `product/tests/autostart_templates.rs`, and `product/tests/test_verify_native.py`. Exact differences are retained in `delta.patch`.

Source-supported confirmation:

- Registration installation now creates the candidate through the existing protected platform open API with exclusive creation, writes through that opened handle, and syncs it. Unix uses create_new/O_NOFOLLOW and existing ownership/ACL validation; Windows uses CREATE_NEW and its existing protected handle validation. Backup collision checks use symlink_metadata and permit only NotFound. Existing registration ownership/snapshot and preview approval checks remain in place. No new manager operations or foreground lifecycle changes are introduced.
- The owned Mac verification fixture marks an on attempt before dispatch. Failure after an attempted on triggers authenticated off with the same configuration, followed by observed stopped state, before deleting recovery state. Unconfirmed cleanup retains the fixture configuration and reports both original and cleanup failures. The normal path also confirms stopped before removing its fixture.
- Explicit login observations now return exit 1 when their acceptance predicate is false. Login success still requires the existing human attestation and registration/runtime predicates. Preview and install stages remain distinct from login acceptance. No manager check was added to readiness, HTTP, accounting, or fairness paths. XML document-boundary and prior S1–S7/R1–R4 behavior are unchanged outside the described candidate-write block.

Direct fresh reviewer execution: **2 tests / 2 contract cases PASS**, using the exact current verifier source loaded via compile/exec. The first confirms preview exit 2, no apply dispatch, and actual_login_verified=false. The second mocks installation and confirms exit 0 with actual_login_verified=false. All status, terminal environment, apply, and subprocess surfaces were mocked. No worker, OS manager, product executable, Cargo, user registration, or real authentication was used. `contract_probe.py`, `contract-probe.log`, and `commands.json` preserve the precise assertions, invocation, environment and exit 0.

Other-reviewer evidence inspected, not rerun or counted as fresh SPEC execution: QUALITY rereview Q1–Q3 raw source, commands and logs show 5 Rust tests/6 cases plus 3 Python tests/8 cases passing. These cover candidate/backup collisions, normal registration update, changed preview, cleanup ordering and recovery retention, and failed/successful attested login observations. Its held 13 files and FINAL_HOLD were included in this preservation check. Current implementer template/Python/Mac fixture results remain held evidence; older full362 belongs to release57d89, and Task4 installed-client runs belong to the older cc15 release. Neither is represented as current release runtime proof.

## Identity and preservation

`source-identities.json` verifies source92, archive93 (92 matching source bytes and the exact embedded manifest), and these current held identities:

- source manifest: 8b281a41f48313a2d664d9f94737cbcba3794b10688904ec1e969d675296e304
- source archive: 12f6c81a44d95879383ffb0883b2b7681dcc203c18c20dcca6cf2621cfe5c875
- product FINAL_HOLD: 2eb88ddc76d2317615693f501a3d3954db7f9b05a813314b2fa71817471a0541
- release: 88c21502025b0bb0b77db2833845d64bb2c562e345e0c48bc5042651a4ee25a3
- default debug rlib: 0c0bfaf27091f5bc10896015f6d99eaacce21efb11f98ed7c5a51950f409fea7

Before/after checks cover **1655 available paths with zero drift**, including current92 and all preceding available phase artifacts carried by QUALITY after1641 plus its13 own files and HOLD. No held target was rebuilt, no product source was written, and no prior evidence was overwritten. The final audit completed with exit 0. Two earlier reviewer audit attempts exited 1 due to an incorrect assumed prior archive filename and an incorrect expected Python test directory; their failure summaries are retained in audit-attempt1-error.json and audit-attempt2-error.json. These were bookkeeping mistakes, not product test failures; the corrected audit and its raw output are retained as audit.py and audit-final.log.

The historically unavailable two diagnostic logs and two overwritten provisional debug originals remain unavailable/unknown recovery. Matching all available paths does not assert preservation or recovery of those four original artifacts. The prior Mac ACL EINVAL cause remains unresolved. Provisional first QUALITY binaries/source and initially incorrect cleanup metadata are retained; unrelated Python bytecode was not removed.

## Cleanup and handoff

No product workers or fixture directories were created by this final confirmation; remaining owned workers and fixtures are zero. All newly written files are confined to this review directory. Actual Windows/Linux manager execution and real human login acceptance remain separately **UNVERIFIED**, not additional bounded software findings.

execution_finished=true. Sole test/runtime ownership is released to the parent after final SPEC PASS. No product edits, package installation, user configuration/authentication, OS registration, PATH, API/model, VCS, or Task6 actions were performed.
