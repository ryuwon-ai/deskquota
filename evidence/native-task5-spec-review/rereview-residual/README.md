# Native Task5 targeted residual SPEC rereview

Verdict: **CHANGES_REQUESTED — one narrow P2 document-boundary finding remains under R2.** The original R1–R4 failures and the HRESULT correction pass the targeted checks described below. No further broad review or unchanged test suite was run. QUALITY and Task6 have not started.

## R2 remaining — reject content outside the single Task document element

`product/src/autostart/mod.rs:959-961` increments the document-root count only when the top-level element is named `Task`. A second top-level element with a different name but the same Task Scheduler namespace is ignored by that count. At `:869-878`, non-whitespace text outside the element stack is also ignored. The streaming XML reader plus these shape checks therefore accepts malformed whole documents as an owned/current-user task.

Fresh independent source probes reproduced both cases:

1. Append `<Other xmlns="http://schemas.microsoft.com/windows/2004/02/mit/task"/>` after the generated `</Task>` document. Parsing succeeds instead of refusing the second document element.
2. Append ordinary non-whitespace text after `</Task>`. Parsing succeeds instead of refusing text outside the document element.

Both refusal assertions failed in `residual-probe.log`. This is one document-boundary issue with two original counterexamples for the next narrow rereview. Require exactly one whole-document element named Task and reject non-whitespace outside it. Continue permitting ordinary XML whitespace and supported prolog/comment handling; do not add a general schema framework or silently accept a concatenated/partial document. Both full start/end and empty-element events need the shared boundary check.

Impact is bounded to interpreting unexpected manager XML: status/planning can classify a malformed/ambiguous response as owned instead of unknown/blocked. No real foreign task XML, user credential, or actual Windows manager was inspected, and no actual scheduler failure is claimed.

## Targeted results

One independent rustc test binary: **8 passed, 2 failed**, exit **101**. Tests link the exact current default held rlib and dependencies for the context; the product autostart module is copied as an exact byte prefix with only independent private tests appended. `crate::config` reexports the held library. The native user identity adapter deliberately panics if invoked and was never called. No product source was edited or held target rebuilt.

| Target | Fresh bounded result |
|---|---|
| R1 numeric XML references | Decimal and hexadecimal forms match named-entity semantic content. |
| R1 invalid/custom references | Invalid Unicode/control references and custom entities refused, including untracked fields. |
| R2 original extra trigger | Additional any-user, empty LogonTrigger, and other trigger rejected. |
| R2 principal/action/settings shape | Extra/empty singleton sections and missing command rejected. |
| R3 diagnostics | Malformed foreign tag produces the fixed generic reason without the synthetic foreign name. |
| R4 Linux observe-or-refuse | Checked local fragment accepted only for empty drop-ins and reload=no; changed/missing fields refused; foreign fragment exposed. |
| R4 post-removal result | Absent succeeds; surviving global/foreign and unknown state do not report success. |
| HRESULT | `0x8004130F`, access/policy errors, and no exit status remain unknown; only `0x80070002` is absent. |
| R2 whole-document boundary | Second non-Task root and trailing non-whitespace text still accepted: two failed assertions. |

The R4 execution checks are pure helper/property fixtures on this Mac, **not real Linux manager execution**. Source inspection additionally confirms disable → owned unlink → daemon-reload → re-observe ordering and no worker stop/`--now`. The approved minimal policy derives executable from an exact checked local fragment after refusing drop-ins/pending reload; direct loaded ExecStart observation is not claimed or newly required.

`source-and-doc-check.json` confirms updated runtime documentation distinguishes unknown login auth from observed empty-environment failure, limits non-Mac verification to rendering, and records percent-path refusal. No dependency or hot-path scope change was introduced by this residual fix: only autostart module, its template tests, and runtime contract changed; Cargo.lock is unchanged.

## Execution and preservation

- Commands and exact exits: `commands.json`; compile diagnostics: `compile.log`; all pass/failure output: `residual-probe.log`.
- New outputs exist only here. No stock verifier, public 5-test suite, Mac 15-check runtime fixture, mock 10-check verifier suite, installed clients, full Cargo matrix, SDK/aws-lc retry, quota/performance experiment, real service/login, model/API, package/PATH, reference clone, or VCS operation was run.
- Source/fixture inputs and failures are retained unchanged. `cleanup.json` records zero product workers started, zero remaining workers, and removal of the three owned fixture directories. Compiled probes and passive evidence remain.
- `before.json`/`after.json`: **1,519 available preserved paths, zero drift**. Includes prior non-live baseline 1,391, current live source 92, all 35 parent residual-fix preservation rows, and parent audit. Source archive: **93 members, zero content drift**.
- Two older unavailable diagnostic logs and two overwritten provisional debug **originals** remain separate historical gaps. Recovery is unknown; no browsing, reconstruction, restoration, or blanket all-artifacts-preserved claim. Current overwritten-after files and their breach records remain preserved. The earlier Mac ACL EINVAL stays unresolved at its exact syscall/phase/post-state; no retry or resolution claim.

Current source manifest: `5689f03f07026fec4e32b0ba9e2ec1591371a19cc55d053543fa92e454983787` (92 files). Archive: `f481877252ac94abef42f49b2d71a86d09816bac0960257832042815cee6cc0d` (93 members). Release: `57d89c8e22bbc22f10bc16590dba5cf0b998fb463f18e9bad243e3fa37b6b957`. Linked default rlib: `79f129f0b7cdb57d76bced7d8baa7bb13af4fb8a80ff681bd30b0e008f9eaa11`. Cargo.lock: `b216737fbfe2dc9946ffba69e34e26ce9902c6c87c90e974ab58ee850337e3e8`.

The implementer's 362 Rust passes, Python 10, release template/fixture 13 and Mac separation/plutil evidence are held evidence, not fresh executions by this reviewer. Earlier Task4 client probes used the older `cc15…` release and are not attributed to this release. Actual Windows/Linux manager and human login acceptance remains **UNVERIFIED** and is not itself a software finding.

`execution_finished=true`; `release_ownership=released_for_same_implementer_R2_document_boundary_fix_then_same_targeted_SPEC_rereview`. Preserve the eight passing checks and these two failure cases. Do not start QUALITY or Task6 from this verdict.
