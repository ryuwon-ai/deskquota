# Native company deployment revision

The user authorized these changes after testing on a company Windows PC: stable Windows builds without MSVC/Docker/WSL at runtime, remove the data-plane X-LLMGW-Token requirement, minimal exact response caching, default actual usage settlement, and configurable startup hold. This supersedes the older design's data-token/default-reserved/fixed-startup-hold contracts. No commit, push, release or real-provider call is included.

## Evidence and scope

The user's Windows build success with GNU/WinLibs and a throwaway creation-time/length patch, and cache hit57ms versus miss1.4–2.4s, are user-reported field observations. Compiler versions, cache policy and quota details have been requested without corporate URLs, keys or prompt text. They are not independently reproduced measurements.

Current code uses unstable Windows MetadataExt volume_serial_number/file_index in config_patch/storage.rs. Existing lifecycle/platform/windows.rs already uses the installed windows-sys handle APIs. Existing headers::prepare_request supports Auth::Forward, and existing SSE observers support actual settlement. Reuse them.

## 1. Stable Windows file identity

Use stable Win32 handle identity through the already-installed windows-sys dependency; isolate unsafe calls in the existing Windows platform module. A snapshot stores the identity read from its opened regular non-reparse file together with bytes and opened metadata. Subsequent replacement validation compares physical identity and attributes as well as bytes. Do not weaken identity to creation time and size. Preserve current permissions/ACL and recovery behavior.

Document that running a supplied executable requires no compiler. For source builds, explicitly describe the GNU target's MinGW compiler/assembler/dlltool prerequisites and toolchain selection without automatic global installation. Cross-compilation/checking is not native Windows execution acceptance.

## 2. Standard data-plane authentication

Remove the required custom data token and its generation, runtime fields, client profile injection and obsolete token-specific patch bookkeeping. Data requests remain numeric-loopback-bound; process locks retain ownership/duplicate-start duties. Standard Authorization and x-api-key are forwarded with Auth::Forward; explicit env/none modes retain their declared upstream behavior. Existing control token, authenticated lifecycle and origin checks stay separate from data requests.

Reject browser-origin data requests instead of relying on a custom authentication header. Require a loopback/localhost Host and an appropriate JSON content type for POST API data requests so browser forms or a rebound remote hostname cannot silently invoke a configured credential. Do not claim loopback or a process lock authenticates another local process. Local native clients on the PC are trusted; this is not a multi-user network gateway.

Client connect/disconnect keeps exact previews, ownership and reversible edits, but no local secret header is written into a client. Preserve actual client credentials and unrelated headers/settings. For forward-mode Pi/Codex custom profiles, write a standard API-key environment reference (default OPENAI_API_KEY, explicit --client-key-env override), never a placeholder sent upstream or a copied secret. Claude forward profiles keep its native API-key/auth-token configuration untouched. Env/none gateway modes may retain nonsecret client-availability placeholders only where the native client requires one; transport strips/replaces them according to the declared mode. Never opt into forwarding subscription/OAuth login credentials automatically. Preserve the doctor's honest inability to validate forwarded credentials when no client request exists. Remove obsolete code rather than adding a legacy token mode.

Claude forward connect requires an explicitly configured native API credential
source (API key/auth-token variable or configured apiKeyHelper); a saved OAuth
login alone is not sufficient. Inspect only configured presence, never execute
the helper or copy credential values. The installed client controls final
credential selection and API-key approval; do not certify that runtime behavior
from a profile-only test.

## 3. Actual settlement and startup hold

New/omitted accounting defaults to actual. Explicit reserved remains a supported provider-contract setting. Supported valid final SSE usage settles at successful EOF; missing/invalid usage preserves reservation, larger usage creates debt, and expired reservations never refund a fresh window. This is post-response correction, not exact pre-generation token counting. Keep a bounded pre-admission estimate while requests are in flight.

Add startup_hold_secs, default60, allowing0 to disable and finite nonnegative integer durations up to3600. It controls restart admission hold only, not the60-second rolling quota window. Unknown/unlimited-only quotas have no startup hold. Include it in setup input/preservation, preview, config snapshots and status meaning; use checked/saturating time arithmetic. A shorter hold accepts uncertainty from requests sent before restart.

## 4. Minimal exact cache

Reuse stdlib collections, existing SHA-256, Tokio/bytes and response streaming; no Redis, database, semantic embedding or new service. Keep cache optional and explicit in config/setup for repetitive classification/replay use. Pending user policy details: provisional TTL300seconds and history threshold3, based on the pinned Bifrost defaults. These numbers are configurable, not a performance claim.

Keys cover root identity, the entire final upstream URL including composed query, effective request headers including credential identity, and the entire original request bytes (including model and options). Do not normalize away tools, system prompts, sampling parameters or other semantics. No credentials/prompts in logs or persistent cache. Entries are memory-only, bounded by payload budget, per-entry limit, entry count and TTL; capture and active replay must retain budget ownership. No in-flight coalescing or cache of failures.

Only eligible short text generation requests are considered; history threshold is a capacity/eligibility filter, not proof of quality equivalence. Requests with tool execution, unsupported stateful references/media or request Cache-Control:no-store bypass caching. Successful responses with Cache-Control:no-store also must not be stored. Store only completely received, structurally valid successful JSON/SSE responses; partial streams, error events, truncation/oversize and transport failures never become hits. Live misses still stream incrementally. Hits do not spend upstream RPM/TPM or increment actual upstream-usage counters; cache counters are separate. Isolation tests must cover different roots, auth, final URL queries, models, bodies and generation options.

Exact-cache integration follows the Windows/auth/quota fixes and the concrete cache eligibility review. User answers can narrow the cache policy without blocking those independent fixes.

### Concrete cache implementation boundary

Use an optional `[cache]` table with `ttl_secs` (default300, range1–3600) and
`max_history` (default3, range1–64). An absent table disables caching. Keep a
fixed4MiB retained payload budget,256KiB entry/capture ceiling and128 entries.
The budget includes retained response headers and survives eviction while a
replay remains alive. These bounds are not a total process RSS guarantee.

Capture within the existing upstream streaming worker, using its EOF/error and
disconnect signals. Reserve capture memory without waiting; insufficient budget
means normal forwarding. Copy bytes before delivery instead of retaining the
delivery semaphore's byte owners. Abandon capture when draining or disconnected.
Reuse the SSE decoder, with cache completion validation distinct from usage
accounting: require normal text completion, no tool/length termination, no
meaningful events after terminal completion and no unfinished tail event at EOF.
Missing usage can retain the TPM reservation without invalidating a complete
text response. Nonstreaming JSON gets one bounded endpoint-specific validation.

Only status200 identity-encoded responses are reusable. Bypass no-store,
response no-cache, Vary:* and Set-Cookie; request no-cache bypasses lookup.
Responses API requests must explicitly set store=false and have no previous
response or conversation reference. Replay body bytes and essential content
headers, excluding stale rate-limit, retry, request-ID and date headers.
Hash input components with unambiguous lengths; sort header names while keeping
duplicate-value order. A request size ceiling bounds extra eligibility parsing.
Keep cache eligibility conservative for unknown generation fields: pass the
request upstream normally, but skip caching rather than assuming a new field
cannot request server-side tools or stateful work (for example web search).

## Acceptance

Native regression checks cover new defaults, custom hold0/short/default, standard-header data access, protected control API, browser-origin denial, client previews and restore ownership. Windows-specific identity tests distinguish equal-byte replacement files and retain reparse rejection; record cross-check versus actual Windows limits separately. Cache checks count actual upstream attempts on miss/hit, verify exact content/framing and bounded capture, expiry, isolation, no-store, failure/partial bypass and unchanged quota/usage accounting. Existing retry, cancel and lifecycle tests remain relevant.

Preserve earlier44-run competitor evidence under its previous source/binary hashes. New synthetic cache timings are reported separately with request counts, hit rate, p50/p95/p99 and resource bounds. No quality-free, universal TPM-compliance or low-end superiority claim follows from these functional checks.
