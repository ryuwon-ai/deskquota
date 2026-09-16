# Optional BPE packaging

2026-09-16. Design for independent review before implementation. Parent selected the small default executable plus an explicit `bpe` Cargo feature under the user's approved lightweight/native scope. This document records source inspection; it is not a successful build or measured size reduction.

## Problem and scope

The held macOS release is 59,921,008 bytes although the default `utf8_bytes` estimator never initializes a vocabulary. BPE is useful when its encoding matches the upstream, but bytes-only users currently install both vocabularies. Keep a small default executable and retain the current BPE implementation as an explicit build option.

The user-facing command remains `llmgw`: the same `setup`, `on`, `status`, and `off`, state paths, config format, installer, and one native executable. Docker/WSL, runtime Rust, downloads at startup, plugins, sidecar files, extra processes, and a second service are out of scope. A publisher may distribute a separately identified BPE-enabled archive through the same existing installer. There is currently no public release URL, and this change does not invent one.

The installer does not guess the user's model encoding, change an estimator, or automatically replace an artifact. A bytes-only build encountering an explicit BPE configuration reports the missing build capability before startup or saving changes. It never silently falls back to byte estimation. Switching to the default build with existing BPE configuration therefore requires an intentional choice: retain a BPE-enabled build or explicitly edit the estimator. The gateway must not perform that edit itself.

## Inspected evidence and alternatives

Directly inspected the held binary with `size -m` and the existing Cargo build outputs, without modifying or executing the binary:

| Item | Bytes |
|---|---:|
| Held macOS executable | 59,921,008 |
| `__TEXT.__const` | 48,444,288 |
| `__LINKEDIT` | 2,801,664 |
| Generated `bpe_cl100k_base.dict` | 15,027,980 |
| Generated `bpe_o200k_base.dict` | 32,176,874 |
| The two used vocabulary files combined | 47,204,854 |

Binary inspected: `/Users/ryuwon/Library/Caches/deskquota-queued-cache-20260916/llmgw-final`, SHA-256 `e28f3322ff1b0dce11941b27d45b771a0a247437686b9d39493af88839421b77`. The two dictionary sizes come from `llmgw-cargo/release/build/bpe-openai-6035b29d4baf9c4c/out`; the other existing build output directory has identical sizes. These sizes explain the dominant cost but do not prove an exact future reduction.

`bpe-openai=0.3.1` has no feature section or per-vocabulary feature. Its `src/lib.rs` uses `include_bytes!` for generated serialized dictionaries and `LazyLock` initialization; `build.rs` prepares three dictionaries, while this product calls only cl100k/o200k. Both product modes are runtime reachable. Selected-only initialization is already implemented and must remain.

| Option | Decision |
|---|---|
| Existing dependency configuration to remove unused encodings | Not available in installed 0.3.1; do not claim it has a feature that it lacks. |
| `strip = "symbols"` alone | Cannot remove the dominant runtime dictionary data. Cargo describes stripping symbols/debuginfo, and the observed constant section is already about 48.4 MB. Exact stripping savings are unmeasured. Do not treat Windows/macOS savings as interchangeable. |
| LTO or size optimization profile alone | May change code size/build time; it cannot simply remove two referenced vocabularies while retaining both selections. Do not alter performance-sensitive release optimization, panic semantics, and BPE packaging together. |
| Default-on optional BPE | Preserves all choices in a single standard artifact but leaves default distribution size essentially unchanged; does not solve this scope. |
| Default-off `bpe` feature | Selected. Uses Cargo's existing optional dependency mechanism and preserves an offline BPE-enabled executable for users who choose it. Adds two compile/test capabilities, not two services. |
| Runtime downloads, custom compressed vocabulary loading, tokenizer fork/vendor | Rejected: new packaging/failure/initialization behavior and maintenance burden without need for this bounded change. |

Cargo's [profile documentation](https://doc.rust-lang.org/cargo/reference/profiles.html) explains symbol stripping and LTO's longer linking cost. Its [feature documentation](https://doc.rust-lang.org/cargo/reference/features.html) provides optional dependencies and `dep:` feature syntax. The inference above combines those documented mechanisms with the actual binary sections and installed library source; no LTO/strip benchmark was run.

## Minimal implementation contract

1. Add exactly one product feature: `bpe = ["dep:bpe-openai"]`. Mark the existing pinned dependency optional. Do not enable the feature by default. `bench-harness` remains independent; `--all-features` is a valid combination.
2. Keep `InputEstimator::{Utf8Bytes, Cl100kBase, O200kBase}` and their existing serialized names in all builds so configuration can produce an actionable unsupported-capability error. Keep current overhead defaults and validation. BPE calls compile only with the feature; a bytes-only `estimate` call for a BPE variant must fail explicitly through its existing optional-result boundary rather than return a byte count.
3. Have one small estimator-capability predicate shared by config validation, typed-config startup validation, and the setup mode list. A plain const method is enough; no registry, trait, dynamic loader, or factory.
4. TOML validation rejects any BPE selection when capability is absent, including when TPM is unknown/unlimited. Error identifies the requested estimator and explains that the executable must be built with `--features bpe`. Do not include prompt/config secrets in the error.
5. `server::validate_start_config` rejects typed `Config` values selecting unavailable estimators as well. All public startup paths (`spawn`, benchmark, and internal fixture entry) converge there before listening or initializing worker state. CLI lifecycle preflight must observe the error before replacing/stopping an existing worker. Do not rely only on TOML parsing or known-TPM preparation.
6. Setup offers only compiled estimators. With the default build there is no BPE vocabulary selection or framing-overhead question; the persisted result stays `utf8_bytes` with zero overhead. The BPE-enabled wizard keeps the existing three choices and explicit upstream-encoding warning.
7. Existing setup validation already serializes and reparses the draft before `render_config` and persistence. Reuse that path: a programmatic BPE draft in the default build fails before saving, and existing user bytes/comments are not rewritten. Do not add an alternate loader or migration for an existing unsupported BPE config.
8. BPE-enabled requests keep the exact existing count implementation, warmup behavior, request bytes, output cap, observed usage, and cache key/eligibility. No tokenizer substitution or early-termination optimization in this change.

The existing `lifecycle::off` and `status` use the config-scoped identity/worker handshake without parsing the saved configuration. Preserve this: a default build must still be able to inspect and stop its existing BPE-configured worker. `restart_inner` parses the new configuration before stopping the old worker, so config validation naturally protects it. No alternate configuration loader is needed.

## Expected changed files

| Files | Purpose |
|---|---|
| `product/Cargo.toml`, `product/Cargo.lock` only if Cargo requires it | Optional dependency and one explicit feature. Keep version pins. No new dependency. |
| `product/src/input_estimate.rs` | Compile-gated BPE calls and shared capability check. |
| `product/src/config/validate.rs` | Reject unsupported explicit estimator before config acceptance. |
| `product/src/server.rs` | Typed-config and lifecycle-preflight capability check with a clear startup error. |
| `product/src/setup/prompts.rs` | Only compiled mode choices; preserve existing default and BPE warnings where applicable. |
| `product/tests/input_estimation_contract.rs` | Shared defaults/invalid fields; feature-on exact vectors and wire/usage checks; feature-off parse/startup rejection. |
| `product/tests/setup_contract.rs` | Keep unrelated setup tests active in both builds; feature-on preservation test plus feature-off unsavable draft/unchanged original config evidence. |
| `product/src/protocol/request.rs` test module only | Gate existing BPE-specific unit test; existing non-BPE endpoint validation stays active. |
| `README.md`, `README.ko.md`, `product/docs/installation.md`, `product/docs/runtime-contract.md` | Default capability, explicit build option, measured size and unsupported-config behavior. |

`setup/mod.rs`, `setup/document.rs`, `setup/summary.rs`, `cli.rs`, installers and packaging scripts should not need production changes: serialization names and archive member layout remain unchanged. If implementation finds a necessary extra edit, state the reason instead of adding compatibility code. `product/README.md` may receive one sentence if needed for packaged users; it already states one executable and no runtime Rust requirement.

No existing CI workflow was found in the current repository file inventory (reference clones excluded). This does not add a CI platform or claim hosted CI coverage. Record both local compile modes and repeat that matrix wherever a release pipeline is introduced.

## Acceptance and evidence

First demonstrate the unsupported-feature case fails under the current source (current code accepts BPE; the desired default-off contract requires rejection). After implementation:

- Default `cargo +1.88.0 test --locked --all-targets --features bench-harness` passes with bytes coverage intact and explicit BPE parse/draft/typed-start rejection. Unsupported startup must fail before port bind and unsupported persistence before file replacement.
- Native lifecycle checks retain the ability to `status`/`off` with an existing BPE configuration while a default build rejects a new startup/restart. A restart rejection must not stop a healthy existing worker.
- `cargo +1.88.0 test --locked --all-targets --features bench-harness,bpe` passes including the original vectors, output/request/usage preservation, endpoint bounds/unmetered bypass, and setup preservation checks. Do not disable all input-estimation/setup tests to make the default build pass.
- Formatting and Clippy run for both modes. Release builds are recorded for default and `--features bpe`; hold each executable under a distinct evidence filename without changing its installed command name.
- Use `cargo tree`/build records to verify BPE is absent from the default runtime dependency graph and present in the enabled graph. A lockfile retaining optional dependency records is expected and is not proof of linkage.
- Measure both executable sizes and archive sizes, plus paired no-wait behavior/idle RSS and native lifecycle for default. Compare against the frozen previous binary without concurrent compilation. BPE-enabled regression checks reuse the existing estimator fixture and preserve original body/usage; do not attribute optional bytes-only savings to the full BPE build.
- The default macOS executable should be less than half the held 59,921,008-byte baseline. This is a validation gate, not a reported result. If not met, inspect the actual link/build graph rather than silently lowering the target.
- Windows must build both modes with the existing GNU environment and execute applicable default/feature-on contract checks in the restricted runtime PATH. Report Windows execution and size only after actual evidence. Linux remains unverified until run on Linux.

The maintainer now carries two feature combinations through test/release verification. Users selecting BPE need the appropriately identified native archive (or an explicit `cargo build --locked --release --features bpe` during development), while ordinary bytes users retain the normal build and one-command lifecycle. This is the deliberate tradeoff for removing unused vocabularies from the default executable.

## Non-goals

This is a packaging change. It does not itself improve the 38-second quota-bound workload, auto-detect tokenizers, make serialized JSON an exact provider token count, reduce selected-BPE RSS, change quota windows/accounting, or create a new retry policy. Those require separate evidence and decisions.
