# Core Task 7 — independent SPEC review

**SPEC PASS.** The held implementation matches the approved Task 7 scope after the parent clarified optional JSON null semantics during this review. No product change is requested. This is a specification conclusion only; QUALITY review and Task 8 remain separate.

Reviewed on 2026-09-12 in `/Users/ryuwon/Desktop/ryuwon-project/llm-gateway-research`. Research and `product/` are not Git repositories. Read root/research AGENTS.md, research README/HARNESS, the Task 7 plan, retry classification research, runtime contract, actual implementation/tests, and raw implementation logs. The requested subagent-driven-development spec-reviewer role was applied without further delegation. A memory quick search found no relevant history and contributed no substantive evidence.

## Direct independent execution

The external Cargo harness uses the actual `llmgw` product library by path dependency. It implements no copied admission, timing, classification or transport algorithm. Its loopback fixture counts a request after a complete HTTP request head arrives, not at TCP accept. All wire requests are GET metadata through production Reqwest forwarding. The existing doc-hidden monotonic clock advances known-RPM warmup and cooldown deterministically; these are real sockets with controlled admission time, not production wall-clock timing measurements.

| Independent test | Direct observation, per profile |
|---|---|
| `tests/probe.rs:58` duplicate/null and complete JSON validation | 12 negative forms rejected, including null/value duplicate ordering, escaped duplicate keys, duplicate null details/code, invalid ignored nested surrogate and trailing JSON; 2 explicit-code/optional-null forms accepted. No HTTP. |
| `tests/probe.rs:86` saturated cooldown after canceled retry | Started hold settles before retry wait; active=0 and RPM debit=1. Canceling retry, advancing time beyond its old deadline, and applying zero cooldown do not unblock a new root. Exactly one library start, no HTTP. |
| `tests/probe.rs:217` cooldown before rejected response EOF | While upstream 429 body is gated, its header establishes shared cooldown and the other root queues despite spare concurrency. After EOF, both wait with active=0/RPM=1; 64.999s still has one attempt. At 65s, retry and other root proceed through the same ledger. 2 ingress, 3 upstream HTTP attempts, 2 final HTTP200, exact RPM3. |
| `tests/probe.rs:293` observation boundary | Exactly 16,384 bytes of valid JSON plus whitespace qualifies for one replay; 16,385 bytes raw-forwards unchanged with no replay. 2 ingress, 3 upstream attempts, one final HTTP200 and one HTTP429, exact RPM charges. |

`debug.log` and `release.log`: **4 tests passed / 0 failed per profile**, command exit0. Each profile has **4 data ingress / 6 actual upstream HTTP attempts / final 200×3 and 429×1**; authenticated status reads are excluded. Combined across the two profiles: 8 test executions, 8 data ingress, 12 upstream HTTP attempts. The manual trace's start is not counted as a wire attempt. No held CLI binary was executed by this reviewer; parent foreground binary probes are a separate evidence class.

The harness initially used a redundant output path relative to its own directory (`debug-initial.log`, exit101: no test target), then referenced `RequestCost::metadata()` instead of the actual `Metadata` constant (`debug-compile.log`, exit101). Those harness errors are preserved and are not product failures or behavioral REDs. The subsequent source correction only changed the review harness. The initial compiler diagnostic echoes synthetic fixture literals; it is not product runtime logging.

## Requirement-by-requirement result

| Requirement | Evidence and conclusion |
|---|---|
| Required files and minimal integration | `src/admission/retry.rs`, `tests/retry_contract.rs`, `tests/resource_bounds.rs` exist. `config/validate.rs:38` defaults the immutable boolean to false; stream receives that configured value. Config/schema, queue/hold, deadline/status and runtime-doc edits support the requested behavior. No provider registry, transport framework, client wizard, model/runtime helper or benchmark feature was added. Source-supported. |
| Opt-in at most one extra attempt | `transport/stream.rs:185–270` only considers complete 429 prefix observation, checks classification/timing, and guards with `!retried`; replay sets `retried=true`. Parent foreground results and implementation real-wire tests cover default0/opt-in1. Independent exact-cap wire case confirms the positive path. |
| Seconds/date/ms, maximum, overflow and malformed timing | `admission/retry.rs:7–16,91–120` uses existing httpdate/rand dependencies, max across all values, digit-only integer syntax and conservative Duration::MAX overflow. `admission/mod.rs:251` saturates and only extends the group deadline. Independent cancellation/zero-update probe passes. Raw implementation tests cover seconds/date/ms/mixed duplicates/negative/NaN/inf/suffix/overflow; parent foreground probes also cover timing. |
| Narrow classification, full validity, raw unsupported responses | `admission/retry.rs:24–85` bounds observation, requires identity JSON, validates whole UTF-8/JSON before typed duplicate-preserving projection, and only accepts specified code/type shapes. Details/conflicting/permanent/generic-Claude cases reject. `stream.rs:417–456` preserves observed prefix/overflow/error and then streams raw bytes. Independent duplicate/null and exact-byte boundary cases pass. No free-form message inference. |
| Missing timing fallback | `retry.rs:87–89` supplies inclusive 1000–1250ms; `stream.rs:224–235` only applies it after recognized rejection and both headers absent. Explicit invalid timing denies replay instead of becoming missing-header fallback. Source plus implementation socket evidence. |
| Same Queue/Ledger, no slot in wait, RPM per actual start | `Hold::retry` (`admission/mod.rs:315–331`) drops prior hold before `acquire_at`, retaining root/cost/endpoint/age/deadline. `stream.rs:163–169` commits at send poll; each retry reenters that same path. Independent early-header/EOF test directly observes active0/RPM1 while both roots wait, and RPM3 for three wire attempts. |
| Original deadline and aging; cooldown outlives request | `Hold::retry` preserves both stored timestamps; queue expiration remains original. Overall deadline begins before body collection at `server.rs:626–640`. Queue cooldown is separate monotonic group state. Implementation retry tests at lines270,353,455 exercise unchanged age/deadline and later clients; independent saturated cancellation probe establishes no reset by cancel/zero update. Age equality is a library-seam observation, not a new wire-visible field. |
| Reserved/Actual/unknown semantics | `quota.rs` retains rolling60s/startup60s/8192 storage and exact provisional/debit rules. `stream.rs` drops rejected hold with unknown usage; no invented token refund. `retry_contract.rs:621–676` checks identical POST body/auth and retained twice-estimated TPM debt under both Reserved and Actual. Byte-based input proxy is not actual provider-token compliance. Source and audited implementation evidence. |
| Root fairness and metadata | `queue.rs` retains max64, rootFIFO/RR, only actual non-fit bypass counting, age5s/bypass8 oldest-valid-head selection and sticky barrier. `config/validate.rs` keeps max16 registered roots; headers do not create shares. Models/count_tokens continue same admission, RPM1/TPM0. Independent other-root metadata socket case uses the same ledger. Full fairness is covered by retained tests and parent foreground probes, not newly benchmarked here. |
| POST ambiguity,503,redirect,partial stream never replay | `UpstreamClient::new` (`transport/upstream.rs:14–23`) explicitly disables Reqwest retry/redirect. `stream.rs` qualifies only 429 before downstream headers and rejects transport/partial errors. `retry_contract.rs:377–394` sends POST for each ambiguity/503/307/partial-SSE scenario and asserts exactly one wire attempt. Source plus audited test logs and parent stream/CLI probes. |
| Cancellation, drain and stop | Existing worker-owned drain holds upstream through EOF/error/deadline; retry-wait cancellation/stop is separate and terminates without another attempt (`stream.rs:244–254`). Stop10s closes listener, aborts and joins connection/worker tasks (`server.rs:468–479,503–543`). Implementation cancellation test and parent forced10s stream test retained. Independent manual cancellation starts no new work. |
| Request memory and progress | `body_budget.rs:25–57` obtains incremental permits before retention with `try_acquire_many_owned`, and returns immediately on failure; partial Vec and permits drop. `server.rs:642–653` returns protocol429 memory error. Resource test at line45 has 64 simultaneously incomplete512KiB bodies holding32MiB, one extra byte rejected, immediate released512KiB, another complete generation progresses while63 remain incomplete, then RST cleanup to0. Only one sender actually transmits the extra byte at the deterministic rejection gate; this is not64 rejection responses. Source and audited execution evidence. |
| Protocol caps/timeouts and connection bounds | Single body8MiB, total32MiB, headers32KiB/10s, body30s, overall30min, queue120s and ingress128 are preserved (`server.rs:35–41,410–445,623–684`). Resource suite tests declared cap413, overall504, body408,128 accepted sockets/129th close/reentry. Header parser exhaustion/timeout can close at HTTP parser level before a protocol handler exists; no new claim that every malformed header receives JSON. |
| Delivery bounds/observer transparency | `stream.rs:15–20,278–291,465–522` bounds each lifetime to8 queue items/64KiB and copied chunks≤16KiB. Observer has256KiB fixed metadata and overflow Unknown; delivery remains independent (`stream.rs:373–388,624–627`). Existing stream test around line325 checks overflow metrics and byte-preserving response. Implementation tests retained in both profiles. |
| Logs and evidence discipline | Independent literal scan of38 implementation Task7 `.log`/`.json` files found0 hits for4 synthetic local/control-token, credential and prompt markers (`sentinel-audit.json`). This finite scan does not prove all secrets can never leak. Raw RED/GREEN distinction and failed fixture stages preserved. |

Optional null clarification: during this review the parent confirmed that optional `error.type`, root optional discriminators and absent conflict details may treat explicit JSON null as absent, provided a required allowlisted string `error.code` exists and no non-null conflicting marker exists. This clarifies the runtime contract's already-visible `(or absent/null)` wording; it was not claimed to be a previously discussed acceptance decision. Duplicate keys still reject, as independently tested. Parent owns the corresponding research-note wording update; product remained held.

## Resource evidence limits

The runtime's conservative **9MiB delivery-only bound** is source-supported: at most128 connection tasks plus16 active upstream workers, each owning at most64KiB of charged delivery bytes. Locally read Hyper1.8.1 `h1/dispatch.rs:26,328–351,532–578` holds one output body and one service future; `h1/conn.rs:1071–1090` returns to keepalive only after read/write completion. This deliberately overcounts overlapping lifetime ownership. Retry waiters have no delivery channel and rejected prefixes are dropped before reentry.

Audited raw debug/release logs show24 stalled readers,16 upstream attempts,8 waiting, **zero upstream EOF before cancellation**, and observed delivery payload1,046,860 /1,038,824 bytes. These are actual slowreader socket observations. The separate `stream.rs:924` production primitive retains20 receivers after producerEOF, observes1,310,720 bytes and release-to-zero. It is **not** proof of20 EOF-completed socket workers. Neither case measures process RSS. Concurrency16 alone is not a process-wide1MiB bound.

Request Vec spare capacity, bounded JSON parsing/prefix copies, current transport chunks, Hyper/TLS/kernel buffers and allocator overhead remain outside the delivery payload figure. This review claims no latency/RSS target, benchmark superiority, Windows/Linux native safety, real provider quota correctness, model execution or backend compute cancellation.

## Existing evidence audited, not rerun

`implementer-log-audit.json` independently sums raw `final-debug-tests.log` and `final-release-tests.log`:186 passed,0 failed,10 test groups per profile (including the zero-test binary group). Formatting output is empty; check/clippy logs show normal completion. Historical logs contain actual behavioral assertion failures for cooldown bypass, replay/classification, ignored Unicode, body deadline, queue-full terminal reason and positive-overflow cooldown loss. Compile-only/config/fixture expectation failures are distinct. Final186 is6 library+19 config+22 fairness+12 quota+6 resource+18 retry+32 stream+27 usage+44 wire. This reviewer did not rerun186 tests and does not inflate the independent4 count with those logs.

Parent foreground artifacts supplied separately identify debug/release retry, stream and debug fairness/CLI runs against held binaries. Their counts are not added to this harness's12 wire attempts. Pi positive/negative user flows also remain existing implementation evidence, not reviewer reruns.

## Commands and integrity

All writes were confined to this directory. Commands used its own `target/`; product source, lockfile and both existing binaries were not rebuilt or changed. Offline resolution retained the exact204 registry package versions and checksums from product (`lock-comparison.json`). No Git command, service/config mutation, external message, download or real API/model call occurred.

```sh
# cwd: /Users/ryuwon/Desktop/ryuwon-project/llm-gateway-research/evidence/task7-spec-review
python3 identity.py identity-before.json
cargo test --offline --target-dir target -- --nocapture > debug-compile.log 2>&1
# Harness Metadata constant corrected; rustfmt only this harness test file.
rustfmt --edition 2024 tests/probe.rs
cargo test --offline --locked --target-dir target -- --nocapture > debug.log 2>&1
cargo test --release --offline --locked --target-dir target -- --nocapture > release.log 2>&1
python3 identity.py identity-after.json
```

Before/after identity checks independently match **41 source hashes +2 binary hashes** against the held parent manifest. `identity-after.json` is the final check after both harness profiles completed:

- Debug: `4fb6c03938774b8ca756490ea6c3f624bc5c2bf9e72cdf54c9fb9fb7421609f0`
- Release: `9be8dcf3cb880b41d6662a41eb91d25c27df07e7be6f23d3fae62c81b374b36d`

Hashes identify held bytes; they do not independently prove source-to-binary build provenance. No remaining SPEC blocker found. Stop here for the separate QUALITY stage.
