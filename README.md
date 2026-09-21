<p align="center">
  <img src="assets/deskquota-hero.png" alt="DeskQuota — Your LLM quota, managed at your desk." width="100%">
</p>

<p align="center"><strong>More work from the LLM quota you already have.</strong></p>
<p align="center">Keep your tools. Let DeskQuota coordinate the limits.</p>
<p align="center">
  <a href="https://github.com/ryuwon-ai/deskquota/actions/workflows/native.yml"><img src="https://github.com/ryuwon-ai/deskquota/actions/workflows/native.yml/badge.svg?branch=develop" alt="Native packages CI"></a>
  <a href="LICENSE-MIT"><img src="https://img.shields.io/badge/license-MIT%20%2F%20Apache--2.0-blue" alt="MIT or Apache-2.0 license"></a>
</p>
<p align="center">English · <a href="README.ko.md">한국어</a></p>
<p align="center"><a href="#why-deskquota">Why DeskQuota</a> · <a href="#performance-at-a-glance">Performance</a> · <a href="#get-started">Get started</a> · <a href="#connect-your-tools">Connect your tools</a> · <a href="#choose-your-limits">Configuration</a></p>

---

**DeskQuota is a lightweight LLM gateway built to make limited capacity go further.**
It brings request scheduling, exact caching, token accounting and recovery together
for Claude Code, Codex, Pi and your own scripts.

Run it on your PC, point your tools at localhost, and give them one place to share
a hosted API, company endpoint or local LLM. Each instance coordinates one upstream
endpoint and the traffic you send through it.

**One Rust executable. No Docker, WSL, Python, Node, Redis or database to operate.**
Set it up with `llmgw setup`. Start with `llmgw on`. Stop with `llmgw off`.

## Why DeskQuota

| Advantage | What you get |
|---|---|
| **Spend quota on useful work** | One shared RPM, TPM and concurrency budget coordinates outgoing requests across your tools. |
| **Skip repeat generation** | Optional exact TTL caching serves eligible identical requests without another upstream call. Concurrent duplicates can share one cache fill. |
| **Give every agent a turn** | Fair scheduling across configured client queues, with starvation protection for requests waiting their turn. |
| **Recover as a group** | Shared `429` cooldowns, provider retry/reset signals and circuit protection coordinate recovery across callers. |
| **Reclaim unused token reservations** | Supported final usage, including streaming usage, settles the tokens actually consumed and releases unused reservations. |
| **Keep the workflow you like** | Standard API authentication, managed client connection profiles and native commands. No custom data-token header required. |

The focus is **quota efficiency across tools, with the footprint of a local utility**.
Scheduling, caching and recovery work together in the same process, with bounded
queues and a bounded in-memory cache.

## Performance at a glance

| Idle worker memory | Local HTTP round-trip p95 | Upstream calls per identical burst |
| :---: | :---: | :---: |
| **9.59 MiB** | **0.46 ms** | **10 → 1** |

**90% fewer upstream calls. A 9.0× faster duplicate batch.** Enabling exact caching
reduced completion time for ten identical requests from **1,035 ms to 114 ms**.
Both cache-off and cache-on configurations completed **50/50 requests** across five runs.

Native `on` / `off` took **39 ms / 35 ms** respectively, measured as the median of
three start/stop cycles.

### Compared with Bifrost and LiteLLM

**A smaller footprint and lower latency on the local request path.** In the same
loopback SSE workload, DeskQuota used **84% less idle memory than Bifrost** and
**97% less than LiteLLM**. Its HTTP round-trip p95 was **44% lower than Bifrost**
and **92% lower than LiteLLM** with the versions and configurations below.

| Local SSE workload | DeskQuota | [Bifrost](https://github.com/maximhq/bifrost) · transports/v2.1.1 | [LiteLLM](https://github.com/BerriAI/litellm) · v1.100.1 |
|---|---:|---:|---:|
| Idle process RSS | **9.59 MiB** | 60.41 MiB | 286.05 MiB |
| HTTP round-trip p95 | **0.46 ms** | 0.82 ms | 5.74 ms |
| Validated completions | 500/500 | 500/500 | 500/500 |

**Shared recovery cuts redundant upstream calls.** During a one-second `429`
outage, DeskQuota completed the same **40/40 requests** with **44% fewer upstream
calls than Bifrost** and **59% fewer than LiteLLM**.

| One-second `429` outage · five runs | DeskQuota | Bifrost | LiteLLM |
|---|---:|---:|---:|
| Total upstream calls, including retries | **45** | 80 | 109 |
| Validated completions | 40/40 | 40/40 | 40/40 |
| Request completion p95, including retry waits | 1.42 s | **1.32 s** | 1.69 s |

Here, shared recovery traded about **0.10 s of p95 latency versus Bifrost** for
**44% fewer upstream calls**. These tables measure HTTP handling and recovery;
model generation time remains upstream. DeskQuota focuses on coordinating one
endpoint; Bifrost and LiteLLM also cover multi-provider routing and API translation.

<details>
<summary>Benchmark setup and measurement definitions</summary>

- **Build and host:** September 21, 2026; Apple M4, 32 GiB RAM, macOS 26.5.1.
  Rust 1.88 release build with default features, source
  [`03f4009`](https://github.com/ryuwon-ai/deskquota/commit/03f4009085c6ef8fceea84c07fb69f26118c3de3),
  from the [native CI package](https://github.com/ryuwon-ai/deskquota/actions/runs/35546518243).
- **Compared builds:** Bifrost HTTP transport
  [`transports/v2.1.1`](https://github.com/maximhq/bifrost/tree/c193745d2a713e9f58f021d43e138df5eb7e038a),
  built from source with the upstream load-test UI placeholder; LiteLLM
  [`v1.100.1`](https://github.com/BerriAI/litellm/tree/1dba17b10ded12ad0021edb453ba2c54e4637928),
  installed from PyPI in an isolated Python environment. Bifrost used file-only
  configuration with governance, logging and telemetry disabled; LiteLLM used one
  deployment and one worker, with spend logging and telemetry disabled. No Redis
  or database services were used. These are pinned releases, not a comparison of
  every feature or each project's published high-concurrency benchmark.
- **HTTP latency and memory:** five runs per path, rotating direct, DeskQuota,
  Bifrost and LiteLLM; 100 sequential SSE requests after five warmups per run,
  reused connections, cache and retries off, no quota wait, upstream concurrency
  cap 2. LiteLLM used `usage-based-routing-v2`, pre-call checks and nonbinding
  RPM/TPM limits. All **500/500 requests per path** completed. Latency is the
  median of five per-run p95 values; direct HTTP was **0.23 ms**. Timing covers
  the client and loopback fixture, which adds no generation delay. RSS is the
  median of five idle gateway-process samples before warmup; it excludes the
  client and fixture. Percentage reductions use unrounded medians.
- **Recovery:** five runs per gateway in rotating order; eight SSE requests per
  run arriving 100 ms apart, a one-second upstream `429` outage with remaining
  delay in `retry-after-ms`, then 100 ms service per call at concurrency 2.
  All paths used the same client retry policy: at most four attempts, provider
  hints or seeded exponential backoff, and a five-second deadline per request.
  Gateway retries and caches were off. DeskQuota used provider-managed limits
  and shared cooldowns; Bifrost used the file-only configuration above; LiteLLM
  used `simple-shuffle` without local RPM/TPM or pre-call checks. Completion
  latency includes retry waits; p95 is the median of five per-run values. Calls
  include failed attempts, and all **40/40 requests per gateway** completed.
  A separate requested-usage check passed for all three before measurement.
- **Exact cache:** five alternating off/on pairs using the same executable,
  ten identical JSON requests per burst, a fresh cache, concurrency 1, ample quota
  and startup hold disabled. The fixture holds responses until the burst is queued,
  then adds a fixed **100 ms service time per upstream call**. Batch time runs
  from first submission to last validated completion; reported times are medians
  of five batches. Upstream calls total **50 without caching, 5 with caching**.
- **Lifecycle:** start/stop times measure CLI lifecycle completion; the configured
  quota startup hold is separate from process startup.

</details>

## Built for the way you work

- **Coding, reviewing and automating in parallel.** Give each configured client
  queue a turn while they share the same allowance.
- **Repeating classifiers, scripts or short requests.** Reuse eligible identical
  answers through exact caching, saving both a provider round trip and quota.
- **Working with an API plan or company endpoint.** Set the RPM, TPM and concurrency
  you want your tools to share, and coordinate provider cooldowns in one place.
- **Running a local model on a busy PC.** Bound simultaneous requests and queue
  excess work before it reaches the model server.

```text
Claude Code ─┐
Codex ───────┼──► DeskQuota on localhost ──► Your LLM endpoint
Pi / scripts ┘      quota · queue · cache · recovery
```

## Get started

### Native packages

Download an archive, its installer, and both matching `.sha256` files from
[the preview release](https://github.com/ryuwon-ai/deskquota/releases/tag/v0.1.0-preview.1).
You can transfer these four files to an offline PC for installation.

| Platform | Archive | Installer |
|---|---|---|
| macOS · Apple silicon | `llmgw-macos-arm64.tar.gz` | `install.sh` |
| Windows · x64 | `llmgw-windows-x64.zip` | `install.ps1` |

The published package version is `v0.1.0-preview.1`. For the latest cache and recovery
features described here, [build from source](#build-from-source).

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

<details>
<summary>Protocol and compaction support</summary>

Chat Completions, Messages and HTTP Responses are forwarded in their original
API format. The upstream supplies the model capabilities. `/responses/compact`
is unsupported; known-TPM inspection rejects opaque compaction inputs and input
estimates exceeding the configured budget.

</details>

## Choose your limits

| Setting | Meaning |
|---|---|
| Numeric RPM / TPM | Enforce a local rolling 60-second budget. |
| `unknown` | Add no local cap; let the provider enforce its allowance. |
| `unlimited` | Explicitly choose no local cap. |

Concurrency and shared cooldown still apply. A continuously refilling provider
allowance differs from a rolling window. Use a numeric cap when you want that
local budget; it cannot account for other PCs spending the same allowance.

<details>
<summary>Token accounting, cache and recovery settings</summary>

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

</details>

## Contributing

New client profiles, bug fixes and focused improvements are welcome.
[Open an issue](https://github.com/ryuwon-ai/deskquota/issues) with a reproducible
problem or a workflow you want to improve.

From `product/`:

```sh
cargo test --locked
cargo fmt --check
cargo clippy --locked --all-targets --all-features -- -D warnings
python3 -m unittest discover -s tests -p 'test_*.py'
```

For bug reports, include the client version, sanitized configuration, expected
behavior and a minimal reproduction. Remove keys, company URLs, prompts and
private source code before sharing.

## License

[MIT](LICENSE-MIT) or [Apache-2.0](LICENSE-APACHE), at your option.
