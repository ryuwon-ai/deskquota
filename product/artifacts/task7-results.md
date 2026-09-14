# Core Task 7 implementation handoff

2026-09-12. Implementation is held for parent foreground probes and fresh SPEC/QUALITY review. This report does not mark Task 7 accepted or start Task 8.

## Implemented

- Default-off immutable `retry_transient_429` boolean; qualified `slow_down` / `rate_limit_exceeded` HTTP429 permits at most one additional attempt. Exact request body/headers are shared unchanged; SDK/Reqwest retries and redirects remain separate and disabled at gateway transport.
- Shared Queue cooldown for standard seconds/HTTP-date and Azure integer milliseconds, maximum across values; positive decimal overflow saturates conservatively, malformed timing cannot become an early fallback replay. Recognized missing-header errors alone get 1,000–1,250ms group cooldown.
- Full bounded UTF-8/JSON validation, duplicate/conflicting discriminator rejection, spend/usage/unknown/encoded/oversized errors forwarded without replay. No free-form message guesses or provider registry.
- Same Queue/Ledger retry reentry with original age, queue deadline and overall deadline. Slot-free wait, each actual start charges RPM; Reserved/Actual tests retain unknown rejected-attempt TPM debt. Retry queue cap returns protocol429 and `terminal_admission_rejected`.
- Overall deadline starts before request-body collection. Authenticated `stored_request_bytes` exposes the existing 32MiB payload budget for deterministic partial-body gates. No hold-and-wait memory allocation was introduced.

The runtime contract documents supported classifications, unsupported responses, ownership, bounds and official source distinctions. Added `httpdate = 1.0.3` and `rand = 0.10.2` as direct dependencies already resolved in the accepted lockfile; rand thread_rng MSRV1.85 is below Rust1.88. No runtime helper executables.

## Directly verified

| Check | Final result |
|---|---|
| Accepted Task6 baseline | 38/38 source hashes matched before edits; historical evidence/archive preserved |
| Formatting / all-target check / clippy `-D warnings` | exit0 |
| Debug Rust tests | 186 passed, 0 failed |
| Release Rust tests | same186 passed, 0 failed |
| Added tests | retry_contract18 + resource_bounds6 + delivery EOF primitive1 =25, alongside all161 baseline tests |
| Builds | debug and release exit0 after final tests |
| Installed Pi0.84.2 positive, each profile | completion1 wire attempt + read-tool2 wire attempts; both user flows successful |
| Pi gateway-off negative, each profile | expected exit1, assistant failure observed, 0+0 wire attempts; zero positive flows |
| Synthetic prompt/credential/local-token log scan | 0 hits in Task7 log/JSON artifacts |

`task7/commands.md` records commands and each failed/intermediate stage. `green-*` names never replace actual exit/results. The first opt-in RPM assertion and first fresh generation model were fixture errors, explicitly corrected and preserved. Genuine behavioral REDs cover shared cooldown, absent opt-in/replay, malformed delay replay, ignored invalid UTF-8, overall body deadline, queue-full terminal reason, and huge decimal cooldown loss.

`retry_contract` contains 136 data ingress requests and 69 asserted actual upstream wire attempts per profile, excluding authenticated control reads; it also has two no-HTTP manual-clock traces. Failed, queued/canceled, raw429 and replayed requests remain in this denominator. Includes a 65-ingress retry queue-full case with exactly1 upstream attempt; the other64 cancel while queued. The replay POST test runs both Reserved and Actual with real HTTP body-byte estimation and checks unchanged body/auth plus retained TPM debt. KnownRPM traces use the existing manual-clock seam to skip real-time warmup and stay separate from ordinary loopback timing claims.

Resource evidence:

| Scenario | Direct result |
|---|---|
| 64 incomplete senders, each512KiB and requiring more body bytes | Exact32MiB held. First extra byte receives429, releases512KiB. Fresh complete generation succeeds while other63 remain incomplete; RST releases remainder to0. Data ingress65: rejection1, canceled63, completion1; upstream1 |
| Actual connection cap | 128 keepalive control connections acknowledged; 129th closes. Releasing1 allows fresh metadata200 and upstream1 |
| Slow readers, debug | Connected24; active16, queued8, upstreamEOF before cancellation0; observed current/max delivery1,046,860 bytes; actual upstream16, canceled ingress24 |
| Slow readers, release | Same24/16/8/0; observed current/max delivery1,038,824 bytes; actual upstream16, canceled ingress24 |
| Finished producer ownership, separate library primitive | 20 production delivery receivers retained after producerEOF hold exactly1,310,720 bytes; dropping receivers returns0. No claim of20 complete HTTP workers |
| Actual ingress timeouts | Header10s/body30s observed; body timeout408 envelope and held bytes0; shortened internal overall deadline produces504 before body collection can finish |
| Existing queue/observer/stream regressions | Baseline retained, including fairness, 64 total waiting, unknown usage on overflow, raw SSE, RST drain/close and stop lifetime |

## Source-based reasoning and unverified areas

The conservative delivery payload bound is `(128 connection response lifetimes +16 active workers)×64KiB =9MiB`, derived from the production connection cap and Hyper1.8.1 HTTP/1 ownership. It intentionally overcounts overlap and includes EOF-completed receivers. It is not a measured process RSS cap. The socket slowreader observation above is pre-EOF; the EOF-retained case is a separate delivery-library test.

32MiB request payload charging, Vec spare capacity, error JSON parsing, copied observation prefixes, current transport chunks and kernel buffers are distinct. No allocator/process RSS measurement, native idle/peak RSS target, latency benchmark, Windows proof, actual provider/LLM execution or backend-compute cancellation was established. Task8 measurements and independent parent foreground probes are pending. Generic Claude errors and unsupported discriminator shapes remain raw forward without automatic replay.

## Identity and self-review

41 source files are listed in `task7/identity.json`; they remained unchanged through the final source audit and Pi runs. Filesystem fingerprints are not Git/build provenance proofs.

- Debug binary SHA256: `4fb6c03938774b8ca756490ea6c3f624bc5c2bf9e72cdf54c9fb9fb7421609f0`
- Release binary SHA256: `9be8dcf3cb880b41d6662a41eb91d25c27df07e7be6f23d3fae62c81b374b36d`

Product changes: new `src/admission/retry.rs`, `tests/retry_contract.rs`, `tests/resource_bounds.rs`; existing configuration/schema, admission Queue/Hold, worker stream, body status, metrics, server deadline and runtime contract. Existing direct-Config fixtures add only default-off selection; accepted Pi script/template/example remain unchanged.

Self-review checked deadline/cancellation ownership, no mutex/drop deadlock, original queue ages, one cleanup per logical worker with each attempt independently settled, bounded observation, exact payload preservation, extreme timing, permanent/unknown classification, independent connection/delivery bounds and sensitive-data logs. The already-large stream file remains one worker module (no new transport framework); this is a maintainability limitation to consider during review, not an unresolved functional failure. No commits, repository initialization, client configuration edits, service installs, model downloads or paid API requests occurred.

## SPEC correction supplement

The original186-test evidence above remains historical. SPEC review reopened the missing64-writer simultaneous follow-on case; the prior64-owner/single-writer test was insufficient coverage for that requirement. [Supplemental implementation and focused validation](task7-spec-fix/README.md) now records the separate barrier test, all64 writer outcomes, unchanged production sources and restored identical final binary hashes. No full-suite/Pi/transport rerun is claimed for this test-only correction; SPEC re-review is pending.

## QUALITY Q7-1 correction supplement

See [Task7 QUALITY correction](task7-quality-fix/README.md) and [current 41-file identity](task7-quality-fix/identity.json). The correction changes runtime pre-header 429 transport-error handling; final full suites were newly run with 188 passing tests per profile, followed by normal builds and final Pi positive/negative per profile. Earlier Task7 and SPEC-fix evidence remains historical and unchanged.
