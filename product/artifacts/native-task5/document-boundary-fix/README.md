# Native Task5 document-boundary fix evidence

Status: **DONE_WITH_CONCERNS**. Execution is finished and ownership is released for the same targeted Native Task5 SPEC rereview. QUALITY and Task6 have not started.

## Change

Only `src/autostart/mod.rs` changed. The Windows task parser now:

- counts every top-level Start or Empty element and requires exactly one top-level `Task`;
- rejects non-XML whitespace text and all references outside that element;
- accepts only XML whitespace `#x20`, `#x9`, `#xD`, and `#xA` outside the root;
- permits one XML declaration only as the first event;
- retains valid declaration, whitespace, comment, and no-declaration controls.

The parameterized regression covers the reviewer’s second non-Task root and trailing-text failures plus an additional Task root, leading text, NBSP, an outside reference, and misplaced declarations. [XML 1.0 production S](https://www.w3.org/TR/xml/#NT-S) defines the four accepted whitespace characters.

`Cargo.lock` is unchanged at `b216737fbfe2dc9946ffba69e34e26ce9902c6c87c90e974ab58ee850337e3e8`.

## Fresh verification on the final source

- Focused TDD RED: 0 passed, 1 failed; retained in `red-document-boundary-attempt1.log`.
- Final manager snapshot suite: 11 passed, 0 failed.
- `cargo fmt --all -- --check`: passed.
- `cargo check --locked --all-targets`: passed.
- `cargo clippy --locked --all-targets -- -D warnings`: passed.
- `cargo build --locked --release`: passed.

The earlier 362-test full suite and macOS plist/runtime-separation fixture remain held evidence for release `57d89c8e22bbc22f10bc16590dba5cf0b998fb463f18e9bad243e3fa37b6b957`. They were not rerun or relabeled as evidence for the current release. This parser-only phase started no worker and created no OS registration.

## Current identities

All paths are relative to `/Users/ryuwon/Desktop/ryuwon-project/llm-gateway-research/product`.

- Release binary: `target/native-task5-document-boundary-fix/release/llmgw` — `10011952` bytes — `cae45cd6da245b00885b33a1e1e0df97647d9e518089724993d1c7e2e7f3107b`
- Debug binary: `target/native-task5-document-boundary-fix/debug/llmgw` — `30632904` bytes — `df92dd0eccf458af31258f7e3b882ab13deecf0d8656015755758b210b9b5d86`
- Default rlib: `target/native-task5-document-boundary-fix/debug/deps/libllmgw-1750bcafbc9dfb14.rlib` — `50593880` bytes — `cfda152fdcdb24bc4f18f25538c944f45b2cf086df89e804af99b75c49d4e330`
- Source manifest: `artifacts/native-task5/document-boundary-fix/source-manifest-final.json` — `16015` bytes — `b0c0ade22ce352c278c4019f8420ef24d16b6870839a716af37bd9e7aee2f801`
- Source archive: `artifacts/native-task5/document-boundary-fix/source-hold-final.tar.gz` — `308304` bytes — `12919fe62ec6cc438ceb00b85bbd817f4e60aa19bd005576cc1b74efc5242282` (93 members)

The manifest has 92 source rows based at `/Users/ryuwon/Desktop/ryuwon-project/llm-gateway-research` with 0 drift. Its release hash matches the current release binary. It contains no probe check because no runtime fixture was run for this binary.

## Preservation and limits

- 1442 retained non-live files rechecked with 0 drift.
- Cleanup/privacy passed: no fixture path or worker remains, and no tested synthetic secret literal occurs in this phase’s artifacts.
- Actual Windows/Linux manager behavior and human login remain unverified.
- Two unavailable historical diagnostic logs, two overwritten provisional debug originals, and the earlier macOS ACL EINVAL remain disclosed with unknown recovery. No reconstruction or blanket preservation claim is made.

All failed/corrected logs use distinct names and remain retained, including the parse typo exposed while adding the XML-space predicate.
