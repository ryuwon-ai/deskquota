# Task 4 results — installed Pi first E2E

Date: 2026-09-12

## Scope implemented

- Added a self-contained Python development probe and a synthetic Pi models
  template. The Rust runtime and its dependencies were unchanged.
- Isolated Pi in a temporary agent directory and empty working directory with
  offline startup, discovery and sessions disabled, telemetry and both retry
  layers disabled, and either no tools or the exact `read` allowlist.
- Exercised `/r/pi-work/v1/chat/completions` through a foreground gateway to a
  fixture at `/team/v1/chat/completions`. The fixture required the synthetic
  end-to-end header and observed that local tokens and Authorization were not
  forwarded.
- Verified one tool-free completion request and an exact two-request read tool
  loop. The second request carried the fixture's tool-call ID and exact
  temporary-file content, while Pi JSON events contained one matching read
  start and one non-error read result.

## TDD behavior evidence

The first gateway-off exploration used Pi text mode and observed Pi subprocess
exit 1 with zero upstream attempts. That mode was not retained as the comparable
RED because positive verification uses JSON events. The final gateway-off RED
uses the exact same Pi JSON mode, inputs, and positive acceptance predicates as
the gateway-on run; `--gateway-off` changes only whether the gateway starts.

```text
python3 scripts/probe_pi.py --binary target/debug/llmgw --gateway-off --output artifacts/pi-e2e-gateway-off.json
exit 1
completion: passed=false, Pi exit=0, assistant failure=true, marker=false, upstream attempts=0
read_tool: passed=false, Pi exit=0, assistant failure=true, marker=false, upstream attempts=0
```

Installed Pi 0.84.2 reports assistant transport failure in its JSON event stream
but exits 0. The probe therefore rejects assistant error/aborted events and
missing output/wire assertions independently of the Pi process exit code. The
negative artifact records `passed=false` and `expected_failure_observed=true`.

The gateway-on GREEN uses the same assertions:

```text
python3 scripts/probe_pi.py --binary target/debug/llmgw --output artifacts/pi-e2e.json
exit 0
completion: passed=true, Pi exit=0, upstream attempts=1
read_tool: passed=true, Pi exit=0, upstream attempts=2
```

## Direct verification boundary

The artifacts identify installed Pi 0.84.2 and its selected source hashes,
reference clone 0.85.1 at
`f3c672245d25ef2283ffc0d9cdec8a5482651103`, macOS arm64, the exercised OpenAI
Chat Completions SSE/HTTP/1.1 protocol, harness/template hashes, and the debug
gateway binary hash before and after execution. They contain no raw prompt,
request body, credential, stdout, or stderr.

This is deterministic installed-agent/mock compatibility evidence. It is not a
real model/API call, quality or latency benchmark, general Pi filesystem
sandbox, other-provider result, or Windows/Linux result. Quota, fairness,
gateway retry, and resource benchmarking remain Task 5 and later work.

## Final verification

The final verification ran `python3 -m py_compile`, `--help`, a missing-Pi
preflight, the gateway-off RED command, the gateway-on GREEN command, artifact
contract assertions, and an owned-process check. The missing-Pi case returned
argparse exit 2 before creating its requested output. The artifact checks
confirmed exact attempts, routes, custom-header receipt, credential stripping,
tool event counts, next-turn tool result, binary immutability, matching source
hashes, and absence of retained raw values. The first process-list assertion
matched its own shell command text; a corrected check limited to actual
`llmgw`/`node` executables and found no owned-process leftovers.

Final fingerprints used for review:

```text
scripts/probe_pi.py                 3339124308fc5f589dc73260146f02ab9186fec6fb454163463b80542444ff68
tests/test_pi_probe.py              a57a770723dfbf5071fa1640584d97a5eaef58596b73dd3b0662e23f45bc9d11
tests/fixtures/pi-models.json       44228610dac6b3dba5d86a5e9b8a04239ca051a4d5f2f840641cfae1ef5138ad
target/debug/llmgw                  1149c72214dcb0121ef3dc92d023e424f9d9c8159b51bb0cc34fbd2bae51b771
```

The gateway binary hash was identical before and after both final probes. No
Rust source, Cargo manifest, or lockfile changed, so the already-established
83-test Rust baseline was not repeated for these Python/documentation-only
changes.

## Quality review correction

The first independent quality review reproduced an owned-child leak when
`KeyboardInterrupt` escaped `communicate()`, and found that an absent optional
reference clone was reported as a known version difference. The RED in
`task4-quality-fix-red.log` uses a real short-lived Python process: interruption
propagated, but the child remained alive until the test's exact-handle cleanup.
The same RED observed `differs_from_installed=true` with no clone version.

`run_pi` now has unconditional ownership cleanup. It terminates or kills and
reaps a still-running child, closes both captured pipes, and does not catch the
original interruption. The clone probe returns immediately when its manifest
is absent, records a commit only after the clone is confirmed as the Git root,
and leaves the comparison null until both versions are known.

`task4-quality-fix-green.log` records four passing standard-library regression
tests for interruption, timeout, normal exit, and absent-clone provenance. The
timeout and interruption tests use real sleeping Python children and exact
handles; no process-name or global kill is used.

The post-fix final run repeated the same JSON-mode gateway-off and gateway-on
commands. The negative probe exited 1 with both Pi subprocesses reporting JSON
assistant failure, Pi exit 0, and zero upstream attempts. The positive probe
exited 0 with one completion attempt and two read-tool attempts. Artifact
contract, secret/raw-value absence, source/hash matching, and owned-process
cleanup checks passed. The debug binary remained unchanged at the hash above.
