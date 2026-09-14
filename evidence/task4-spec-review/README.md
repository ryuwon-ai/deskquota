# Core Task 4 independent specification review

Verdict: **SPEC PASS**. Reviewed 2026-09-12. No product file was modified.

The reviewer read the standalone probe, synthetic models template, runtime contract, prior research Pi probe, and applicable workspace instructions. This review applies to script SHA-256 `52a806e7c6da8417ec43353a3ebddd3164db9e5e8ac2b56fe9992ca64407cfa7`, template `44228610dac6b3dba5d86a5e9b8a04239ca051a4d5f2f840641cfae1ef5138ad`, and debug binary `1149c72214dcb0121ef3dc92d023e424f9d9c8159b51bb0cc34fbd2bae51b771`.

## Direct independent execution

Both commands ran from `product/`, with outputs redirected only to this review evidence directory. A reviewer-owned temporary parent was supplied through `TMPDIR`; after each command its contents were empty.

```sh
python3 scripts/probe_pi.py --binary target/debug/llmgw --output ../evidence/task4-spec-review/positive.json
python3 scripts/probe_pi.py --binary target/debug/llmgw --gateway-off --output ../evidence/task4-spec-review/gateway-off.json
```

| Observation | Gateway on | Gateway off |
|---|---|---|
| Probe exit | 0 | 1 |
| Completion passed / upstream attempts | true / 1 | false / 0 |
| Read flow passed / upstream attempts | true / 2 | false / 0 |
| Pi subprocess exits | 0, 0 | 0, 0 |
| Assistant failure events | absent in both | present in both |
| Final output markers | present in both | absent in both |
| Read start / end count | 1 / 1 | 0 / 0 |
| Owned gateway cleanup | control stop in both | no gateway started |

The positive request observations verified the configured root-to-upstream route, model, stream mode, custom header, stripped local tokens and Authorization, and exact attempts. The read flow additionally verified the single matching read invocation, exact temporary path arguments, non-error result, exact synthetic content, and matching tool-call ID plus content in the next upstream request. Independent assertions checked these fields explicitly rather than accepting only the artifact's `passed` field.

All Pi processes exited normally. The owned temporary parent was empty after each run, and a subsequent process check found zero matching `node`/`pi`/`llmgw` processes bearing the reviewer-owned temporary prefix. Script, template, contract, and binary hashes were unchanged across execution. The independent comparison of the 28 Task 3 baseline files found only the intended runtime-contract change; Rust and Cargo files were unchanged. Existing Rust tests were not repeated in this Python/documentation review.

## Source-supported acceptance and isolation

- `scripts/probe_pi.py:199-278` validates wire fields before issuing fixed responses. It emits only the single deterministic read call and emits the final tool marker only after receiving the matching actual tool result.
- `scripts/probe_pi.py:471-499` checks one start and end, exact read name/ID/path, a non-error end, and exact synthetic result text.
- `scripts/probe_pi.py:502-583` uses temporary agent/config/fixture directories, an initially empty cwd, private synthetic tokens, loopback listeners, a restricted environment, offline startup, no discovery/context/session, disabled telemetry and both retry layers, and either no tools or only read. Installed Pi's CLI source also recognizes the relevant offline/discovery/context switches.
- `scripts/probe_pi.py:412-421` bounds Pi execution and terminates/kills only its owned process when needed. `:652-659` cleans up the owned gateway and fixture; no global process matching is used for mutation.
- `scripts/probe_pi.py:627-635` uses the same acceptance expression for both modes. The only execution branch on `gateway_off` is gateway startup at `:541`. The aggregate predicate at `:786-792` is also shared; the negative flag does not hardcode failure. Other negative branches add mode labels and observations only.
- `scripts/probe_pi.py:455-468` rejects assistant error/aborted events even if a prior marker appeared. Installed Pi JSON exit 0 is not accepted by itself.
- The probe imports only Python standard-library modules. Reference-clone inspection supplies optional provenance metadata; no research script is imported as a production dependency.

## Artifact and claim limits

Independent schema/value checks covered both new review JSON files and both implementer JSON artifacts. No raw prompt, messages/body, credential values, exact synthetic content, tool path/arguments, stdout, or stderr was retained. Only required structural observations, booleans/counts, policy names, code paths, versions, and hashes are present. Synthetic fixed values remain in source code as expected.

Directly observed scope: installed Pi **0.84.2**, reference clone **0.85.1** at `f3c672245d25ef2283ffc0d9cdec8a5482651103`, **macOS 26.5.1 arm64**, OpenAI Chat Completions SSE over HTTP/1.1. This is deterministic agent/mock compatibility. It does not prove real model quality, performance, other providers/protocols, Windows/Linux behavior, or a general filesystem sandbox. Only the harness-owned synthetic file was requested by the fixture; Pi's read tool itself is not path-sandboxed. No real LLM/API, user configuration change, OS service, Git mutation, or product source edit was performed.

Evidence: `positive.json`, `gateway-off.json`, `execution-summary.json`, `independent-assertions.json`, and `baseline-comparison.json`.
