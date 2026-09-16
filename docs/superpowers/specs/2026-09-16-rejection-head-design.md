# Stream 429 responses when classification cannot affect a decision

Status: bounded fix authorized by the user's request to improve newly verified wasted work. A gated normal-release baseline and its negative expectation reproduce the issue. No retry-policy expansion or 503 behavior change.

## Evidence and choice

`scripts/probe-rejection-head.py` observes upstream head received with body completion gated. Baseline `fec36cfa...` delays the downstream head for JSON+retry off+Retry-After and non-JSON with retry off/on. The three cases fail the desired `stream-ineligible` expectation while required-classification controls pass. This is an ordering check, not a p95 benchmark. Evidence: `evidence/quota-efficiency-audit-2026-09-16/rejection-head-baseline-02.json`.

Unconditionally skipping body classification when retry is disabled is wrong: timingless recognized transient JSON still sets the current fallback shared cooldown. Reading all 429 bodies creates avoidable waiting. Adopt conditional classification using the existing decision rules.

## Minimal contract

- Apply `header_delay` exactly as today, before any body decision. Duplicate/overflow/malformed timing semantics remain unchanged.
- Extract the current content-type/encoding eligibility checks from `retry::transient` into one shared helper. Require at least one content-type and every value to be application/json ignoring case and parameters; every encoding must be identity ignoring case. Preserve transient's full bounded JSON/discriminator checks.
- A body is needed only when timing headers are absent, or a further internal retry can still be considered (`retry_transient_429 && !retried && !disconnected && timing_allows_retry(headers)`). The body probe additionally skips unclassifiable representations and retains its existing declared-size/16KiB limit and exact lowercase `identity` encoding guard. The existing probe is stricter than the transient classifier for uppercase encoding; this optimization must not widen the set of replayed requests. Include uppercase-encoding and mixed content-type negative controls.
- When skipped, keep an empty incomplete RetryPrefix and forward the original response head and streaming body through existing bounded delivery. No extra retry, cooldown, usage, queue, or cache policy is introduced.
- Bodies still needed for retry or missing-timing classification keep the existing bounded pre-head probe. Error/cancellation/deadline behavior remains unchanged on that path.
- An excluded response can now expose its original 429 headers before body EOF; subsequent truncation is a stream error after 429 rather than a replacement pre-head 502. This matches ordinary streaming pass-through and must be documented/tested.
- No new dependencies/configuration, no token/body/header rewriting, no unrelated refactor. Engine computation and provider limits are unchanged.

## Verification

Reuse the independently written normal-binary probe: desired expectation must turn all six scenarios green. Add minimal Rust gated regressions including non-JSON and retry-off+timing plus missing-timing/retry-on controls; existing retry classification, encoding/size, multi-header and malformed cases remain required. Test cancellation/EOF ownership through existing stream suites and verify one upstream attempt on skipped paths. Record RED/GREEN, exact final source/binary hashes and exclusions. All calls use private loopback synthetic fixtures.
