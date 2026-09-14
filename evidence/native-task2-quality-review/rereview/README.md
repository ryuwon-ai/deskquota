# Native Task 2 QUALITY re-review — Q1/Q2

**PASS. Q1 and Q2 are resolved; no open Critical, Important, or Minor finding in the bounded fix and adjacent Task2 paths reviewed.** This accepts the reviewed Native Task2 quality scope, not production, cross-OS, packaging, client integration, or performance readiness.

## Reviewed identity and scope

- WHAT_WAS_IMPLEMENTED: comment-preserving selective edits for existing TOML models/roots, and local proxy/TLS client preflight before restart stops the current worker.
- PLAN_OR_REQUIREMENTS: Native experience plan Task2; native setup specification; prior Q1/Q2 quality report and strict existing-data/safe-runtime contract.
- BASE: F8 source archive SHA256 `682f1dde419f2fa35ea4adadf1816b889e9c2b927ce6820282733b10cbec0b89`.
- HEAD: quality-fix source archive SHA256 `b221562a379edbd6cfe1e6faad77a1176ac0622774936a479d7734a190a6cd99`, 215,107 bytes. Manifest SHA256 `3495daf57907d5e4472513aaef5131cd6bf418fa58d369041b2d0484454ca603`; 71 source files verified against archive contents and live files.
- Held release used for independent PTYs: `product/target/native-task2-quality-fix/release/llmgw`, SHA256 `aa4015d5b34426b5d8e502d8cfbb2f4af2757e16a7cd2094b43e219c974cb3a6`, 9,335,616 bytes.
- Six changed source/test paths reviewed via `source.diff`: lifecycle/mod, server, setup/mod, new setup/document, lifecycle_contract, setup_contract. No product edits or Git actions. Original quality report and evidence remain unchanged.

## Resolved findings and fresh direct evidence

| Finding / adjacent case | Observation |
|---|---|
| Q1 original endpoint edit | Real PTY Save only retains original heading and port comments and adds Completions alongside Responses; exit 0. |
| Q1 untouched second root | First-root endpoint edit retains the second root's entire exact TOML suffix, including multiline endpoint/model array internal comments. |
| Q1 model edit, array-of-tables | Actual wizard changes ID and fallback to model-b/128, updates the root model reference, and keeps all fixture comments. |
| Q1 model edit, inline tables | Same intended edits preserve inline model/root container comments and original heading/port comments. |
| Q2 original missing CA | Real PTY Save and start returns exit 1 with actionable sanitized CA error; old authenticated identity remains running; saved config is pending restart; zero upstream requests. |
| Q2 malformed / empty CA | Non-PEM and empty-file PTYs preserve the running identity and return actionable errors without generic worker_start_failed. Fresh lifecycle test also executes the actual binary with malformed BEGIN CERTIFICATE/base64 and verifies the specific invalid-PEM error. |
| Valid CA then external loss | Actual on with the explicit valid fixture CA reaches readiness. Deleting only that owned CA file and repeating on preserves the same running identity; no upstream request. |
| F8 saved pending rerun | Save only preserves the original worker with pending_restart; a second wizard's Save and start applies the new fingerprint and clears pending state. |
| F6 unchanged Save and start | Third wizard preserves exact config bytes and the entire already-running identity. |

Independent release driver: `probe_rereview.py`; results `probe-results.json`. Ten real PTY sessions and additional native lifecycle calls used nine owned configs. All assertions passed; loopback HTTP fixture observed **0 upstream requests**. This is a check of offline setup/runtime behavior, not provider inference. Before/after TOML and terminal transcripts are retained.

## Quality assessment

**Strengths, source-supported:** Existing documents now always use the installed toml_edit editor, while new documents retain ordinary typed serialization. The new setup/document module has one responsibility, updates only typed fields that changed, supports both TOML collection forms, and reparses the rendered output to require semantic equality with the intended config. Unchanged root/model fields are not reconstructed. There is no generic Task3 patch or compatibility layer.

Restart calls the shared upstream-client construction under its existing operation lock and before stop. The actual worker uses the same builder. Missing/invalid/empty CA failure details stay sanitized through the existing BuildError display. This fixes the deterministic local failure without adding an upstream probe, repeated discovery, a replacement server, or a request-path watcher. Idempotent on remains outside this preflight, which is appropriate for an already authenticated matching worker.

**Issues:** Critical 0; Important 0; Minor 0. No additional actionable defect was found in the six-file change and adjacent paths checked. This is not a claim of exhaustive absence of defects.

**Recommendations:** Retain the original counterexamples and these passing probes as regression evidence. Continue to distinguish real native runtime results from platform models and static review in later tasks. No additional implementation is requested for this review.

## Fresh verification versus reused evidence

**Fresh:**

- `CARGO_TARGET_DIR=/Users/ryuwon/Desktop/ryuwon-project/llm-gateway-research/product/target/native-task2-quality cargo test --locked --offline --lib --test setup_contract --test lifecycle_contract` — exit 0; lib11 + lifecycle26 + setup42 = **79 passed, 0 failed**. Full log: `focused-tests.log`.
- `CARGO_TARGET_DIR=/Users/ryuwon/Desktop/ryuwon-project/llm-gateway-research/product/target/native-task2-quality cargo fmt --check` — exit 0, empty diagnostic log. cargo-fmt does not accept --locked; this check resolves no dependencies and did not rewrite source.
- Exact held release native probes above: all assertions passed. No full accounting or benchmark matrix rerun.
- **71 source and 426 protected files: zero before/after drift.** Six separately pinned artifact/binary/report identities match. Original README remains SHA256 `55265cf113515d96cf280d6cbeb96840f2fcc7e8edd60eceee30b7624177e549`.
- All ten PTYs waited; nine configs authenticated off/stopped; HTTP fixture stopped and joined; owned scratch removed; no held/review worker remains. Cleanup is recorded in `cleanup.json` and `after.json`.

**Reused, not rerun here:** implementer's all-targets264, check/clippy/release, eight final PTYs, and parent's 71-file archive/16-identity audit; prior SPEC F1–F8 and quality review history. Fresh evidence above independently resolves Q1/Q2 and samples F8/F6; reused reports do not replace it.

**Limitations:** macOS host and synthetic loopback fixtures only. Actual Windows/Linux runtime/ACL behavior, real upstream inference/tools, installation/login automation, client patching, fresh RSS/latency, and performance superiority remain unverified. No real user config/auth files, startup registration, paid API/model calls, downloads, or source mutations were performed. Client/login intent remains intentionally pending for later tasks.

## Assessment

**Ready for Native Task2 quality acceptance: Yes.** The original failure inputs and directly affected adjacent cases now satisfy preservation and safe restart expectations, with fresh tests and held-binary runtime evidence. Task3 has not been started. Execution is finished and runtime/test ownership is released.
