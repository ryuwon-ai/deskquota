# Native Task5 document-boundary SPEC rereview

Verdict: **PASS for the bounded implementation/fixture SPEC scope, with disclosed real-OS limitations.** The remaining R2 document-boundary finding is closed. No actionable SPEC finding remains from this targeted rereview. Product ownership is released for a fresh independent QUALITY review; this reviewer did not start QUALITY or Task6.

## Fresh verification

**4 independent tests, 25 cases, all passed; exit 0.** `document-boundary-probe.log` retains every case result.

- Original failures: second non-Task document root and trailing non-whitespace text are both rejected (**2 cases**).
- Direct invalid adjacency: additional Task/paired or empty Other roots, leading text, leading/trailing NBSP, vertical tab/form feed, outside named/numeric references, misplaced/duplicate/nested declarations, and no document element are rejected (**16 cases**).
- Valid controls: generated declaration, no declaration, all four XML whitespace characters, leading/trailing comments, correct declaration placement with comments, and processing instructions remain accepted with the same parsed result (**7 cases**).

The product module was copied as an exact byte prefix with only independent private tests appended. The context reexports config from the exact held default rlib; its native user identity adapter panics if used and was never called. One rustc compilation wrote only into this reviewer directory. No Cargo command, held rebuild, product edit, OS manager, worker, user config/auth, or registration was executed. Exact compiler/test commands and exits are in `commands.json`; `source-copy-identity.json` and `source-check.json` record the source relationship.

This is native Rust parser execution on macOS under a reviewer context, **not Windows Task Scheduler execution**. The current held release was not executed. The prior eight targeted R1–R4/HRESULT passes remain their own earlier evidence; they were not repeated simply for counts. No public/Mac/verifier/full-suite matrix, installed client, SDK/aws-lc, core/performance, or real-login test was rerun.

## Source disposition

Only `product/src/autostart/mod.rs` changed from the prior residual-fix source. Cargo.lock and the other 91 source inputs are unchanged. The shared Start/Empty handling now counts all document elements, requires exactly one Task document element, and rejects text or references outside it. Outside whitespace is restricted to XML S characters: space, tab, CR, LF. Declaration placement is checked separately. Valid whitespace/comments controls prevent this refusal fix from rejecting the supported normal forms.

The earlier runtime documentation corrections, minimal Linux observe-or-refuse policy, HRESULT handling, and parser/content fixes were not expanded into a general schema or service framework. Their prior SPEC disposition remains in `../rereview-residual/README.md` and `../rereview-s1-s7/README.md`; this review closes only the remaining document-boundary item.

## Preservation and cleanup

`before.json` / `after.json`: **1,562 available paths, zero drift**. This covers the retained non-live baseline 1,442, current source 92, all 27 parent document-boundary phase preservation rows, and the parent audit. The current source archive has **93 members, zero content drift**. Previous failed/corrected logs and earlier reviewer results remain untouched.

`cleanup.json`: the two owned empty HOME/TMP directories were removed, zero active fixture paths remain, and zero product workers were started or left. Captured source copies, compiled probes, logs, and results remain here. No package, PATH/profile, reference clone, model/API, or VCS mutation occurred.

The historical gaps remain disclosed: two unavailable diagnostic logs and two overwritten provisional debug **originals**, recovery unknown, no restoration or reconstruction. Available overwritten-after files and breach records are preserved without substituting them for original identities. The earlier Mac ACL EINVAL remains unresolved at the exact prior syscall/phase/post-state and was not retried or claimed fixed. Zero current drift does not erase those incidents or support a blanket historical preservation claim.

## Held identities and evidence limits

- Source manifest, 92 inputs: `b0c0ade22ce352c278c4019f8420ef24d16b6870839a716af37bd9e7aee2f801`.
- Archive, 93 members: `12919fe62ec6cc438ceb00b85bbd817f4e60aa19bd005576cc1b74efc5242282`.
- Current release `target/native-task5-document-boundary-fix/release/llmgw`: `cae45cd6da245b00885b33a1e1e0df97647d9e518089724993d1c7e2e7f3107b`.
- Exact linked default rlib: `cfda152fdcdb24bc4f18f25538c944f45b2cf086df89e804af99b75c49d4e330`.
- Cargo.lock: `b216737fbfe2dc9946ffba69e34e26ce9902c6c87c90e974ab58ee850337e3e8`.

Implementer focused 11/fmt/check/clippy/release passes are held evidence, not independent executions here. The prior full Rust362/Python10/macOS fixture/template evidence belongs to the older `57d89…` release and is not relabeled as current runtime evidence. Task4 installed-client checks belong to the still older `cc15…` release.

Actual Windows/Linux manager execution, human login/on/off acceptance, current-user registration, and Windows console behavior remain **UNVERIFIED**. These external gaps are separate from this bounded software SPEC pass. No support-complete or CI/CD-safe claim follows.

`execution_finished=true`; `release_ownership=released_for_fresh_independent_QUALITY_review`.
