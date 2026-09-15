# Native client compatibility

This phase supports three fixed, version-checked profile formats. The current
Claude Code profile accepts **2.1.76**, verified on Windows 10 x64 with isolated
loopback traffic on 2026-09-16. The earlier 2.1.63 macOS result is historical;
that version is no longer accepted by `connect`. Pi 0.84.2 was rerun after the
data-token removal on macOS. Codex 0.154.0 completed Windows profile loading and
a Responses round trip, but its native tool policy rejected the fixture's read
command; full Windows tool compatibility is unverified. No real-provider
authentication is established by these synthetic checks.
`connect`
does not discover providers, sign in, forward a chat
subscription, or make a paid request. The selected root, model, and endpoint
are a fixed gateway route. If missing, `connect` adds them through the same
reviewed `SetupDraft` path used by setup; it never writes a second route format.
For upstream `auth.mode = "forward"`, Pi and Codex use an explicit API-key
environment reference (`OPENAI_API_KEY`, or `--client-key-env NAME`). Claude
keeps its native API-key/auth-token settings. No key value is copied into a
generated profile, and connection does not opt into subscription/OAuth forwarding.
Claude forward connection requires a configured native API credential source;
a saved claude.ai login alone is not enough. The client still controls its own
credential selection and any API-key approval prompt. See the official
[gateway credential mapping](https://code.claude.com/docs/en/llm-gateway-connect#how-the-credential-variable-maps-to-a-header).

| Client | Version | Protocol | Listing | Selection | Tools | OS and scope |
|---|---:|---|---|---|---|---|
| Pi | 0.84.2 | OpenAI Chat Completions | picker/list verified from `<home>/.pi/agent/models.json` | verified | exact read-tool result and follow-up request verified | macOS arm64, isolated user-private fixture |
| Claude Code | 2.1.76 | Anthropic Messages | opt-in discovery unsupported in this profile | verified | exact Read result and follow-up request verified | Windows 10 x64, isolated private home and Korean path; managed connect/disconnect passed |
| Claude Code (historical) | 2.1.63 | Anthropic Messages | unsupported | verified | exact Read result verified | earlier macOS arm64 fixture; not accepted by current profile |
| Codex (historical) | 0.154.0 | OpenAI Responses; WebSocket disabled | unverified because the gateway list is not a Codex catalog | verified | exact `exec_command` read result verified | earlier macOS arm64 isolated named profile |
| Codex | 0.154.0 | OpenAI Responses; WebSocket disabled | unverified | verified | blocked by native client policy; not passed | Windows 10 x64, managed connect/disconnect and synthetic response round trip |

The earlier macOS combinations also verified inference, expected failure with
the gateway off and zero new upstream fixture calls, reload after disconnect,
and preservation of an unrelated user edit. Those controls were not rerun for
the Windows clients. The Windows suite separately covers 24 client-profile
and 39 config-patch contract tests; installed-client evidence is in the
[Windows follow-up report](../../reports/windows-followup-and-improvements-2026-09-16.md).
A repeat on the final Windows binary failed once while replacing the protected
patch journal (Windows error 1175); the next isolated run passed the entire
Claude flow. The cause is unresolved, so a successful row is not a reliability
guarantee. Recovery files were retained; see the follow-up failure record.

Windows Pi and all Linux native clients remain unverified. A profile parse or
list command alone is not counted as inference or tool compatibility.

## Automatic compaction

DeskQuota does not disable client compaction or replace response usage with its
quota estimate. Synthetic native checks on 2026-09-16 verified automatic
summarization and the next turn for Pi 0.84.2, Codex 0.154.0 and Claude Code
2.1.175 on macOS, plus Claude Code 2.1.76 on Windows. Direct-provider controls
also passed. These used isolated explicit context limits; they do not certify
long real conversations or change the version checks in `connect`.

Important limits: `/responses/compact` is unsupported (404); known TPM rejects
opaque `compaction` input items (400), and the byte estimate can reject large
summary requests before usage is available. Pi's unverified 128000 context
default can also shift compaction timing. `/messages/count_tokens` works when
explicitly allowed on the root; new Claude connections add `messages` alone.
Exact-cache replay preserves the original response usage. See the
[compaction audit and reproducible checks](../../reports/auto-compaction-audit-2026-09-16.md).

## Managed connection and restoration

`connect` previews absolute target paths, scope, public URL, root, model,
protocol, owned keys, redacted credential source, disconnect command, and one
exact hash. The hash binds the client patch to the desired gateway config
fingerprint, client, root, model, and protocol. Preview alone changes no gateway
or client file. Noninteractive apply requires that exact hash:

```sh
llmgw --config /absolute/gateway/config.toml connect pi \
  --client-executable /absolute/path/to/pi \
  --client-home /absolute/synthetic-or-user-home \
  --root shared-work --model example-model

llmgw --config /absolute/gateway/config.toml connect pi \
  --client-executable /absolute/path/to/pi \
  --client-home /absolute/synthetic-or-user-home \
  --root shared-work --model example-model \
  --apply-hash <exact-preview-hash>
```

Client config location precedence is explicit: `--client-config-dir` selects the
exact native directory, then `--client-home` derives the normal native
subdirectory, then `PI_CODING_AGENT_DIR`, `CLAUDE_CONFIG_DIR`, or `CODEX_HOME`,
then `HOME` (or `USERPROFILE` when HOME is unavailable). The resolved source and
absolute native directory are part of the preview and the same directory is
used by the installed-version probe and patch. Interactive setup defaults to
that resolved native directory, including an existing client-specific
environment override. If `--client-home`, `--client-config-dir`, or an edited
setup path differs from the current native client environment, the preview
prints the exact `PI_CODING_AGENT_DIR`, `CLAUDE_CONFIG_DIR`, or `CODEX_HOME`
setting required when launching the client. llmgw does not change the user's
future shell environment.

If the worker is stopped or its authenticated fingerprint differs from the
saved config, apply remains pending. After reviewing the printed runtime
impact, repeat the command with `--restart`; llmgw starts or restarts through
the existing lifecycle path, confirms authenticated readiness for the desired
fingerprint, and only then applies the client patch. A failed activation leaves
client bytes unchanged. After an interactive confirmation, llmgw reads fresh
authenticated worker status. If the required runtime action changed while the
user was reviewing the preview, the client remains pending until the new impact
is reviewed. A setup `SaveOnly` result also leaves selected clients pending, but
final setup apply creates the protected control token material needed to activate a
client connection later; it does not start the gateway or write a client
profile. Interactive successful setup separately previews and confirms each selected
client resource after the gateway reaches readiness. It applies the same
in-memory reviewed plan after confirmation; the user does not retype its
64-character hash.

Pi receives one dedicated `providers.llmgw` object with `baseUrl`, explicit API
type and model metadata. Forward mode uses an `apiKey` environment reference
such as `$OPENAI_API_KEY`; env/none modes use a nonsecret client placeholder
that the gateway replaces or strips. No custom authentication header is added.
When the real upstream limits are unknown, the written 128000 context and 16384
output values are labeled Pi client defaults, not verified provider limits.
An unrelated existing `providers.llmgw` object is never adopted or overwritten.
For a reconnect, active ownership of the dedicated provider is retained in the
reviewed plan, checked before gateway activation, and checked again while the
existing transaction and resource locks are held before any journal or client
write. A disconnect that retires the journal invalidates an older prepared
reconnect even if the old managed bytes are recreated exactly.

Claude preserves other `env` entries and custom-header lines. It writes no
local data token. Project-local use requires
both an explicit private-file confirmation and confirmation that the file is
not Git-tracked or shared. llmgw also performs read-only `git ls-files` and
`git check-ignore` checks and verifies an existing `.claude` directory or target
is private. Unknown, tracked, unignored, shared, or symlinked target files are
refused. A resolved native `settings.json` target is canonicalized before scope
classification. Ordinary native homes outside a Git worktree do not require
Git. If the canonical target is inside a worktree, it must be untracked and
ignored; this state is checked during preview, before gateway activation, and
again immediately before the client write. Safe parent-directory aliases remain
supported, while aliases into tracked or shared targets are refused. Conflicting
process environment or supplied managed policy blocks the patch. Discovery is
opt-in and remains unsupported for the verified 2.1.76 profile. Claude 2.1.76
also requires a nonempty API-key setting before it starts a headless request.
For env/none gateway modes, the profile owns the public literal
`ANTHROPIC_API_KEY=llmgw-local-only` as a client-availability placeholder. It is
not an upstream credential; those gateway modes strip incoming client
authentication. Forward mode leaves the user's native authentication untouched.
A previous placeholder-managed value is protected and
restored on disconnect, and a process environment or managed-policy conflict is
refused.

Codex receives a separate `llmgw.config.toml` profile. Forward mode sets the
model-provider `env_key` to the selected API-key environment variable; MCP
server headers and global `config.toml` are untouched. The profile uses `wire_api = "responses"`,
`requires_openai_auth = false`, and `supports_websockets = false`. The gateway
model list is not treated as a Codex catalog. An unrelated existing
`model_providers.llmgw` object is refused. Codex reconnects use the same active
ownership checks as Pi. Disconnect removes the whole managed provider;
installed Codex profile loading verifies the restored file.

The installed-client probes run with temporary HOME, USERPROFILE, XDG, native
config, and working directories plus an allowlisted child environment. They use
loopback fixtures, synthetic local tokens, and no login, subscription, keychain,
or real API credential. Claude's generated placeholder API key is accepted only
by the client and is stripped by gateway `auth = "none"`; it never reaches the fixture.
The earlier macOS probes completed an exact synthetic file read followed by a
second model request for each client. Windows Claude 2.1.76 also passed that
flow; Windows Codex tool execution remains blocked by client policy. The Codex model catalog remains unverified because a
Responses endpoint is not its catalog. In the gateway-off controls, Claude and
Codex were terminated at the bounded eight-second timeout with zero upstream
fixture calls; those expected failures are not positive inference results.

Disconnect uses the protected journal and restores only keys whose current
value still equals a value written by llmgw:

```sh
llmgw --config /absolute/gateway/config.toml disconnect pi
```

Unrelated edits survive. User changes to an owned value are reported as a
conflict and preserved. A newly created unchanged file can be removed; a
user-changed created file is preserved. Disconnect does not remove the separate
gateway control token or alter the upstream credentials.

`llmgw off` preserves client URLs and autostart intent. The client will fail
until `llmgw on` or disconnect restores its prior values. `llmgw status` reports
each client journal as a separate state. Listing, selection, inference, and
tools are separate results; no unverified capability becomes true because a
profile parsed or a list command exited successfully.

The gateway controls its loopback routes and strips local control/data headers
before upstream forwarding according to the runtime contract. It does not
control every network operation, telemetry setting, or update behavior of Pi,
Claude Code, or Codex.
