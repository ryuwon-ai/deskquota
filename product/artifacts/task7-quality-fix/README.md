# Task7 QUALITY correction Q7-1 — DONE / HOLD

This supplement preserves the original Task7 186-test and SPEC-fix artifacts. The current product has 41 authoritative source files. Only `src/transport/stream.rs`, `tests/retry_contract.rs`, and `docs/runtime-contract.md` changed from the SPEC-fix HOLD. The accepted 64-writer barrier regression is unchanged. No dependency/config/Pi script/template change.

## Correction and directly verified boundary

A transport failure already detected while probing a small HTTP429 body now returns through existing `HeadError::Upstream`, yielding protocol HTTP502 `upstream_transport_error` before downstream headers. `RetryPrefix.extra` now only represents a successfully read overflow chunk. No fallback, knob or additional replay path was added.

The new real-socket test gates peer EOF after a short 429 body and confirms no downstream bytes before gate release, then an actual HTTP502 JSON envelope afterward. Both incomplete Content-Length and incomplete chunked framing are tested under Reserved and Actual accounting. Known RPM and TPM are used with the existing ManualClock (startup advanced to 60s): the 5s group cooldown still blocks a fresh other-root metadata client at t=64s; at t=65s it completes with HTTP200. This is real Reqwest/socket response behavior with virtual admission time, not a wall-clock latency benchmark.

Each variant directly checks one cleanup after failure, zero active/held resources, released request-body bytes, one upstream-error terminal and zero response-EOF terminals. Original POST RPM and uncertain TPM debit remain; after fresh metadata succeeds, starts/cleanups are exactly two, active/held zero and original TPM debit remains. Complete 429/raw forwarding, encoded/oversized exclusions and post-head failures retain the existing paths; the complete locked regression suites passed. The narrow source diff, not this new test alone, establishes that excluded response paths were unchanged.

## Preserved RED and intermediate failure

- `red.log`: behavioral failure on baseline runtime: one POST ingress / one actual upstream; downstream received 0 bytes instead of HTTP502. The assertion stopped before the fresh client or remaining variants.
- `green.log`: **FAILED**, despite its provisional filename. The first Content-Length variant passed (2 ingress/2 upstream); a hardcoded incorrect chunk length in the new fixture then caused premature protocol failure in the chunked variant (1 ingress/1 upstream). This was a fixture defect; the intended incomplete chunked EOF now calculates its valid chunk size and omits only the terminal zero chunk.
- `green-corrected-fixture.log`: all four variants passed (8 data ingress/8 upstream, 4 HTTP502/4 HTTP200, zero replay). Final terminal metric assertions were then added and verified by both full suites below.

## Final verification

| Check | debug | release |
|---|---:|---:|
| Locked all-target Rust tests | 188 passed / 0 failed | 188 passed / 0 failed |
| Q7-1 variants within full suite | 4 | 4 |
| Q7-1 data ingress / actual upstream | 8 / 8 | 8 / 8 |
| Q7-1 HTTP502 / HTTP200 / replay | 4 / 4 / 0 | 4 / 4 / 0 |
| Pi positive completion / read-tool wire attempts | 1 / 2 | 1 / 2 |
| Pi gateway-off completion / read-tool wire attempts | 0 / 0 | 0 / 0 |

Final fmt, locked all-target check, and clippy `-D warnings` passed. Normal locked debug/release builds passed after integration tests. Installed Pi positive and gateway-off negative were each run once per final built binary; positives exited 0, negatives exited expected 1 with assistant failures and zero wire attempts. Both binaries remained byte-identical throughout these Pi runs. No later Cargo command ran.

Each profile's unchanged barrier test again observed 64 arrivals, 64 attempted/successful follow-on writes, 63 original successes, one protocol memory rejection, one fresh success, 65 data ingress/64 upstream, zero cancel/unexpected/pending, and owned payload 33,554,432 bytes down to zero. This is byte-budget/socket evidence. Slow-reader observations are retained in `identity.json`; no new RSS, Windows, provider, native latency or performance claims are made. The full 188-test suite's total HTTP denominator is not aggregated; the above denominators apply to named Q7-1/Pi/barrier cases only. Control/status requests are excluded from data ingress counts.

## Identity and review handoff

`identity.json` lists all 41 source hashes and per-profile evidence. `held-source.tar.gz` contains those 41 files only, re-read and hash-verified after creation. Existing research archives/manifests/review failures were preserved. Artifact log scan found zero synthetic prompt/credential/token sentinel hits. Commands and expected exits are in `commands.md`, `build-command-results.json`, and `pi-command-results.json`.

Final binary SHA256:

- debug: `ec3742d6eb0acd618cc1e1ce9d0ca1581e5a17d286a727b9c3267eeef276f2fc`
- release: `48a9232ae5ffa012ddd4caeae90ee6f2e6094755b0815c68d472e7b69a8316d4`

Self-review confirmed the pre-head error is returned before `head.send(Ok(...))`, cooldown is applied before probing, no retry follows the error, and the existing guard remains the sole cleanup owner. Independent native probing and the same QUALITY reviewer's Q7-1 re-review remain pending; this implementer result does not grant QUALITY PASS or authorize Task8. Source and both binaries are now HOLD.
