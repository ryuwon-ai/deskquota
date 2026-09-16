# Respect explicit upstream retry directives

Status: A (explicit replay veto) is the approved implementation scope. B (global 503 pause) is rejected for this change because the current configuration does not establish a shared availability boundary. Independent specification review is next; product edits have not started. The user approved improving prior candidates and comparing actual task completion with retries. This change does not promise lower successful-request p95 or solve the original 38-second quota lower bound.

## Verified gap and established patterns

`scripts/probe-retry-directives.py` reuses the existing native retry fixture, isolated HOME/XDG state, normal release `e28f3322...`, two configured roots, unlimited RPM and unknown TPM. The original exploratory `evidence/retry-directives-2026-09-16/required-behavior-red.json` is preserved: it tested both A and the subsequently rejected B, so its 6 failed expectations are not six confirmed product defects. Four failures reproduce A; two demonstrate the unadopted 503 policy proposal. Its five controls pass with no harness errors:

- 503 with explicit 800ms timing, including malformed standard timing plus valid millisecond timing, exposes no shared cooldown. The next root starts immediately.
- Opted-in 429 replay ignores `x-should-retry: false`: two attempts occur instead of one for standard, timingless, conflicting true/false, and explicit-pause cases.
- Missing or malformed 503 timing and a 500 with explicit timing do not trigger a pause. `true` still retries a known transient error once and never makes the permanent error retryable.
- Every completed response retains its status, body, and listed headers. These are contract/order observations, not performance gains. The 503 fixture returns success on its next request, so it also illustrates the downside of treating an advisory hint as an instance-wide hold.

The reviewed scope has a separate native RED artifact, `evidence/retry-directives-2026-09-16/retry-veto-red.json`, produced with explicit `--expect retry-veto`: **four replay-veto failures and seven passing preservation controls**, 17 ingress requests and 22 upstream attempts, no harness errors, all owned workers stopped cleanly. The original exploratory artifact is unchanged.

Read-only source evidence:

- HiveMind `0468db5db9d0526e326b4db26971642301d7deed`, `src/hivemind/proxy/interceptor.py:418` and `src/hivemind/scheduler/rate_limiter.py:372-427`: response headers update the shared pause before error classification. Reuse the concept, not its float parser.
- LiteLLM reference checkout `9a715df212d777bbd43f4cab05731978c708ec63`, `litellm/router_utils/cooldown_handlers.py:205-255`: error-based deployment cooldown includes server errors; this is not evidence that every 503 always pauses all models.
- [OpenAI SDK](https://github.com/openai/openai-python/blob/main/src/openai/_base_client.py) and [Anthropic SDK](https://github.com/anthropics/anthropic-sdk-python/blob/main/src/anthropic/_base_client.py), retrieved 2026-09-16: the nonstandard exact `false` value overrides status-based retry. DeskQuota should adopt the negative instruction without adopting their broader retry surface.
- [RFC 9110 section 10.2.3](https://www.rfc-editor.org/rfc/rfc9110.html#name-retry-after): 503 Retry-After describes expected service unavailability to the client. It does not establish that unrelated models or credentials share the same outage scope.

## Minimum contract

### A. Explicit replay veto — approved scope

Add `retry::server_forbids_retry(&HeaderMap) -> bool`, using all `x-should-retry` field values. An exact lowercase `false` value after HTTP optional whitespace trimming vetoes internal replay. Multiple values containing false also veto; `true`, uppercase `FALSE`, absent, or unrecognized values do not expand current eligibility.

Use this helper in both the replay-only `needs_body` branch and the final replay decision in `transport/stream.rs`. Keep the missing-timing branch outside this veto: recognized timingless transient errors must still install the existing fallback cooldown. Never alter header-derived cooldown because retry permission and server availability are separate decisions. Preserve complete bounded JSON classification, duplicate discriminators, all encoding guards, one-extra-attempt cap, cancellation/deadline checks, quota debits, original body/usage, and no replay after downstream head or output.

If false makes a timed error skip probing, forward its original head/body immediately, with the already documented late-stream-error consequence. No new dependencies or settings.

### B. Explicit 503 shared pause — rejected for this change

The investigated proposal would move the existing header-derived delay block ahead of the 429-only body block and match statuses 429 or 503, reusing `header_delay` while leaving body classification/replay 429-only. **Do not implement this proposal.** All 503 and other 5xx paths remain unchanged, including original headers/body and no internal replay. In the adopted probe mode, absence of global 503 cooldown is an explicit preservation control, not a failure.

The current instance has one upstream and one shared ledger. The smallest implementation would pause all roots, models, and metadata admissions through that instance, although already admitted/started requests and cached responses would remain usable.

**Decision:** a per-model/credential 503 or pessimistic recovery hint can delay unrelated healthy requests. The RFC alone does not justify globally broad scope. The parent rejected B because worsening healthy-task latency conflicts with the motto. Reconsider only after an actual provider/instance availability contract is verified; do not invent model-level breaker maps or additional config to make this proposal appear complete. Forwarded 503 Retry-After remains available to each original client.

## Exact edit surface after review

- `product/src/admission/retry.rs`: one veto helper.
- `product/src/transport/stream.rs`: two helper calls, preserving the current 429-only timing block.
- `product/tests/retry_contract.rs`: reuse fixture helpers/manual clock for native contract checks.
- `product/docs/runtime-contract.md`: negative header semantics; explicitly retain existing 503 behavior.

No quota/scheduler/config/cache body changes are required.

## Verification before acceptance

Run the normal-binary probe with explicit `--expect retry-veto` and a new output path. This adopted mode requires 11/11: the four false cases must turn green, while 503 continues to stream without an instance-wide pause. The optional `--expect shared-pause-proposal` retains the historical exploratory expectation and is labeled an unadopted policy proposal in its output. Never interpret its two 503 failures as product defects or overwrite the original RED artifact.

Add targeted Rust checks before product changes: false blocks replay but preserves explicit and timingless cooldown; true cannot override permanent/partial/oversized exclusions; duplicate false veto; timed false head arrives before gated body but timingless classification remains gated. Preserve other-root 429 waiting and queue deadline/cancellation cleanup. Verify 503 is still original pass-through with no global pause or replay. Reuse current admission/manual-clock contracts rather than adding a retry framework. Full retry/stream suites must preserve two-debit replay accounting and no cancellation replay.
