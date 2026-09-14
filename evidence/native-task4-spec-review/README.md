# Native Task 4 independent SPEC review

Verdict: **SPEC FAIL — one confirmed actionable finding.** This is the fresh SPEC review only; no QUALITY acceptance is implied. The `subagent-driven-development` spec-reviewer template was read and applied. The product, held source, held binaries, reference clones, and prior evidence remained read-only.

## 1. [P1] Native Claude directory overrides bypass the shared-project token restriction

**Source:** `product/src/clients/claude.rs:44–55`, especially the unconditional `Scope::UserPrivate` branch at line 55. CLI reachability is source-supported by `product/src/cli.rs:589–606`, which accepts the exact native directory, explicit home, native environment, or HOME-derived location; `cli.rs:385–412` passes this as the adapter request.

The project-local branch verifies Git tracking/ignore state. The native-directory branch simply chooses `<config_dir>/settings.json` and labels it user-private. Private filesystem permissions do not establish that the file is not shared through Git. Consequently a native override pointing at a repository's `.claude` directory writes the token to the shared `.claude/settings.json`, contrary to the explicit Task 4 requirement that this file must never receive the local token.

**Directly reproduced against the held debug rlib:**

1. Create an owned temporary Git fixture with `project/.claude/settings.json` containing `{}` and mode 0600.
2. Track this exact file with fixture-only `git init` and `git add`. `git ls-files --error-unmatch .claude/settings.json` confirms tracking; no commit is made.
3. Prepare Claude 2.1.63 with `project_local=None`, `config_dir=project/.claude`, and a synthetic local token, matching a resolved `CLAUDE_CONFIG_DIR` or `--client-config-dir` request.
4. `clients::prepare` succeeds, and `clients::apply_reviewed` succeeds using the exact returned hash.
5. Read back the owned fixture: the Git-tracked `.claude/settings.json` now contains the synthetic local token.

The adapter write itself is directly verified. The equivalent CLI override path is verified by source, **not** claimed as a fresh end-to-end CLI execution. Existing gateway readiness checks do not change this destination classification.

**Required correction:** establish the actual resolved target's user-private/nonshared scope for native Claude locations too; refuse a shared project `.claude/settings.json` before gateway activation or file mutation. Keep ordinary supported native overrides usable. A caller-selected directory name, the `UserPrivate` enum, and mode 0600 are insufficient proof. Add a regression for the tracked fixture and preserve normal private native-directory and explicit private/untracked project-local behavior. Parent-directory canonical aliases alone should not trigger a generic blanket ban.

**Evidence:** `spec_probes.rs`, `spec-probes-attempt1.log`; repeated with the expanded private harness in `spec_probes_expanded.rs`, `spec-probes-expanded.log`, and `spec-probes-expanded-execution.json` (actual test process exit 101). Only owned temporary fixtures and a held library were used.

## Other independent checks and their boundaries

- `before.json`: 1,066 held source/binary/earlier evidence entries match their supplied baselines. `after.json` records the final comparison.
- `source-audit.json`: independently compared the accepted Task 3 and held Task 4 manifests. Exactly 19 source files differ; the current 88 source entries also match the held source archive. Core config validation, fairness, quota/accounting, server/upstream hot-path source and Cargo dependency manifests are unchanged.
- Fresh held-rlib positive checks: private Claude unrelated env/header preservation and restoration; different desired gateway fingerprint produces a different composite hash; wrong hash and changed client snapshot are refused; both Pi and Codex allow same-journal reconnect, refuse a user-changed provider, preserve the changed resource at disconnect, and refuse re-adoption of the recreated old value after journal retirement.
- The provider check's first test expectation required a key conflict even for a newly created file. The accepted Task 3 contract instead reports that case in `preserved_created_files`. That review-harness assertion was corrected in a **new** source/binary, then the one affected test passed for both clients. Original failed source/logs are preserved; this is not a product finding.
- The initial symlink probe expected every symlinked `.claude` parent to be refused. It reached an ordinary private target inside the same fixture repository, with actual Git untracked/ignore checks performed on the canonical target. The blanket expectation conflicts with the accepted canonicalization contract and is **not** counted as a finding. Its failed assertion remains in both earlier logs for transparency.
- The three unique focused positive test functions passed after the above harness correction. No full 332-test rerun, benchmark/accounting matrix, cross-build, actual installed-client run, or gateway process was started by this reviewer.

## Requirement inspection map

| Requirement | Independently inspected evidence | Assessment |
|---|---|---|
| Native profiles, concrete dirs, version gate, managed/process conflicts | `clients/{mod,pi,claude,codex}.rs`, `cli.rs:516–671`, `tests/client_profiles.rs` | Formats and exact installed-version gates exist; native dir precedence resolves concrete paths. Shared Claude target bypass is finding 1. |
| Fixed root and protocols, one FIFO share, max 16 | `setup/mod.rs:340–372`, unchanged config validation; profile previews/docs | Existing fixed route writer is reused; no dynamic roots/classifier/hot-path changes. |
| Composite review hash before activation, desired authenticated readiness, failed activation preservation | `clients/mod.rs:238–350`, `cli.rs:311–512`, setup persist implementation; held focused failure fixtures | The client plan/hash is validated before gateway activation, existing setup/lifecycle helpers are reused, and client apply follows readiness. Fresh hash/snapshot probes passed. Full CLI activation failure evidence here is held implementer evidence, not a reviewer rerun. |
| Same-invocation first setup with fresh concrete preview and separate confirmation | `cli.rs:123–151,918–982`; held `native-setup-env-default-final3.json` and final PTY script | Source retains the reviewed in-memory plan/hash, proceeds after core readiness, honors native env default, and separates resource results. Held PTY observation reports the selected Pi path; no fresh reviewer PTY run. |
| Actual installed listing/selection/inference/tools/off/reload | `scripts/verify_clients.py`, `scripts/probe_pi.py`, final3 client JSON and corresponding logs | Driver checks real protocol/tool follow-up and loopback auth stripping. Held observations match the held release hash. Pi listing is verified; Claude listing not requested/discovery unsupported; Codex catalog unverified. Claude/Codex off controls are timeout with zero upstream, not self-terminated errors. These remain the implementer's bounded actual-client evidence. |
| Public preview/redaction/AuthForward and no broad network claims | `clients/mod.rs`, `cli.rs:315–320`, `docs/client-compatibility.md`; probe auth checks | Explicit public URL/model/root/protocol/scope/paths/source/hash shown; local secret is hidden; automatic Forward refused. Existing none/env hot path unchanged. |
| Narrow LocalDataTokenObject and ownership | `config_patch/{mod,apply,journal}.rs`, Pi/Codex adapters | Object-only validation, distinct hash type, opaque Debug and token-bearing privacy checks exist. No generic pruning/framework flag. Fresh ownership boundary tests passed for both adapters. |
| Capacity defaults and helpers | `clients/pi.rs`, `cli.rs:404–409`, preflight/docs | CLI does not copy core reservation fallback; Pi defaults 128000/16384 disclosed as unverified upstream capacity. Adapter reads settings as data; no config helper resolver added. |
| Claude generated public placeholder and stripping | `clients/claude.rs:4,6–12,64–80`, held `claude-installed-wire-final3.json` | Reviewed public literal is owned and previous settings are backed up through existing protected transaction. Held no-driver-auth control records exit 0 and two upstream requests; real keys were not used. |
| Final checks and claim limits | final3 fmt/check/clippy/full-test/release/client/PTY logs and JSON; source audit | Final logs exist for the held source identity; no reviewer full-suite rerun and no new performance/other-OS claims. |

## Reproduction invocation

The retained probe was compiled by `rustc --edition 2024 --test`, linking the immutable held library `product/target/native-task4/debug/deps/libllmgw-eba4665eb2ddaf45.rlib` and using its dependency directory read-only. Output went only to this review evidence directory. No Cargo operation or held/default target mutation occurred.

`spec_probes_corrected` contains the preserved blocker probe and positive probes. Run a named test to reproduce the blocker or ownership control. The parent-symlink test is deliberately retained only as a rejected initial review hypothesis, not a normative acceptance test.

The research tracking/report files were excluded from preservation because the parent owns them. Temporary fixtures were reaped by RAII/TemporaryDirectory; test processes exited. No real user client/auth/config, keychain/login, paid API, model server, installation, or OS registration was used.
