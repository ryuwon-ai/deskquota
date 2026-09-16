# `llmgw` runtime contract

This package implements typed configuration and a native loopback HTTP gateway
with bounded streaming and worker-owned upstream lifetimes. Data requests use
standard upstream authentication; authenticated control remains separate and
available during shutdown. Actual usage settlement is the default, and startup
admission hold is configurable independently of the quota window.
Native Tasks 1–6 add lifecycle control, first-run setup, explicit upstream
transport settings, bounded doctor network checks, reversible client profiles,
user-login autostart registration, and checksum-verified thin installers. Model
management remains separate work.

## Native lifecycle and setup (Tasks 1–6)

The global `--config PATH` is accepted before or after every subcommand. A
relative path is resolved once from the invocation directory. With no explicit
path, the standard config is `%APPDATA%\\llmgw\\config.toml` on Windows,
`~/Library/Application Support/llmgw/config.toml` on macOS, and
`${XDG_CONFIG_HOME:-~/.config}/llmgw/config.toml` on other Unix systems.

`llmgw` with no subcommand starts setup when the resolved config is absent and
runs `on` when it exists. `llmgw setup` reconfigures an existing file using its
current values as defaults. `on`, `off`, `restart`, `status [--json]`, `run`, and
`doctor [--json]` remain available. First-run setup requires terminal input. A
pipe or other non-terminal first run exits 2 immediately, creates nothing, and
prints `llmgw setup` plus the resolved configuration path. It does not refer to
repository-only fixtures or documents that are absent from an installation. Normal
or idempotent operations exit 0, operational failures exit 1, invalid arguments,
invalid configuration, and unconfigured non-terminal use exit 2, and an explicit
wizard cancel exits 130. Ctrl-C while Dialoguer is reading an input follows the terminal's SIGINT
path, whose shell status is also 130.

### Setup boundary

The wizard uses Dialoguer 0.12.0 and creates its terminal adapter only for the
interactive setup command. Each step has a Back/Cancel action. No config, state,
client, or login file is written before final Apply. The environment choice is
only a set of defaults: it does not start or download a local model and does not
infer a verified provider protocol. The connection step requires the user to
select the Completions, Responses, or Messages formats the upstream actually
supports. Existing Models and Messages count_tokens routes remain selected when
generation choices are accepted unchanged, including a valid metadata-only
route. The gateway performs no API format translation.

Upstream authentication can forward the incoming client header, read a complete
header value from a named environment variable, or send no auth. The environment
mode neither displays the value nor invents a `Bearer` prefix. Availability in
the current shell and availability after the next login are separate facts.
Setup records login intent in its pending metadata. After the core config is
saved, it separately previews the exact OS target, executable, config, argv,
prior registration state, manager command, and preview hash. Registration needs
its own confirmation and never copies a secret or auth environment value.
Forward mode cannot be exercised by doctor because doctor
has no incoming client credential.

Corporate setup may add an explicit HTTP(S) proxy without embedded credentials
and a PEM CA bundle. Environment proxy discovery remains disabled. Explicit
proxy use always bypasses localhost and loopback addresses. The client merges
the explicit CA bundle with normal system roots, keeps TLS verification enabled,
disables redirects and retries, and is built once when a worker starts. It reads
the CA file once while building that client. No CA is installed into the OS.

Model listing is a separate, optional, bounded 10-second `GET` to the designated
upstream's `/models` path. Responses are limited to 64 KiB and 256 nonempty IDs.
A 404, malformed response, empty listing, missing auth, or network failure is
shown as unavailable and manual model entry remains possible. Listing success
does not verify generation, tools, vision, reasoning, prices, or any other model
capability. Setup performs no catalog, pricing, telemetry, update, or inference
request automatically.

RPM and TPM are each explicitly known with a nonzero number, unknown, or
unlimited. Unknown is not treated as unlimited. Setup separately records whether
other PCs share the contract and whether the provider describes combined or
separate input/output limits; contracts that the gateway cannot model are shown
as estimated. Concurrency defaults to 1 and is bounded to 1–16. Existing
accounting, retry, cancel, multi-root, and multi-model settings are preserved
during reconfiguration unless the user explicitly replaces them.

The default build uses `utf8_bytes` and skips tokenizer questions. With the
explicit Cargo `bpe` feature, the model step also offers `cl100k_base` and
`o200k_base` input estimates and asks for a framing allowance only for BPE.
It explains the JSON estimate and vocabulary memory cost. Keeping the same
model ID preserves its selected estimator and allowance; a renamed model starts
with byte defaults unless explicitly changed. Editing the first model preserves
additional model settings, route memberships and TOML comments. Renaming updates
references to that model so existing routes remain valid.

The optional model `max_output_tokens` value collected by setup is a
user-supplied reservation fallback for local accounting when a request omits its
own cap. It is not a verified provider limit. The gateway does not insert it
into the body or enforce it upstream. Without a fallback, request-provided caps
remain valid; known-TPM generation admission requires such a request cap. The
summary keeps listing verification and capability verification separate and
leaves unknown capabilities unverified.

Setup listens on loopback port 4141 by default. Setup stores the selected client
names, login request, and shared/separate quota answers
in a private `.<config-name>.setup-pending.json` sidecar so a rerun preserves
them. The sidecar contains no credential value or client before-image and does
not affect the worker config fingerprint. The final
preview shows absolute config/state paths, old and normalized new upstream bases,
explicit proxy/CA behavior, route URLs, protocol/model visibility, estimated
quota facts, the auth environment name without its value, fingerprint impact,
and pending client/login intent. Core config, each client profile, and OS
registration remain separately previewed and applied resources; failure or
cancellation of one does not imply another succeeded. Root IDs use only ASCII letters, digits, `-`, and
`_`; for example `pi-work` is valid and a root containing a space is rejected.
OpenAI-style clients use `http://127.0.0.1:4141/r/pi-work/v1`. A Messages client
uses `http://127.0.0.1:4141/r/pi-work` and appends `/v1/messages` exactly once.
Sessions sharing that configured root share one root budget.

Final apply writes the config and pending sidecar and initializes the protected,
the config-scoped control token file. Preview and cancellation create no
state. Save only starts no worker, changes no client file or registration, and
preserves any running worker and every existing token byte. A changed config
then remains pending for an explicit restart. Before the
final choice, setup compares the desired saved-config fingerprint with the
authenticated running-worker fingerprint. This also detects a Save-only or
external edit that predates the current wizard. A matching worker uses
authenticated idempotent `on` and keeps its PID, nonce, and start identity. A
different worker is disclosed before selection; Save and start then explicitly
restarts, cancels queued requests, drains active requests for up to ten seconds,
and succeeds only when authenticated readiness reports the desired fingerprint.
If the worker fingerprint cannot be authenticated, Save and start refuses
before writing or restarting while Save only remains non-disruptive. Setup
rechecks the typed decision during apply, so a newly discovered mismatch whose
restart impact was not previewed is also refused. Readiness is separate from the
known-quota admission hold, which defaults to 60 seconds and is configurable.

New config directories are created privately on Unix and new config and pending
files use mode 0600. Final apply takes a persistent private
`.<config-name>.setup.lock`, then compares the config and pending sidecar with
the exact bytes loaded for the preview. A cooperating second writer is rejected,
and a detected external edit requires a fresh setup preview. Changed files are
fully written and synced in the destination directory before publication. On
macOS and Linux, interruption before the final rename leaves the existing
destination intact. The last snapshot/path check and atomic rename are adjacent,
but do not claim an external-editor filesystem CAS primitive.

Existing regular non-symlink files are updated without silently changing their
owner, group, permissions, or ACL. macOS copies and verifies the extended ACL.
On Windows, setup validates the current-user-only protected ACL, then retains
protected copies of both the reviewed original and replacement candidate around
`ReplaceFileW`. It removes those copies only after the destination is verified;
an ambiguous failure reports both recovery paths without their contents.
Linux uses native xattr inspection and refuses a source or inherited POSIX ACL
that this setup path cannot preserve; it does not add a libacl dependency. Other
Unix systems explicitly refuse changed existing files because this package has
no verified ACL-preserving replacement there. A no-op reconfiguration preserves
the exact config source bytes, including comments, and edited scalar values
preserve TOML decor and inline comments. Setup refuses a link or non-regular
config or pending target.

### Doctor boundary

Plain `doctor` is fully offline. It validates config and reports the resolved
state path, local port availability, TLS/CA availability, auth-reference
availability, and that worker readiness was not checked. `doctor --network`
performs exactly one bounded TLS/`GET /models` check and no generation.
`doctor --inference --model ID --max-output-tokens N` performs exactly one
bounded generation request for that configured model and does not list models.
Non-2xx, missing auth, unsupported Forward auth, missing CA, timeout, and response
size failures remain distinct operational failures. Outputs name environment
references but never print their values.

Plain doctor and one-shot CLI status query autostart registration outside the
lifecycle readiness loop. They report registration, current authenticated
runtime, and next-login auth availability as separate fields. A missing user
manager or policy failure is unknown or unavailable with its reason, never
silently treated as disabled or healthy. The current shell environment is never
evidence that an environment credential will exist at the next login. Doctor
also reports a missing or moved executable recorded in an owned registration.

### User-login autostart boundary

`llmgw autostart on` and `off` first print an exact config-scoped plan. A
noninteractive apply requires `--apply-hash` from that fresh plan; an interactive
terminal can confirm it directly. The hash includes the absolute executable and
canonical config paths, rendered definition, target, action, and inspected prior
registration. A target changed after preview or an unowned same-label target is
refused. Neither action starts, stops, restarts, or otherwise probes the worker.

macOS installs or removes only
`~/Library/LaunchAgents/io.llmgw.gateway.<config-hash>.plist`. The plist uses
literal `ProgramArguments` for `llmgw --config ABS run`, `RunAtLoad=true`, and
`KeepAlive=false`; llmgw never calls launchctl bootstrap or bootout. Linux uses
the matching user systemd unit with a literal absolute `ExecStart`,
`Restart=no`, and `WantedBy=default.target`; it runs `systemctl --user
enable/disable` without `--now` and only discloses existing linger state. Windows
uses a current-user LogonTrigger task with `InteractiveToken`, `LeastPrivilege`,
`IgnoreNew`, unlimited execution time, continuous-service battery/idle/network
conditions, no restart policy, and the executable plus quoted argv. Registration
never calls Task Scheduler Run, and removal never calls End. The task file is
written as UTF-16LE with a BOM. Readback honors the scheduler's omitted true
Enabled defaults and resolves exported account names to SIDs through Windows
before checking ownership. File/path-not-found HRESULTs mean unregistered;
access errors remain unknown and block mutation.

Linux registration status comes from one `systemctl --user show --all` query
bound into the reviewed plan. The local fragment is accepted only when
`FragmentPath` is exact, `DropInPaths` is empty, and `NeedDaemonReload=no`;
otherwise the registration is foreign or unknown and mutation is refused. The
reported executable is derived from that checked local fragment. Removal
disables without `--now`, unlinks the owned fragment, runs `daemon-reload`, and
re-observes once. A surviving global or foreign unit is reported and left
untouched; none of these steps stops the current worker.

The OS manager directly owns foreground `run`; there is no shell wrapper,
watcher, alternate supervisor, privilege escalation, Windows Startup/VBS
fallback, or Linux linger change. Environment-auth configs report next-login
availability as `unknown` until that login environment is observed. The
empty-login fixture separately reports `configured_but_unavailable` only after
its run fails for the missing environment reference. No token or shell
environment is copied into registration.

`scripts/verify_native.py --templates-only` performs render contracts only on
Linux and Windows and never dispatches `systemctl` or `schtasks`. On macOS it
also installs and removes a plist under an owned temporary home and runs the
read-only `plutil` validator; it never calls `launchctl`. These fixtures do not
prove an actual login. Windows executable or config paths containing `%` are
refused before planning or mutation until literal Task Scheduler delivery is
verified on Windows. The staged `--exercise-user-service` path prints exact
resources before requiring explicit execution in a temporary OS account and
requires a human login timestamp plus explicit login attestation for login
evidence.

Doctor JSON reports `state_directory` as the machine-readable fixture boundary
and creates no runtime state. No mandatory Python, Node.js, Redis, Docker, or WSL
runtime is introduced. The owned PTY and network fixture drivers use Python only
as test tooling. Interactive and synthetic loopback runtime verification for
Task 2 was performed on macOS. The 2026-09-16 Windows 10 x64 run passed
404 native tests. `scripts/verify_windows_autostart.py` additionally registered
a real temporary task, manually ran it through Task Scheduler, checked a
non-elevated worker token and owned registration, then stopped the worker and
removed the task. Linux and an actual login cycle remain unverified. See the
[Windows follow-up](../../reports/windows-followup-and-improvements-2026-09-16.md).

The canonical config path's SHA256 selects a sibling `.llmgw-<64 hex hash>`
directory. Config content SHA256 separately identifies the immutable running
configuration. There is no old `.llmgw` fallback or migration. Off/status resolve
the canonical path without parsing TOML, so an invalid edit or changed listen
port does not hide a worker. Deleted configs and broken symlinks fail explicitly.
Restart acquires that instance's operation lock before reading the current
canonical configuration and validating credentials, then stops the existing
worker. Edits made while waiting for the lock are therefore validated before
disruption. If loading resolves to a different canonical path, restart rejects
it before stopping either instance. This does not make later external edits atomic with drain/spawn;
the child still rejects a changed fingerprint.

The worker retains the same `worker.lock` file object for its entire lifetime;
lock paths are never unlinked. A separate operation lock serializes on/off/restart
with a bounded 17-second busy wait. On confirms readiness within five seconds
of spawn through authenticated health, checking path hash, immutable fingerprint,
nonce, bound address and start timestamp. Readiness is independent of the
configured known-quota startup hold and shared cooldown, both reported even with
an empty queue. Changed bytes set `pending_restart`; on checks the authenticated
running identity before parsing saved edits and returns exit 1 with
`restart_required`, including for malformed TOML. New/stopped configuration is
validated before token creation. Off uses the recorded address, validates the identity, and
sends a nonce-conditional stop. Failed identity checks never signal a PID.

Off allows ten seconds for drain and up to two seconds for confirmation, then
reports a failed/unknown outcome rather than killing a process. Authenticated
control remains queryable while draining; new data ingress gets 503 and does
not reach upstream. Drain control connections share the existing 128-connection
budget, close after one response, and cannot extend the drain deadline. Existing
request/upstream/body cleanup remains required before the lifetime lock releases.
Ctrl-C and Unix SIGTERM use the same shutdown path. Stopped status is exit 0;
failed inspection is exit 1, with `state=failed` in JSON. Stale records are
reported separately from whether a worker lock is held.

Manual `on` launches the same executable with argv and disconnected stdio. Unix calls
setsid before creating Tokio threads; Windows uses DETACHED_PROCESS and
CREATE_NEW_PROCESS_GROUP. Before spawning, inherited caller stdio handles are
marked non-inheritable so captured `on` output reaches EOF while the worker
continues. There is no watchdog, periodic worker, shell command,
or automatic model start. `worker.log` holds one fixed bounded
sanitized diagnostic event, not request or provider data. Startup outcomes come
from the new child's exit status, never a possibly stale log. An internal hidden
worker exit code distinguishes a bind conflict; public command failures remain 1.
Tokens never appear in argv, runtime JSON, OS registration, or output. Manual
off preserves client URLs and autostart registration.

State protection is checked before token writes and existing permissions are
never automatically modified. Unix requires current UID, private mode bits,
regular non-symlink files, and no-follow/nonblocking opens. macOS additionally
uses exacl 0.13.0's no-follow ACL API and rejects all extended ACLs, including
inherited entries. Its path lookup is checked against the opened object's
dev/inode; this is not an atomic defense against a hostile same-user editor
changing ACLs/paths between checks. Linux POSIX ACL masks correspond to group
mode bits; mode 0600/0700 also restricts inherited ACLs. Other filesystem ACL
semantics and Linux runtime are not verified. exacl is macOS-only and introduces
no Linux libacl runtime requirement.

Windows uses windows-sys 0.61.2 to create a protected current-user-only DACL,
then verifies the opened object's SID, ACL and disk/non-reparse type before
returning a writable file. Null/unsupported ACLs and any non-owner grants are
rejected. Missing state ancestors are created with the same protection, rather
than created with inherited grants and then repaired. Windows replacement
syncs use writable handles. Native Windows tests now cover these paths.

Unsafe Win32 calls are confined to the storage/process protection module and
the Windows disconnect monitor; other product source retains `deny`. On
Windows, Mio 1.2.3 does not classify a TCP reset as ERROR readiness. The monitor
uses Winsock FD_CLOSE and a Windows thread-pool wait to distinguish reset from
graceful send-half-close without periodic polling or one thread per connection.
Cancellation disarms the wait and joins its callback before freeing the event
or callback context. The existing 128-connection cap bounds these registrations.

No actual LLM, current-account login registration, human login cycle, or release
was exercised by Native Task 5. Owned temporary-home macOS registration fixtures
and plist validation are distinct from actual login evidence. The subsequent
2026-09-16 Windows check verified task registration, manual scheduler execution,
non-elevated worker identity, and removal. Linux registration and a real login
event remain unverified. See `artifacts/native-task5/` for the historical scope
and the linked Windows follow-up for the newer evidence.

## Local quota admission

All five data endpoints use one joint concurrency/RPM/TPM ledger. The ledger
accepts explicit monotonic elapsed timestamps; the HTTP coordinator supplies
Tokio monotonic time in production. `known` is a nonzero local budget,
`unknown` means unenforced and unknown, and `unlimited` is an explicit setting.
Unknown is never evidence that the provider allows unlimited usage. Other
machines' consumption and the provider's own window or accounting remain unknown.

Setup labels known values as local rolling 60-second caps. Choosing unknown
defers that quota's enforcement to the upstream and marks its limit unverified;
it leaves concurrency, fairness, bounded queues and shared 429 cooldown active.
A provider's advertised RPM number alone does not establish a rolling-window
contract. A local cap is useful as an additional user-chosen budget, but can add
waiting when the upstream refills capacity differently. No provider policy is
detected automatically, and setup does not relax saved limits.

If either quota is known, process startup uses `startup_hold_secs` (default 60,
range 0–3600). Set 0 to disable this admission hold. A shorter hold accepts
uncertainty about upstream requests sent before restart; it does not recover a
provider balance. Unknown/unlimited-only quotas have no startup hold. Restart
never replays in-flight work. Each subsequent
window is a local rolling 60 seconds, with no accumulated token-bucket burst
following elapsed time or resume. OS-specific suspend-clock behavior has not
been measured. The deterministic library seam advances the same ledger's
monotonic time; startup hold configuration does not change the quota window.

A waiting request holds no execution slot or quota. Admission obtains all
resources together. Its provisional RPM/TPM hold has no time expiry until the
worker starts or cancels, even if the admitted worker has not yet been polled.
The first poll of the upstream send records RPM 1 and starts the committed
RPM/TPM window. Start wakes waiters so they learn the new expiration. RPM is
never refunded before its expiration, including an upstream failure or close.
Prestart cancellation releases the joint hold exactly once and starts no HTTP
attempt. Worker RAII cleanup still occurs after local HTTP futures/bodies drop.
The ledger serializes mutations under one short synchronous mutex and waits on
notifications or the next relevant expiration, not polling. Existing stop,
disconnect, 120-second queue and absolute request deadline branches stay active.
Root scheduling and the explicit total 64-request waiting queue are described below.

## Root admission queue (Task 6)

The coordinator owns one queue and its single concurrency/RPM/TPM ledger. Every
configured route maps to a bounded root index (maximum 16 unique 1–64-byte safe
ASCII identifiers); the public server validates these identifiers even for a
programmatically constructed Config. Empty roots remain control-only. Session
headers never create roots or additional shares. A root and all its children
share FIFO order; independent sessions using that route share the same FIFO.

Each selection inspects valid root heads once and chooses in root round-robin
order. Only an actual joint admission moves the cursor. A candidate check takes
no reservation. All endpoints, including GET models and POST count_tokens,
share the total **64 waiting requests** ceiling; admitted holds are distinct.
Overflow returns bounded HTTP 429 `gateway_queue_full` with no upstream attempt.
Unknown routes, invalid payloads and impossible estimated costs are rejected
before queue insertion.

A resource-blocked head gains one bypass only when another request is actually
admitted. Scans, wakes, failed fits and ordinary RR turns between currently fit
heads do not count. Once any head reaches eight such bypasses or five seconds of
its original wait age, the oldest valid head across all roots becomes a barrier.
Eligibility means its cost fits total configured capacity, not current remaining
budget. Other admissions pause until the selected head enters or is removed by
cancellation/deadline. The barrier stays attached to the chosen ticket; canceling
some other triggering head does not remove it. Queued impossible costs cannot
create a barrier because capacities are immutable and insertion rejects them.

The scheduler wakes on enqueue/cancel/admission/start/completion notifications,
the next quota expiration, head age threshold, or earliest waiting deadline.
It does not poll. Queue expiry is at most 120 seconds and remains bounded by the
existing absolute request deadline. Waiting entries retain original enqueue age;
the admitted worker's Hold also retains that timestamp and original queue
deadline. Retry reentry retains both original values. Cancellation of an admitted but
unpolled waiter releases the provisional reservation once without starting HTTP.
An already-running long request at concurrency one remains nonpreemptible.
These initial policy constants give no universal bounded-wait theorem under
sustained overload, external quota use, or long-running work.

Authenticated `GET /_llmgw/status` preserves existing counters and adds a typed
`admission` object:

| Field | Meaning |
|---|---|
| `queue_length`, `roots[].queue_length` | Total/per-registered-root waiting tickets, excluding admitted holds |
| `active` | Joint reservations, including admitted workers before first HTTP poll; existing top-level `active` counts worker lifetimes |
| `blocked_reason`, `barrier_root` | Current queue blocking cause and selected protected root, or null |
| `estimate_mode` | Known TPM: `json_utf8_bytes_plus_output_reservation` when all models use bytes, otherwise `model_json_estimate_plus_output_reservation`; unknown/unlimited: `tpm_unenforced` |
| `model_estimators` | Configured model IDs, `input_estimator` selectors and `input_token_overhead` allowances; active only for known TPM, never provider-exact counts |
| `rpm_mode`, `tpm_mode`, `accounting` | Explicit known/unknown/unlimited and reserved/actual settings |
| `rpm_capacity`, `tpm_capacity` | Known local capacity as a decimal string, otherwise null |
| `rpm_debited`, `tpm_debited`, `tpm_held` | Exact local ledger sums as decimal strings, preserving values above u64 |
| `reservation` | Cumulative reservation-versus-observed-usage diagnostics for finished, started generation attempts under known TPM |

Ledger sums being exact does not make the HTTP token estimator exact or prove
provider compliance. Root labels come only from validated configuration; status
contains no request bodies, prompts, auth values, or per-request/session labels.
Existing control-token and Origin restrictions remain in force. The native
identity-aware `status --json` CLI exposes the same fields. Ordinary `status`
also shows the representative queue reason, protected root, local quota modes,
capacities/debits/holds, accounting and estimate mode. A representative reason
is not each request's exact cause or an ETA. Unknown/unlimited capacities appear
as `n/a`, not zero remaining capacity; local ledger sums are not provider balances.
With queued requests, shared cooldown takes precedence. A protected root otherwise
reports its head's actual ledger blocker, such as TPM, rather than a generic
starvation-barrier label. An empty queue has no blocking cause, even while the
separate cooldown duration remains positive. These fields do not change scheduling.

`reservation.samples` counts finished upstream attempts with known usage;
`reservation.unknown` counts those without it. Retries are distinct attempts.
Known samples contribute to `reserved_tokens`, `observed_tokens`, `excess_tokens`
and `shortfall_tokens`; the last two sum each attempt's positive difference in
the corresponding direction. Totals saturate at u128 and are serialized as
decimal strings. Valid zero usage is known; malformed or overflowing usage is
unknown. Metadata, unstarted cancellations, cache hits and unmetered TPM are
excluded. Both actual and reserved accounting, including expired reservations,
record observations without changing debit rules. These are original reservations
versus final usage, not tokenizer accuracy, refunds, saved tokens or provider
remaining capacity. Counters reset with the worker.

The Task 6 exact-cost fairness traces use the same Admission/Queue/Ledger library
as HTTP, with a manual/paused monotonic clock and `exact_fixture` costs. Actual
socket tests use real HTTP estimation/metadata policy and never test-cost headers.
They are separate evidence classes, not a performance benchmark.

## Token estimation and output bounds

Known TPM uses each model's selected **input estimate** plus the unchanged
output reservation. `input_estimator = "utf8_bytes"` is the default and counts
the complete original JSON UTF-8 bytes with `input_token_overhead = 0`.
In a `--features bpe` build, explicit `cl100k_base` and `o200k_base` selections count that same serialized JSON
with the selected BPE encoding and add `input_token_overhead` (default 32;
explicit zero allowed). Only byte mode requires zero overhead. Unknown selectors
and invalid or negative allowances are configuration errors.

The default build omits the optional `bpe-openai` dependency and its vocabularies.
Serialized estimator names remain recognizable so explicit BPE settings produce
an actionable missing-build-capability error, including with unknown/unlimited
TPM. Config parsing, typed startup and setup persistence reject them before
listening or replacing user configuration. There is no byte fallback, runtime
download or vocabulary sidecar. Existing worker `status`/`off` do not parse the
saved estimator; a rejected restart leaves that worker running.

```toml
[[models]]
id = "your-model-id"
input_estimator = "cl100k_base"
input_token_overhead = 32
# max_output_tokens is an optional output reservation fallback, unchanged.
```

These are not exact provider prompt counts or guaranteed upper bounds. JSON
includes non-input fields and cannot reveal server templates or hidden tokens.
Users must match the encoding and framing allowance to the upstream; protocol
compatibility or an OpenAI-compatible URL does not identify a tokenizer. There
is no automatic model-name inference, adaptive ratio, or byte-count clamp.
Literal special-looking strings are ordinary text; the JSON is not normalized.
Accepted bodies are forwarded unchanged. No cap, usage flag or extra
count-token call is inserted.

When enabled, the pinned native `bpe-openai` library bundles its vocabularies in the executable;
there is no runtime download. Gateway construction prewarms only the selected
encodings when TPM is known, before opening the listener, and requests share
those instances. Config parsing, setup preview, doctor and status clients do not
initialize vocabularies. Metadata and unknown/unlimited TPM skip counting.
On the measured macOS gateway, idle RSS was about 9.9 MiB in byte mode,
41.7 MiB with cl100k and 76.7 MiB with o200k. Earlier mandatory-BPE builds grew from 10.19 to
59.87 MB because both vocabularies were bundled, even when unused. Unknown TPM
with a BPE selection stayed at about 9.9 MiB idle RSS. These are startup samples,
not peak memory or low-end hardware guarantees. See the
[paired results and resource costs](../../reports/input-estimation-results-2026-09-16.md).
Historical byte-mode measurements below describe their old binary.

The following positive integer request fields take precedence over the configured
model `max_output_tokens` default:

| Endpoint | Explicit output reservation |
|---|---|
| Chat Completions | `max_completion_tokens` or `max_tokens`; sending both is ambiguous and rejected |
| Responses | `max_output_tokens` |
| Messages | `max_tokens` |
| Models / Messages count_tokens | No generation reservation, no output bound required; RPM remains 1 |

Models and count_tokens retain a distinct non-generating cost through admission
and settlement: they cannot acquire positive generation TPM debt from a late
usage value. Generation with unknown/unlimited TPM is a separate unmetered cost,
not metadata. Generic zero estimates and fixture costs retain the existing late
positive usage debit behavior. This distinction lets the experimental
`bench-harness` backfill consider genuine metadata without treating every zero
cost as safe. Metadata still uses upstream HTTP, RPM, execution slots, root FIFO,
cooldown and deadlines; it does not use a local replacement catalog. The default
policy remains RR with its existing barrier.

Known TPM rejects missing bounds (`output_bound_required`), zero, null, negative,
fractional or malformed inspected caps (`invalid_output_bound`), wrong-endpoint
cap fields (`unsupported_output_bound_field`), and ambiguous Chat caps
(`ambiguous_output_bound`). Chat generation count `n` must be absent or 1.
Input count plus overhead plus output arithmetic is checked; a request whose estimate exceeds capacity
returns `400 estimate_exceeds_budget`, which does not claim its actual tokens
exceed the provider limit. These errors occur before admission/upstream traffic.

The one-pass visitor scans generation input locations without retaining a JSON
body tree. Chat/Messages message content and Responses input/content/tool-output
blocks recognize text and the supported tool/thinking/refusal forms. Media,
unknown block types and malformed content shapes receive
`unsupported_multimodal_estimate` with known TPM. This is a deliberately limited
text estimator; RPM/concurrency-only use can choose TPM unknown. Plain text
mentioning images/base64, opaque tool arguments/input, tool JSON schemas and
unknown top-level fields are not interpreted as media. Count-tokens metadata
requests may themselves contain media because their generation TPM cost is zero.
Duplicate top-level policy fields, including escaped equivalent keys, are
rejected; this includes the output caps and endpoint-relevant input/count fields.

Default `accounting = "actual"` settles the
remaining current-window reservation once at successful response-body EOF from
supported final SSE or complete JSON usage. Explicit `reserved` keeps the entire reservation
until its original expiry even if known final usage is smaller. Missing,
malformed, partial, encoded or unsupported
usage remains unknown and preserves the original reservation until expiration.
Larger actual usage produces debt: subsequent candidates wait until they fit.
An expired reservation can never produce a refund into fresh budget. For a
request still active past its 60-second reservation, known final usage is a new
debit at the final observation/cleanup time, including under reserved accounting.
The observer waits for successful HTTP EOF before treating usage as final;
server-side token timing and billing timing are not known.

Chat/Responses input already includes cached input, so cache-read detail is
reported separately and is not added again. Messages input excludes cache
creation/read, so all four reported categories contribute once. Overflow in the
supported usage sum is unknown rather than wrapping. Reported usage counters
remain separate from the request estimate. JSON observation requires HTTP 200,
one valid `application/json` Content-Type and absent or one identity
Content-Encoding, checked on original headers. Duplicate/conflicting/non-UTF8
representation headers remain usage-unknown. JSON and SSE use mutually exclusive
raw observer storage of at most 256 KiB; temporary JSON parse allocations, cache
capture and delivery buffers are separate. Chunks still forward immediately.
Oversized, incomplete or malformed JSON keeps its reservation. Complete Chat
choices require terminal finish reasons; Messages requires a terminal stop reason;
Responses requires status completed with no error or incomplete marker. Valid
usage on tool/length stops can settle even when the response cannot be cached.
Required input/output counts are nonnegative integers (zero is valid); a total
alone cannot substitute. No decompression, provider-policy inference or billing
claim is added.

Only active requests and unexpired debits retain identities. Monotonic IDs never
wrap or reuse an old identity, and duplicate terminal/start/cancel events cannot
settle a different or completed request. At most 8,192 entries are retained. If
this fixed storage ceiling is reached, admission conservatively waits for an
expiry instead of discarding debits. Cleanup/start counters saturate. This
storage ceiling is not a performance or throughput claim.

The doc-hidden `server::testing` clock and prestart gate supply deterministic
synthetic loopback tests in both debug and release. They are available only via
explicit library calls, with no runtime flag or header control. The exact fixture
cost constructor also exists only as a doc-hidden library test seam and is never
read from the wire. A separate paused-Tokio unit test exercises the production
coordinator clock and start notification; Tokio `test-util` is a development
feature only.

Ordinary `llmgw status` renders the exact-cache snapshot: enabled state, policy
evaluations, hits, eligible misses, named bypass counts, entries and
retained/budget bytes. Missing data displays `n/a`; misses exclude requests that
fail cache eligibility. These fixed counters add no request log or polling.

## Installed Pi compatibility probe

The Pi transport probe uses an explicit temporary RPM unlimited / TPM unknown
fixture and records both modes. It does not change the example or production
warmup policy. Its Python regression suite includes this fixture precondition.

`scripts/probe_pi.py` runs the installed
`@earendil-works/pi-coding-agent` 0.84.2 through the foreground gateway and a
fixed loopback OpenAI Chat Completions SSE fixture. The configured local route
is `/r/pi-work/v1/chat/completions`; the fixture observes the joined upstream
route `/team/v1/chat/completions` and the synthetic end-to-end header. It also
observes that `Authorization`, `X-LLMGW-Token`, and
`X-LLMGW-Control-Token` do not reach upstream when gateway auth is `none`.

The probe covers two flows. The first disables all tools and requires one HTTP
attempt plus a fixed completion marker. The second enables only Pi's built-in
`read` tool. Its fixture emits exactly one call for an exact absolute path in a
temporary fixture directory. The probe checks one `read` start, one successful
`read` end, the fixed synthetic file content, the matching tool-call ID and
content in the next request, and a final marker. The second flow therefore has
exactly two upstream attempts. The working directory starts empty, while the
only readable target supplied by the fixture is in a separate temporary
directory. This allowlist and deterministic fixture are the test boundary;
Pi's built-in `read` allowlist is not a general filesystem sandbox.

Pi runs with a temporary `PI_CODING_AGENT_DIR`, no sessions, context files,
extensions, skills, prompt templates, or themes, offline startup, thinking off,
telemetry disabled, and both client retry layers disabled. The gateway,
fixture, Pi process, configuration, mode-0600 synthetic token files, and test
file are owned by the probe and cleaned up without global process matching.
Normal completion, timeout, and `KeyboardInterrupt` all terminate when needed,
reap the owned Pi process, and close its captured pipes; interruption still
propagates to the caller. The JSON artifacts retain booleans, counts, versions,
paths to installed code, source and binary hashes, and policy names. They do
not retain prompts, request bodies, credential values, or raw Pi stdout/stderr.

The installed package differs from the reference clone used for source
research: installed Pi is 0.84.2, while the clone at
`f3c672245d25ef2283ffc0d9cdec8a5482651103` declares 0.85.1. The observed
result therefore applies to installed Pi 0.84.2 on
`macOS-26.5.1-arm64-arm-64bit-Mach-O` and OpenAI Chat Completions SSE over
HTTP/1.1. It establishes agent/mock compatibility for these two deterministic
flows. It does not establish real LLM quality or performance, other providers
or protocols, Windows/Linux behavior, or a filesystem security boundary.
When the optional research clone is absent, its version, commit, and comparison
remain `null`; absence is not reported as a known difference and does not start
a Git lookup. A commit is recorded only when Git reports the expected clone
directory as its repository root.

The reproducible positive command is:

```sh
python3 scripts/probe_pi.py --binary target/debug/llmgw --output artifacts/pi-e2e.json
```

The matching gateway-off control uses identical Pi JSON mode and acceptance
checks, omits only gateway startup, writes `passed=false`, and exits nonzero:

```sh
python3 scripts/probe_pi.py --binary target/debug/llmgw --gateway-off --output artifacts/pi-e2e-gateway-off.json
```

## Forwarding contract

A configured root such as `pi-work` maps
`/r/pi-work/v1/chat/completions` to the exact configured upstream API-base
prefix plus `/chat/completions`. Configured and request query strings remain in
their received encoded form only when that exact composed query survives the
`url::Url` serialization required by Reqwest. A decoded key present in both
query strings is rejected before an upstream attempt.

This is an explicit initial support boundary, not arbitrary raw-query support.
For example, a literal apostrophe would be changed to `%27` by the URL stack,
so the gateway returns `400 unsupported_query_encoding` instead of silently
changing and forwarding it. Clients must send the apostrophe as `%27`.
Percent-encoded apostrophes, `+`, and percent escapes with lowercase hex such
as `%2b` were verified to remain byte-identical. An empty query introduced by a
trailing `?` is also rejected because Reqwest normalizes it away. The same
lossless, nonempty query requirement applies to `upstream.api_base` during
config validation, with errors that do not echo the input value.

The data endpoint allowlist is exact:

- `POST chat/completions`
- `POST responses`
- `POST messages`
- `POST messages/count_tokens`
- `GET models`

The configured root, endpoint, HTTP method, and body model allowlist are
checked before forwarding. `GET models` needs no generation body or output
bound. Responses requests with `background=true`, HTTP upgrades, WebSockets,
and unknown routes are rejected before upstream traffic. POST JSON is inspected
once with a selective visitor after one whole-body UTF-8 validation that does
not copy the body. Invalid UTF-8 anywhere in the JSON, including an otherwise
ignored value or nested key, receives `400 invalid_json` before an upstream
attempt. Duplicate top-level `model` or `background` fields, including
equivalent escaped key spellings such as `mo\u0064el`, receive
`400 duplicate_inspected_field` before an upstream attempt. Unknown fields are
skipped during policy inspection, and duplicates in unknown or nested fields
are not rejected by this rule. For accepted requests, the original received
bytes, including unknown fields and whitespace, are forwarded without insertion
or rewriting.

The shared Reqwest client has automatic retry, redirect following, and ambient
proxy discovery disabled. It uses a 10-second connect timeout, a 30-minute
total request timeout, and at most 16 idle pooled connections per host. The
response status, end-to-end headers, and bytes stream back directly.

`Connection`-nominated fields and standard hop-by-hop headers are removed in
both directions. The upstream Host is generated from the upstream URL. The two
gateway credential headers are never sent upstream. Other headers, response
status, compression metadata, body bytes, and Content-Length remain intact
where HTTP framing permits. An actual fixed gzip fixture verifies unchanged
status 201, `Content-Encoding`, compressed `Content-Length`, and compressed
response bytes without decompression.

## Credentials and local control

Data requests use the client's standard authentication. With upstream
`auth.mode = "forward"`, `Authorization` and `x-api-key` reach the upstream
unchanged. Env/none modes replace or remove them according to the explicit
configuration. No local data token is required or created. The listener is
loopback-only; data requests reject browser `Origin`, nonlocal `Host`, and POST
requests without a JSON content type. Native processes on the same PC are
trusted. The process lock prevents duplicate workers; it is not authentication.

The separate `X-LLMGW-Control-Token` authenticates only these loopback endpoints:

- `GET /_llmgw/health`
- `GET /_llmgw/status`
- `POST /_llmgw/stop`

Any `Origin` header rejects a control request. Upstream credentials do not
authorize control access, and control paths never reach the configured upstream. Status
contains bounded aggregate lifecycle, observation, token, and queued-byte
counters; it contains no request IDs, headers, credential values, or payloads.

## Stream ownership, cancellation, and observation

Admission acquires one joint execution/quota hold. One supervised worker owns
that permit, the HTTP attempt starting before upstream headers, the response
body stream, and one terminal outcome. A downstream handler or response-body
drop does not own this lifetime. The permit is released only after the local
upstream HTTP future/body has dropped or reached actual response-body EOF.
Terminal cleanup is held in one RAII guard and records one outcome across EOF,
transport error, deadline, downstream close, or forced shutdown races.

`cancel_policy = "drain"` is the default. If a downstream connection resets,
the worker stops delivering bytes and drops cache capture. It retains only bounded
observer state, discards subsequent delivery data, and retains its permit until
upstream body EOF, error, or the original absolute 30-minute request deadline.
`cancel_policy = "close"` drops the owned local HTTP attempt promptly. The close tests prove that the fixture HTTP socket closes;
they do not prove that a remote provider stops GPU, KV-cache, or billed work.

Reset detection uses Tokio error readiness plus `SO_ERROR` through socket2.
Actual loopback RST is tested both before and after upstream headers. A request
write-half-close remains valid while the upstream is silent. A FIN by itself is
therefore deliberately not treated as a full downstream cancellation; native
no-delay cancellation for every FIN-only close form is not claimed.

Downstream delivery has an eight-item channel and a separate 64 KiB byte
semaphore per response lifetime. Each delivered item is copied into at most
16 KiB so a small slice cannot retain a large upstream backing allocation.
Currently executing workers therefore contribute at most 1 MiB of queued
application payload at the maximum configured concurrency of 16. This is not a
process-wide 1 MiB bound: after upstream EOF releases an execution slot, a
finished response can retain its delivery receiver and queued bytes while a new
worker starts. The current chunk being returned by Reqwest/Hyper and their
internal transport/socket buffers are also outside that queued-byte counter.
The Task 7 resource accounting below separates a conservative aggregate
application-payload bound from unmeasured allocator/transport/RSS peaks. A real 4 MiB fixture
with a stalled reader verifies the 64 KiB per-response application queue bound
and disconnect progress.

Payload delivery and body termination use separate bounded paths. The payload
channel ending becomes a successful downstream EOF only after the worker has
observed successful upstream body EOF. Deadline, upstream body error, or forced
worker interruption makes the downstream HTTP body fail even when all 64 KiB
of payload capacity is occupied. The gateway does not append an SSE error or
rewrite the already accepted status.

SSE bytes are passed in their original order without decoding, reserialization,
or response-wide allocation. The side observer handles split UTF-8 JSON data,
LF/CR/CRLF (including split CRLF), one split initial UTF-8 BOM, comments, event
fields, and multiline data. An event is dispatched only at an empty-line
delimiter; pending data at body EOF is discarded. Comments and unknown event
types stay transparent. One fixed storage buffer holds accumulated data and the
current line; known event names and the BOM prefix use fixed arrays. Their
combined retained buffer allocation is exactly 256 KiB per SSE observer. A
multiline data event followed by a large unknown event name and unfinished
comment verifies the aggregate allocation. Temporary JSON parse allocations,
the current transport chunk, and exact process RSS are separate and remain
unmeasured here. Supported JSON uses the same raw observer allowance instead of
SSE storage and is parsed once at clean EOF. Overflow, invalid supported usage,
encoded or unsupported content, a missing terminal usage report, or unsupported endpoints produce
usage `unknown` while wire delivery continues unchanged.

Observed instants and counters are named for what the gateway can see:
`first_body_byte`, `first_observed_output_delta`, `terminal_marker`, and
`response_body_eof`. A role/comment event is not counted as model output. A
terminal protocol marker is not treated as HTTP body EOF and does not release
capacity. JSON observation does not synthesize SSE output-delta or terminal-marker
counters; its first body byte and HTTP EOF retain their existing meanings.

- Streaming Chat Completions accepts only the final `choices: []` usage chunk before
  `[DONE]` for accounting and reads prompt/completion tokens. Numeric usage in
  nonempty, missing or malformed choices does not qualify as final usage;
  normal null-usage deltas and numeric interim observations may still precede a
  valid supported final report. Usage arriving after `[DONE]` or an explicit
  stream error leaves accounting usage unknown. Optional
  `prompt_tokens_details.cached_tokens` is preserved as a separate cache-read
  subset; it is not added to the already-inclusive prompt total. Missing cache
  detail and explicit zero are valid zero, while malformed detail, a cache
  subset greater than input, or decreasing cumulative usage makes final usage
  unknown. The gateway never rewrites a request to force `include_usage`; an
  omitted final report remains unknown.
- Responses records completed nested `response.usage` input/output and keeps
  cached input separate. `response.incomplete`, `response.failed` and standard
  `error` events invalidate accounting usage permanently for that stream, even
  when they precede or follow a completed report. A completed event with a
  contradictory nested status/error or without required usage cannot reuse an
  earlier completed report. Repeated consistent completed observations remain
  supported; unknown event types do not invalidate otherwise supported usage.
- Messages preserves input/cache fields from `message_start`, assigns rather
  than sums cumulative `message_delta.usage.output_tokens`, rejects a decrease,
  and requires final delta output usage before `message_stop`. Usage-bearing
  `message_start` or `message_delta` after stop invalidates accounting usage;
  it cannot qualify or revise a final report. Non-accounting events after stop
  do not invalidate an otherwise supported report. Cache creation and cache
  read remain separate. A standard Messages error before or after
  final usage/stop invalidates accounting usage without changing forwarded bytes.

The bounded SSE name classifier preserves the standard `event: error` name.
That explicit error invalidates final accounting usage for all three generation
observers, including when JSON omits its own type/error key. Other unknown event
names remain transparent. These observation changes do not release execution
capacity at a marker; settlement/cleanup still waits for HTTP EOF or the existing
terminal cancellation path.

Non-SSE and encoded responses use the same worker, delivery bounds, deadline,
and permit lifetime. The gateway does not decompress them merely to observe
usage. It does not replay a request or change status after downstream headers
or body have started.

Upstream auth modes have these transport meanings:

- `forward` preserves client `Authorization` and `x-api-key` values.
- `env` removes both client credential headers and inserts only the configured
  header with the environment value captured at process startup.
- `none` removes those two known credential headers and inserts no upstream
  authentication. Unknown end-to-end headers are still preserved.

Runtime credentials have no `Debug` representation and are not printed in
arguments, diagnostics, stdout, or stderr. The control credential and any
configured environment credential must be nonempty, valid HTTP header values.

## Development run precondition

Run `llmgw setup` to create or review a config. Its final apply provisions the
local control token in the protected, config-scoped state directory,
including for Save only. Then use `llmgw run` for a foreground development
process or `llmgw on` for the owned background worker; those lifecycle commands
also provision a missing control token for a hand-written config. Users do not
create control token files by hand. The token
must still be nonempty, bounded, and protected; an invalid token file
causes startup to fail without weakening its permissions or following a link.
Windows ACL creation, file identity, and replacement were exercised on Windows
10 Education x64/NTFS with Rust 1.88 GNU. The symlink-privilege case and a clean
non-admin account remain unverified. Tests use only synthetic credentials and
temporary state.

## Receiver limits

The listener accepts only a numeric loopback address. Configuration loading
validates this restriction, and the public server entry point validates it and
the fixed `1..=16` concurrency range again before credential checks or any bind
so a directly constructed `Config` cannot bypass either bound. HTTP/1 request parsing uses a 32 KiB connection buffer, at most 128
headers, a 10-second header read timeout, and at most 128 concurrently served
ingress connections. Stored request bodies are read within 30 seconds, limited
to 8 MiB each, and charged incrementally against one 32 MiB semaphore. If a new
chunk cannot acquire memory immediately, that partial request is dropped and
receives `429 gateway_memory_full`; it never waits while holding a partial body.
Declared oversize bodies receive 413 before reading.

Stop is selected before ready admission branches, drops the listener before
draining, prevents queued capacity waiters and keepalive requests from starting
new attempts, and lets accepted connection tasks and supervised workers drain
for at most 10 seconds. The forced path aborts and awaits connection and worker
tasks so RAII cleanup runs before server completion. Tests exercise listener
closure, keepalive rejection, and a gated worker completing during drain; they
do not sleep through the full production 10-second force timeout.

## Configuration schema and identity

The configuration describes one process, one upstream API base, and one shared
quota group. Unknown TOML fields and unsupported enum values are errors.

- `listen` defaults to `127.0.0.1:4141`, accepts only numeric loopback socket
  addresses, and does not select another port after a bind conflict.
- `concurrency` defaults to `1`, accepts `1..=16`, and is enforced by the
  joint queue/admission ledger for all five data endpoints, with root FIFO,
  root round-robin, and the bounded bypass/age barrier described above.
- `cancel_policy` defaults to `drain`; the only explicit alternative is
  `close`. Unknown values are rejected.
- `retry_transient_429` is an immutable boolean, default `false`. Explicit `true`
  allows at most one additional qualified rate-rejection attempt as defined below.
- `accounting` is `actual` by default or explicit `reserved`, with the local
  rolling-window behavior described above.
- `startup_hold_secs` defaults to `60`, accepts `0..=3600`, and affects startup
  admission only; the quota window remains 60 seconds.
- Optional `[cache]` enables the bounded exact response cache described below.
  `ttl_secs` defaults to `300` (`1..=3600`), and `max_history` defaults to `3`
  (`1..=64`). Omitting the table disables the cache.
- `upstream.api_base` is an HTTP(S) URL whose path and supported losslessly
  encoded, nonempty query are retained. Queries that the URL stack would
  reserialize, including a literal apostrophe or empty trailing `?`, are
  rejected. Userinfo and fragments are rejected. Debug output redacts the full
  value.
- `quota.rpm` and `quota.tpm` distinguish nonzero `known`, `unknown`, and
  `unlimited`, with joint quota admission implemented in Task 5.
- Models have unique nonempty IDs, an optional nonzero output bound and the
  explicit input estimator/allowance described above. Roots
  have unique URL-safe IDs, list configured models, and enable one or more
  exact endpoints.

Loading canonicalizes the explicit config path and fingerprints the same bytes
used for parsing. State paths are derived only from that canonical config's
parent; there is no implicit home-directory lookup.


## Optional exact response cache

Enable this for repeated short text calls whose previous result you want to
reuse. Redis, an embedding model, and a disk database are not required:

```toml
[cache]
ttl_secs = 300
max_history = 3
```

The cache reuses a previous response to a byte-identical request. It does not
produce a fresh random sample. The history threshold limits eligibility; it
does not guarantee model quality or determinism. Restarting the gateway clears
all entries. Prompts, credentials, and cache contents are not written to disk.

Keys include the root, full composed upstream URL and query, effective
request headers including authentication, and the original body. Length framing
keeps component boundaries distinct. Different credentials, roots, options,
queries, or body bytes do not share an entry. The sole header exception is
`x-stainless-retry-count`: absence, changed values or duplicate values do not
split entries. The header is still forwarded unchanged on misses. No other
`x-stainless-*` or caller header is removed from the key; changing those can
still lower the hit rate. An original response `Vary` naming this excluded
header prevents storage, case-insensitively across comma-separated or duplicate
fields, even if hop-by-hop processing would remove the Vary field later.

Only text generation requests up to 32 KiB and within `max_history` qualify.
The history limit counts input messages/items; a Responses string input is one
item. System instructions still contribute to the request byte ceiling.
Tools, tool results,
media, multiple generations, unknown generation fields, and stateful requests
bypass caching. Responses requests must explicitly use `store: false` and
cannot refer to a previous response or conversation. Bypassed requests still
follow ordinary forwarding and admission. Request `Cache-Control: no-store`
or `no-cache` bypasses lookup; response `no-store` or `no-cache`, `Vary: *`,
`Set-Cookie`, encoding, and non-200 status prevent storage.

Misses stream as chunks arrive. Capture is a separate bounded copy, and only
a complete, valid text response at HTTP EOF can become an entry. Tool/length
termination, errors, unfinished SSE tails, meaningful events after completion,
transport failure, cancellation, and oversized responses are not stored.
Cache completion and accounting usage are separate: missing usage retains the
quota reservation even when otherwise complete text can be reused.

Hits return the original body and essential content headers without starting an
upstream attempt. A request that initially misses also checks for a completed
entry while waiting for initial admission, including quota, startup hold and
shared cooldown. Cache completion wakes these waiters without re-enqueuing them
or resetting FIFO order, aging, cancellation, or deadlines. A final recheck after
admission returns an unused reservation if it finds a hit. Requests already sent
upstream and internal retry waits retain their existing behavior; this is not
full in-flight request coalescing. Stale date, request-ID, rate-limit, and retry headers are not replayed.
Hits do not consume upstream RPM/TPM or add worker/usage observations. Separate
`exact_cache` status counters describe local reuse; provider prompt-cache token
counters keep their existing meaning. The cached body retains its original
usage fields; these describe the reused response, not a new upstream charge.

`exact_cache` includes `enabled`, `considered`, `bypasses`, `hits`, `misses`, `stores`, `evictions`,
`budget_bypasses`, `entries`, `retained_bytes`, and `budget_bytes`. Hits and
misses count eligible requests, not individual rechecks or all incoming requests.
A later hit reclassifies that request's initial miss exactly once; unrelated
cache notifications do not add misses. A miss need not become a stored entry.
`retained_bytes` includes active captures and replay ownership.

`considered` counts policy evaluations after route/body validation while caching
is enabled. Invalid ingress and disabled-cache traffic do not enter this count;
it is distinct from the existing global `requests` metric. Each excluded request
increments one `bypasses` field in this precedence order:

| Field | Exclusion |
|---|---|
| `request_cache_control` | Original request Cache-Control/Pragma prohibits reuse, including when later header processing strips it |
| `size` | Request body exceeds 32 KiB |
| `endpoint` | Endpoint does not support exact caching |
| `tools_state` | Tools or stateful request fields, including empty tools |
| `history` | Input history exceeds the configured limit |
| `unsupported_shape` | Other unsupported fields or request shapes |

After evaluations and lookups settle, `considered = hits + misses + sum(bypasses)`.
Response storage rejection and payload-budget exhaustion are separate from request
eligibility. The counters reset with the worker; independent live status counters
are not a transactionally consistent whole-runtime accounting snapshot.

The fixed limits are 4 MiB of retained payload, 256 KiB per capture/entry, and
128 entries. Payload ownership includes retained headers and remains charged
while a replay still references an evicted entry. Capture never waits for
memory: exhausted capacity means ordinary forwarding. These are cache payload
bounds, not a total process RSS limit. HTTP/TLS buffers and allocator overhead
remain additional memory.

## Retry ownership and shared cooldown (Task 7)

The default is zero gateway retries. `retry_transient_429 = true` is explicit
ownership opt-in, not an assertion that client/SDK retries are disabled. The
gateway does not modify the clients' own retry settings. Reqwest automatic
retries and redirects remain explicitly disabled. Tests count the actual
loopback upstream requests, including rejected and replayed attempts.

Any `x-should-retry` field whose value is exactly lowercase `false` after
trimming spaces and tabs vetoes internal replay. Repeated fields are inspected;
one `false` wins over a separate `true`. `true` never broadens the existing
eligibility rules. The veto does not remove a 429's explicit cooldown or its
missing-timing classification and fallback cooldown. When explicit timing is
present, a veto also removes the otherwise unnecessary replay-only body probe.
Other values, including `FALSE` or a comma-joined value, are not interpreted as
this exact SDK directive. Original headers and body remain unchanged.
The negative directive follows the official
[OpenAI](https://github.com/openai/openai-python/blob/main/src/openai/_base_client.py)
and [Anthropic](https://github.com/anthropics/anthropic-sdk-python/blob/main/src/anthropic/_base_client.py)
Python clients inspected on 2026-09-16; their broader retry policies are not adopted.

Only a complete HTTP 429 response with an identity-encoded `application/json`
body of at most 16 KiB is considered for replay. The full body must be valid
UTF-8/JSON, including ignored strings and Unicode escapes. A separate typed
projection rejects duplicate error/type/code discriminators. The supported
`error.code` contracts are deliberately narrow:

| code | Compatible `error.type` (or absent/null) |
|---|---|
| `slow_down` | `rate_limit_error` |
| `rate_limit_exceeded` | `rate_limit_error`, `requests`, `tokens` |

An unknown or incompatible type/code, duplicate discriminator, contradictory
root code/type, or any non-null `error.details` prevents replay. This includes
`insufficient_quota`, credit balance, organization/project spend/usage limits,
and Claude's `details.error_code = enforced_spend_limit_reached`. Generic
Claude `rate_limit_error` without a supported code is unclassified. Free-form
messages are never searched to guess retry eligibility. These are explicitly
supported contracts, not an exhaustive provider registry or proof from a paid
provider execution.

Timing is independent of body eligibility and applies even with retry off.
Every received 429's valid `Retry-After` values (nonnegative integer seconds or
HTTP-date parsed by httpdate 1.0.3) and `retry-after-ms` values (the Azure OpenAI
nonnegative integer millisecond contract) update the one shared quota group's
cooldown. If several valid values are supplied, their maximum is used; a later
429 can extend but never shorten existing cooldown. Dates in the past mean
zero additional wait. Integers are checked as u64; negatives, signs, fractional
values, suffixes and NaN/infinity are invalid. Decimal values larger than u64
saturate group cooldown conservatively at Duration::MAX and cannot be shortened
by a simultaneous zero header; they are not replay-eligible. Saturating elapsed
time addition and checked Instant construction prevent overflow/panic. A
malformed explicit timing value also prevents automatic replay even when
another valid value still updates cooldown. It cannot silently become a shorter missing-header retry.

Only a positively recognized rejection with *both* timing headers absent gets
1,000–1,250 ms (inclusive) of group cooldown using rand 0.10.2. Header absence
alone never qualifies replay. Unknown, malformed, oversized and encoded bodies
are forwarded unchanged; headers alone can still establish cooldown. Observing
an unknown-length 429 retains at most 16 KiB of copied prefix plus the current
Reqwest transport chunk. On overflow the prefix and remaining stream use the
same bounded delivery path. There is no response-wide unbounded collection.
If this pre-header probe detects a transport failure before complete rejection
EOF, the gateway returns HTTP 502 `upstream_transport_error` without replay.
The already-observed header cooldown and uncertain attempt debt remain; its
request body and admission hold are released once. Responses excluded from
probing and failures detected after downstream headers retain the streaming
failure behavior described above.

The pre-header body probe runs only when its classification can still establish
the missing-timing cooldown or qualify a remaining internal retry. Ineligible
media types and responses whose body cannot affect either decision stream their
original headers and body immediately. Header-derived cooldown is applied first
even on these paths. The existing strict lowercase `identity` probe guard is
preserved; this optimization does not expand the replay set. If a skipped body
later fails, the client has already received the original 429 and sees a stream
failure, rather than a replacement pre-header 502. Timingless eligible JSON still
requires classification with retry disabled.

A retry starts only after complete rejected-response EOF and before any
response has been sent downstream. The rejected response and observation
prefix are dropped, its execution/quota hold settles with unknown usage, and
the worker enters the existing same root queue again. It keeps the original
wait age, original queue deadline (at most 120 seconds) and original absolute
request deadline. Waiting has no execution or provisional quota hold. A new
attempt acquires quota and concurrency jointly and charges RPM on its first
upstream poll. There is no inferred TPM refund for a 429; Reserved/Actual
unknown-usage rules remain unchanged. Request bytes share their original
32 MiB memory permits across the attempt, and release on terminal cancellation.

The original request deadline now starts before ingress body collection;
body reading still has its independent 30-second maximum. Before response
headers, queue/memory caps return protocol 429 errors and deadlines return 504.
A queue-full retry has terminal reason `admission_rejected`, not `deadline`.
During retry wait, downstream cancellation or stop removes the ticket and
releases the body without a new HTTP attempt. A group cooldown is never clamped
to one request's deadline. New clients, other configured roots and metadata
endpoints all use that same cooldown and queue. HTTP 503, redirects, ambiguous
POST socket errors and downstream-started/partial streams never replay.
503 timing headers are forwarded without creating a group-wide cooldown: a
single response does not establish that every model or credential in the group
is unavailable. This is a deliberate scope limit, not tested provider recovery.

Classification sources (read 2026-09-12):
[OpenAI error guide](https://developers.openai.com/api/docs/guides/error-codes),
[Codex pinned HTTP classification tests](https://github.com/openai/codex/blob/944d6fd1ba4baab69dbedd205282dc72ec20abb5/codex-rs/codex-api/src/api_bridge_tests.rs#L325-L356),
[Claude spend-cap contract](https://platform.claude.com/docs/en/api/rate-limits#reaching-your-spend-cap),
[Azure OpenAI millisecond header](https://learn.microsoft.com/en-us/azure/foundry/openai/how-to/quota),
[RFC 9110 Retry-After](https://www.rfc-editor.org/rfc/rfc9110.html#name-retry-after).
Official documentation/client fixtures support these interpretations; actual
provider quota behavior remains unverified. Both newly direct dependencies were
already resolved in Cargo.lock. rand's enabled `thread_rng` feature has MSRV
1.85, below this package's Rust 1.88; no runtime process or custom RNG/date
parser was added.

## Aggregate resource evidence and limits (Task 7)

Authenticated status adds `stored_request_bytes`: bytes currently charged to
the one request-body semaphore, including partial ingress, queued and retrying
bodies. It is a byte-ownership counter, not allocator capacity or RSS. Existing
`queued_response_bytes` includes owned delivery bytes until their final clone
drops, even when they have left the channel for HTTP output. The eight-item,
64 KiB, 16 KiB-per-item limits are per response lifetime.

At most 128 ingress connection tasks exist. Hyper 1.8.1 HTTP/1 owns one output
body and one service future per connection; keepalive returns to its next
request after read/write completion (verified in its local `h1/dispatch.rs`
and `h1/conn.rs`). Conservatively allowing one delivery lifetime per connection
plus all 16 executing workers gives `(128 + 16) * 64 KiB = 9 MiB` of application
delivery payload. This overcounts overlapping ownership deliberately; it is
source-derived, not an observed RSS maximum. Detached retry waiters do not
create delivery channels; discarded error prefixes are dropped before reentry.
A single concurrency cap by itself cannot bound completed response lifetimes.

Direct resource tests separately establish:

- 64 partial bodies each retain 512 KiB, filling the 32 MiB payload budget; each
  needs an additional byte. One additional byte receives immediate memory429,
  releases that 512 KiB, and a fresh complete generation request progresses
  while all other 63 senders remain incomplete. RST releases their remaining
  bytes to zero. This is a deterministic memory-pressure gate, not a latency
  comparison or 64 completed generation requests.
- 128 actual keepalive sockets are accepted; the 129th is closed, and releasing
  one connection permits a fresh metadata request.
- 24 actual stalled downstream readers produce 16 active attempts and 8 queued
  ingress requests. This observation is before upstream EOF; exact counters,
  cancellation outcomes and wire attempts are recorded in Task 7 test logs.
- A separate production delivery-primitive test retains 20 receivers after
  producer EOF, observes exactly 1,310,720 owned payload bytes, then zero when
  receivers drop. This is not a claim of 20 completed socket workers or RSS.
- Real 10-second header/30-second body timeouts and a shortened internal
  overall-deadline seam release incomplete senders; the seam has no config or
  HTTP control. Existing observer overflow and stream-close regressions retain
  raw forwarding and cleanup behavior.

The 32 MiB budget charges stored payload length; Vec spare capacity, bounded
JSON parse allocations, current Reqwest/Hyper chunks, TLS/HTTP transport and
kernel socket buffers are additional memory. The error JSON validity pass and
discriminator pass are sequential within a fixed 16 KiB observation cap.
No process-wide RSS bound, idle/peak RSS target, latency advantage, Windows
runtime behavior or provider compute cancellation is established by these
functional tests. Those require separate measurements and accepted evidence.

## Benchmark observation boundary (Task 8)

Authenticated admission status now also reports `retained`, the existing scalar
count of ledger entries. Its meaning and bounds are unchanged. The optional
`bench-harness` build enables a required-feature example that selects benchmark
FIFO or ordinary RR through example arguments. Production CLI and TOML schema
have no scheduling-policy option. Both arms use the same HTTP body estimator,
transport, admission ledger, cancellation and production deadlines.

The source checkout's `docs/benchmark-method.md` separates exact internal traces, no-wait
transport samples and the five-window composite quota pilot. Only a completed
artifact establishes its measured results; no real-provider, native OS matrix,
client compatibility or real coding-task success is established by mock timing.
