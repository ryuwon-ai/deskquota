<p align="center">
  <img src="assets/deskquota-hero.png" alt="DeskQuota — Your LLM quota, managed at your desk." width="100%">
</p>

<p align="center"><strong>Keep your tools. Share your LLM limits.</strong></p>
<p align="center">A lightweight local gateway for agents and scripts using the same limited LLM endpoint.</p>
<p align="center">English · <a href="README.ko.md">한국어</a></p>
<p align="center"><a href="#get-started">Get started</a> · <a href="#connect-your-tools">Connect your tools</a> · <a href="#choose-your-limits">Choose your limits</a></p>

---

One agent is coding. Another is reviewing. A script retries in the background.
They share an API allowance, but none can see what the others are doing.

**DeskQuota gives those requests one place to coordinate.** It runs on your PC,
between your existing tools and one company, hosted, or local LLM endpoint.
It schedules requests within your limits, reuses eligible identical responses,
and coordinates recovery when the upstream is busy or failing.

**One Rust executable. No Docker, WSL, Python, Node, or database needed to run it.**
The terminal command is `llmgw`.

```text
Claude Code ─┐
Codex ───────┼──► DeskQuota on localhost ──► Your LLM endpoint
Pi / scripts ┘      quota · queue · cache · recovery
```

## What it handles

| When… | DeskQuota… |
|---|---|
| Several tools share an allowance | Tracks RPM, TPM and concurrency across this instance. |
| One workload fills the queue | Gives configured client queues turns, with starvation protection. |
| An eligible identical request repeats | Reuses a complete response; concurrent duplicates can share one cache fill. |
| The provider returns `429` | Shares cooldown and valid retry/reset signals across waiting requests. |
| An upstream repeatedly fails | Uses bounded circuit protection and one recovery probe per protected scope. |
| A response streams back | Forwards chunks and reconciles reservations with supported final usage. |

## Get started

**Early preview.** This README describes the source checkout. Published
`v0.1.0-preview.1` packages predate several cache and recovery improvements.
Build from source for the current implementation.

### Native packages

Download an archive, its installer, and both matching `.sha256` files from
[the preview release](https://github.com/ryuwon-ai/deskquota/releases/tag/v0.1.0-preview.1).
You can transfer these four files to an offline PC for installation.

| Platform | Archive | Installer |
|---|---|---|
| macOS · Apple silicon | `llmgw-macos-arm64.tar.gz` | `install.sh` |
| Windows · x64 | `llmgw-windows-x64.zip` | `install.ps1` |

<details>
<summary>macOS installation</summary>

Run from the directory containing the four downloaded files:

```sh
shasum -a 256 -c install.sh.sha256 &&
sh install.sh --archive ./llmgw-macos-arm64.tar.gz \
  --checksum-manifest ./llmgw-macos-arm64.tar.gz.sha256 \
  --install-dir "$HOME/.local/bin" --path-action preview
```

</details>

<details>
<summary>Windows installation</summary>

Run in PowerShell from the directory containing the four downloaded files:

```powershell
$expected = ((Get-Content ./install.ps1.sha256 -Raw).Trim() -split '\s+')[0]
if ((Get-FileHash ./install.ps1 -Algorithm SHA256).Hash -ne $expected) { throw 'Installer checksum mismatch' }
& ./install.ps1 -Archive ./llmgw-windows-x64.zip `
  -ChecksumManifest ./llmgw-windows-x64.zip.sha256 `
  -InstallDir "$env:LOCALAPPDATA\Programs\llmgw\bin" -PathAction Preview
```

</details>

The installer verifies the archive and prints PATH instructions; it does not edit
your shell profile or registry PATH. Add the printed directory to your user PATH
and open a new terminal, or run the executable by its full path.
These are unsigned previews; follow your OS and company installation policy.
Linux and Intel Mac packages are not currently provided.

### Build from source

Requires Rust 1.88 and the native compiler/linker for your platform. On macOS:

```sh
git clone https://github.com/ryuwon-ai/deskquota.git
cd deskquota/product
CARGO_TARGET_DIR="$HOME/.cache/deskquota/cargo" cargo install --locked --path .
```

Keep Cargo's binary directory on PATH. Windows source builds need MSVC or GNU;
the prebuilt package needs neither. Add `--features bpe` only for the optional
tokenizer-based estimator, which uses more memory.

### Set up once, start when you need it

```sh
llmgw setup
llmgw on
llmgw status
llmgw off
```

Setup asks for your endpoint, API format, models, authentication, RPM/TPM,
concurrency and optional exact caching. It previews changes before saving.
Running `llmgw` without a configuration also opens setup.

| Command | Purpose |
|---|---|
| `llmgw setup` | Create or revise your configuration. |
| `llmgw on` / `llmgw off` | Start or stop the background gateway. |
| `llmgw status` | Inspect queues, quota accounting, cache and errors. |
| `llmgw doctor` | Check configuration locally, without an upstream call. |
| `llmgw restart` | Apply saved configuration to the running gateway. |
| `llmgw autostart on` / `llmgw autostart off` | Preview login-startup changes; apply with the printed hash. |

## Connect your tools

The upstream must speak the API your client uses. DeskQuota forwards Chat
Completions, Messages and HTTP Responses; it does not translate between them.

| Managed client profile | API | Accepted version |
|---|---|---|
| Pi | Chat Completions | 0.84.2 |
| Claude Code | Messages | 2.1.76 |
| Codex | Responses over HTTP | 0.154.0 |

Managed profiles are version-checked. For other clients or versions, configure
the base URL manually and verify the protocol and model capabilities.
See `llmgw connect --help`. For a configured root and model:

```sh
llmgw connect pi --root pi-work --model YOUR_MODEL_ID
# Review the preview, then repeat with --apply-hash HASH_FROM_PREVIEW.
llmgw disconnect pi
```

**Connecting changes the client's base URL and configuration.** The preview
shows the exact file and fields: for example, Pi's `models.json`, Claude's
`settings.json`, or Codex's `config.toml`. Model lists and capabilities may differ
from the original connection. Stopping DeskQuota leaves these settings in place;
`disconnect` restores managed values that have not subsequently changed.

Forward mode passes normal `Authorization` or `x-api-key` authentication; no
custom data token is required. The worker binds to loopback and protects lifecycle
controls separately. A chat subscription or browser login is not an API credential.

## Choose your limits

| Setting | Meaning |
|---|---|
| Numeric RPM / TPM | Enforce a local rolling 60-second budget. |
| `unknown` | Add no local cap; let the provider enforce its allowance. |
| `unlimited` | Explicitly choose no local cap. |

Concurrency and shared cooldown still apply. A continuously refilling provider
allowance differs from a rolling window. Use a numeric cap when you want that
local budget; it cannot account for other PCs spending the same allowance.

- **Tokens:** new configurations use `actual` to reconcile reservations with valid
  final usage. Input estimates are approximate; missing usage retains the reservation.
  Use `reserved` when the provider's quota contract requires it.
- **Exact cache:** off by default. Enable it in setup, or add `[cache]` with
  `ttl_secs = 300` and `max_history = 3`. Payload storage is bounded to 4 MiB.
  Eligible identical text requests reuse an earlier complete answer and its usage.
  Tool calls, stateful requests and incomplete responses are excluded.
- **Retries:** off by default. Top-level `retry_transient_429 = true` allows at most
  one eligible pre-output replay per request. Client retry limits and deadlines
  still apply; long outages without timing hints can outlast them. No automatic
  replay after streaming starts.
- **Restart hold:** `startup_hold_secs` defaults to 60; values from 0 to 3600 are
  accepted. It protects known quota windows after restart. Set 0 to disable it.

Run `llmgw restart` after editing the saved configuration.

## Boundaries

- One instance manages one endpoint and its own traffic. It cannot increase quota,
  observe every outside consumer, or pause and resume a running generation.
- Waiting can preserve completion opportunities while increasing latency. An
  already well-paced single client may gain little from another gateway.
- Native-client automatic compaction has been exercised, but `/responses/compact`
  is unsupported. Known-TPM inspection rejects opaque compaction inputs and
  estimates exceeding the configured budget.
- macOS and Windows have native packaging workflows. Current Windows client
  reliability, long-running workloads and low-end hardware need broader validation.
  There is no universal speed or zero-429 guarantee.

## Development

From `product/`:

```sh
cargo test --locked
cargo fmt --check
cargo clippy --locked --all-targets --all-features -- -D warnings
python3 -m unittest discover -s tests -p 'test_*.py'
```

Tests use synthetic fixtures. Keep build outputs outside synced folders such as
iCloud Desktop. Research notes, raw measurements, generated data and local
credentials stay out of Git; source, regression tests and CI remain versioned.

For bug reports, include the client version, sanitized configuration, expected
behavior and a minimal reproduction. Remove keys, company URLs, prompts and
private source code before sharing.

## License

[MIT](LICENSE-MIT) or [Apache-2.0](LICENSE-APACHE), at your option.
