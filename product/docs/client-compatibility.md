# Native client compatibility

This phase supports three fixed, version-checked profile formats. The versions
below were freshly exercised with one checksum-verified macOS arm64 packaged
binary on the Apple M4 development host. `connect`
does not discover providers, sign in, forward a chat
subscription, or make a paid request. The selected root, model, and endpoint
are a fixed gateway route. If missing, `connect` adds them through the same
reviewed `SetupDraft` path used by setup; it never writes a second route format.
Automatic connection is refused for upstream `auth = "forward"` because an
existing client subscription credential or display placeholder could otherwise
reach an arbitrary upstream.

| Client | Version | Protocol | Listing | Selection | Tools | OS and scope |
|---|---:|---|---|---|---|---|
| Pi | 0.84.2 | OpenAI Chat Completions | picker/list verified from `<home>/.pi/agent/models.json` | verified | exact read-tool result and follow-up request verified | macOS arm64, isolated user-private fixture |
| Claude Code | 2.1.63 | Anthropic Messages | opt-in discovery unsupported in this profile | verified | exact Read result and follow-up request verified | macOS arm64, isolated user-private fixture |
| Codex | 0.154.0 | OpenAI Responses; WebSocket disabled | unverified because the gateway list is not a Codex catalog | verified | exact `exec_command` read result and follow-up request verified | macOS arm64, isolated named profile |

All three combinations also verified inference, expected failure with the
gateway off and zero new upstream fixture calls, reload after disconnect, and
preservation of an unrelated user edit. Linux and Windows client runtime remain
unverified. A profile parse or list command alone is not counted as inference or
tool compatibility.

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
final setup apply creates the protected local token material needed to preview a
client connection later; it does not start the gateway or write a client
profile. Interactive successful setup separately previews and confirms each selected
client resource after the gateway reaches readiness. It applies the same
in-memory reviewed plan after confirmation; the user does not retype its
64-character hash.

Pi receives one dedicated `providers.llmgw` object with `baseUrl`, explicit API
type, explicit model metadata, and the native custom `X-LLMGW-Token` header.
When the real upstream limits are unknown, the written 128000 context and 16384
output values are labeled Pi client defaults, not verified provider limits.
An unrelated existing `providers.llmgw` object is never adopted or overwritten.
For a reconnect, active ownership of the dedicated provider is retained in the
reviewed plan, checked before gateway activation, and checked again while the
existing transaction and resource locks are held before any journal or client
write. A disconnect that retires the journal invalidates an older prepared
reconnect even if the old managed bytes are recreated exactly.

Claude preserves other `env` entries and custom-header lines. Its llmgw token
is never written to shared `.claude/settings.json`. Project-local use requires
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
opt-in and remains unsupported for the verified 2.1.63 profile. Claude 2.1.63
also requires a nonempty API-key setting before it starts a headless request. The profile therefore owns the public literal
`ANTHROPIC_API_KEY=llmgw-local-only` as a client-availability placeholder. It is
not an upstream credential; automatic connect is limited to gateway auth modes
that strip incoming client authentication. A previous value is protected and
restored on disconnect, and a process environment or managed-policy conflict is
refused.

Codex receives a separate `llmgw.config.toml` profile. The model-provider
`http_headers` table carries the local token; MCP server headers and global
`config.toml` are untouched. The profile uses `wire_api = "responses"`,
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
Pi, Claude, and Codex each completed an exact synthetic file-read result followed
by a second model request. The Codex model catalog remains unverified because a
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
user-changed created file is preserved. The local data token is shared gateway
state and is not deleted by disconnect.

`llmgw off` preserves client URLs and autostart intent. The client will fail
until `llmgw on` or disconnect restores its prior values. `llmgw status` reports
each client journal as a separate state. Listing, selection, inference, and
tools are separate results; no unverified capability becomes true because a
profile parsed or a list command exited successfully.

The gateway controls its loopback routes and strips local control/data headers
before upstream forwarding according to the runtime contract. It does not
control every network operation, telemetry setting, or update behavior of Pi,
Claude Code, or Codex.
