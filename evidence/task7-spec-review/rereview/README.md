# Core Task 7 SPEC re-review — 64 simultaneous follow-on writers

**SPEC PASS.** The required 64-writer fixture gap is closed. No remaining finding from this bounded re-review. The original PASS was incorrectly granted; the correction to CHANGES REQUESTED was valid, and this subsequent PASS rests on the newly added fixture and its independently repeated execution.

The original [initial PASS](../README-initial-pass.md) and [CHANGES REQUESTED report](../README.md), including their result JSON and identity snapshots, remain unchanged. This document supersedes the open resource-fixture finding only. It is not a QUALITY review or Task 8 acceptance.

## Scope and source identity

Only `product/tests/resource_bounds.rs` changed among the 41 held source files. Its current SHA256 is `0289bb9e2b00f58d87cc4da7c79d520c08c7424744bb3f72200ab25dc5d9d1a6`. The original 13,451-byte resource test file is a byte-identical prefix of the current file, checked against the held archive; the existing single-writer immediate rejection/progress regression was preserved. All 40 other source files, including production runtime code, Cargo manifest/lock, runtime contract, examples and Pi script, are unchanged. See `source-diff-audit.json`.

Read the appended test, its existing socket capture implementation, implementer supplemental README/results and both raw focused logs. Reviewed the original finding rather than repeating a complete specification review. The four earlier independent retry/classifier probes per profile remain historical evidence for unchanged runtime code; they were not rerun here or added to the counts below.

## Why the correction satisfies the requirement

| Original acceptance concern | Current code and verification |
|---|---|
| All 64 bodies must own partial payload before further demand | `resource_bounds.rs:307–348` opens 64 real sockets, sends 512KiB each, then waits for the server's actual charged ownership to equal exactly 32MiB with zero upstream attempts. |
| All 64 writers must actually request their next chunk together | Lines352–392 create 64 independently scheduled writer tasks and a 65-party barrier, assert 64 arrivals and zero follow-on write attempts, recheck full ownership, and release the coordinator as the last barrier participant. Each writer executes `write_all` after release (lines363–370). This is concurrent sender release; it does not pretend 64 server instructions execute at exactly the same CPU instant. |
| No hidden sender outcome or indefinite wait | Each writer bounds the write/read lifetime to 10 seconds, closes its socket and returns an ID/outcome. Every task is joined. Lines475–482 require 64 unique terminal IDs, 64 attempted and successful socket writes, zero canceled/unexpected outcomes, at least one memory429 and completions+rejections=64. Pending count is zero after draining the JoinSet. |
| Immediate memory rejection releases partial ownership | The memory429 branch validates `gateway_memory_full`. Every observed outcome samples ownership and checks ≤32MiB; after rejections the stricter remaining-contender upper bound is checked (lines412–430). The unchanged production semaphore uses immediate `try_acquire_many_owned` before retain. The preserved deterministic single-writer test additionally checks the precise 512KiB release within its one-second rejection bound. |
| Another completable request progresses after rejection | Lines431–441 send a fresh generation after the first observed memory rejection and require HTTP200 within five seconds. The preserved single-writer test still proves fresh progress while the other 63 bodies remain incomplete, so the new race case does not weaken that gate. |
| Full wire accounting and cleanup | Lines444–457 require body ownership, admission active holds and queue tickets to return to zero. Lines483–502 match actual captured upstream contender IDs to the completed ID set, forbid duplicates and rejected IDs, and require exactly one fresh capture. The existing fixture increments attempts only after `read_request` has read complete HTTP head and body (`support/fixture.rs:93–98,443–505`), not merely at TCP accept. |
| Racing successes are legitimate | The test does not hard-code exactly one rejection or 63 successes. A rejection releases capacity that other writers can use. It accepts any complete accounting split with at least one correct memory rejection and no unaccounted outcome. |

This is the previously missing real socket scenario, not 64 sockets passively waiting for their last network byte. The writer ID header is ordinary forwarded fixture metadata and does not add a gateway test-cost or scheduling control.

## Independent execution in reviewer-owned target

To verify the implementation's test rather than trust its report, an external Cargo package directly names the actual product test source as its integration-test path and links the actual `llmgw` library. No algorithm or test body was copied. A byte-identical local copy of the public fixture TOML supplies its expected relative read path. Dependency resolution is offline; all 204 registry package versions and checksums match product Cargo.lock (`lock-comparison.json`). Only the reviewer's evidence target was used.

| Observation | Reviewer debug | Reviewer release |
|---|---:|---:|
| Focused tests passed / failed | 2 / 0 | 2 / 0 |
| New barrier case writer arrivals / follow-on attempts / successful socket writes | 64 / 64 / 64 | 64 / 64 / 64 |
| Original contender HTTP200 / memory429 | 63 / 1 | 63 / 1 |
| Canceled / unexpected / pending original contenders | 0 / 0 / 0 | 0 / 0 / 0 |
| Fresh request HTTP200 | 1 | 1 |
| New-case data ingress / upstream HTTP attempts | 65 / 64 | 65 / 64 |
| Ownership samples / max charged bytes / final charged bytes | 65 / 33,554,432 / 0 | 65 / 33,554,432 / 0 |
| Both focused cases: data ingress / upstream attempts | 130 / 65 | 130 / 65 |

Both commands exited0. The second focused case is the preserved single-writer regression: its 65 ingress comprise one memory429, 63 canceled incomplete senders and one fresh HTTP200, with one upstream attempt. Thus both cases per profile total 65 completed HTTP200, two memory429 and 63 canceled incomplete senders. Across both profiles the execution total is four passing focused test executions, 260 ingress and 130 actual upstream HTTP attempts. Authenticated status/control reads are excluded. These are independently executed product-authored tests, not four newly designed scenarios.

Raw `debug.log`, `release.log` and parsed `run-summary.json` preserve these results. No failure occurred in this re-review execution. The implementer's initial and focused logs are separate pre-existing evidence and are not added to this reviewer execution denominator. **No full 187-test run is claimed**, nor was the old 186-test suite rerun. No Pi or held foreground binary was executed by this re-review.

## Limits preserved

The barrier synchronizes follow-on senders; OS/network/runtime scheduling determines server processing order. The ownership figure is the charged payload semaphore state sampled at explicit gates, not a continuous allocation trace or RSS maximum. The rejection-release upper bound is supported by actual samples and unchanged source ownership, while the preserved single-writer case supplies the exact immediate-release gate.

The previous resource distinctions remain: 24 real slowreader sockets were observed before upstream EOF; 20 retained delivery receivers after EOF are a separate production-primitive proof; the 9MiB conservative delivery-only aggregate is source-derived. This correction establishes none of performance, RSS targets, Windows safety, real provider token compliance or Task 8 outcomes.

## Commands and final identity

```sh
# cwd: /Users/ryuwon/Desktop/ryuwon-project/llm-gateway-research/evidence/task7-spec-review/rereview
python3 identity.py identity-before.json
cargo test --offline --target-dir ../target --test resource_bounds sixty_four_ -- --nocapture > debug.log 2>&1
cargo test --release --offline --locked --target-dir ../target --test resource_bounds sixty_four_ -- --nocapture > release.log 2>&1
python3 identity.py identity-after.json
```

`identity-before.json` and `identity-after.json` match the authoritative `product-task7-spec-fixed-source-manifest.json`: all 41 current source hashes and both held binaries match before/after reviewer execution. No product file or product target binary was edited or rebuilt.

- Debug SHA256: `4fb6c03938774b8ca756490ea6c3f624bc5c2bf9e72cdf54c9fb9fb7421609f0`
- Release SHA256: `9be8dcf3cb880b41d6662a41eb91d25c27df07e7be6f23d3fae62c81b374b36d`

The unchanged binary/runtime identities allow prior functional evidence to remain associated with the same held runtime. Hashes still do not independently prove source-to-binary provenance. No product change is requested; proceed to the separate QUALITY gate under the parent's workflow.
