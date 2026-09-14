# Native Task 1 QUALITY re-review

**QUALITY PASS. The original Important [P2] finding is resolved. No new Critical, Important or Minor findings. Ready for the next task.**

**execution_finished: true; state: HOLD.** This is the same independent QUALITY reviewer's focused re-review. No product edits, builds, Git operations, release actions, provider/model calls, client settings or OS registration were performed.

## Held target and delta

- Release: `product/target/native/release/llmgw`, **8,697,040 bytes**, SHA256 `e16ba39c54fcec9bc5ce4fb4039d551dccdd5cdfd3bec9e0a7c412e58bd4364f`.
- HEAD archive: `product/artifacts/native-task1/quality-fix/source-hold.tar.gz`, SHA256 `d84f5b1ee94fc2a57a697b92007e2b2715168735a97ef33c2209f4e01f6d5c33`; all **61** archive members match current files.
- Compared directly to the prior SPEC-fix archive. Exactly three changed files: `src/lifecycle/mod.rs`, `tests/lifecycle_contract.rs`, and `docs/runtime-contract.md` (`source-delta.json`). Platform modules, dependencies, on ordering and child fingerprint check are unchanged.

## Finding resolution and code assessment

`product/src/lifecycle/mod.rs:198-209` now resolves the canonical instance, acquires its operation lock, reloads current configuration, rejects a newly resolved different source path, validates credentials, then performs stop/start. This closes the avoidable stale snapshot across lifecycle lock waiting that caused both original outages.

The added same-instance guard runs before token provisioning or stopping either worker. It is a narrow ownership check at the existing boundary, with no new framework, fallback, migration, IPC or supervisor. The existing child fingerprint guard remains responsible for rejecting later changes during drain/spawn. This does not promise atomicity against an uncooperative external editor across that entire interval.

The focused regression tests cover invalid and valid edits under an explicitly held operation lock, including whole identity and current fingerprint assertions. The Unix path-retarget regression checks both instances. They are appropriate tests of the observed failure and the new ownership boundary, rather than a broad style-driven expansion.

## Direct replay on this release

`probe-restart-race.py` retains the original reviewer probe's owned worker, existing operation lock, 500 ms wait/pending check, edit and unlock sequence. Only its location, expected assertions and obsolete recovery portion changed; the exact adaptation is in `probe-adaptation.diff`. The original failed script, results and report remain untouched one directory above.

| Original scenario / adjacent check | Direct result |
| --- | --- |
| Invalid TOML saved while restart waits for operation lock | PASS: exit 1 with configuration validation error; running state and entire old identity preserved; pending restart true |
| Valid comment saved while restart waits for operation lock | PASS: exit 0; new running identity; fingerprint matches latest bytes; pending restart false |
| Canonical config name retargeted to another owned running config while waiting | PASS: `configuration_path_changed` / exit 1; both full worker identities and running states preserved; owned loopback upstream received zero connections |

Results: `restart-race-results.json` and `path-retarget-results.json`. These three focused cases were executed by this reviewer; no full suite was repeated merely to refresh hashes. All calls were native lifecycle/control commands, with no data/model endpoint calls.

The implementer's current final lifecycle logs were read and each reports **25 passed / 0 failed**, debug and release. This is log inspection, not reviewer execution of those suites. Earlier 211-test suites, Pi/SSE/fairness/retry/footprint and the SPEC-fix 22-case suites retain their own source and binary associations; they are not new evidence for this revision. No performance superiority is asserted.

## Preservation, cleanup and limits

`identity-before.json` and `identity-after.json` match for **295** entries: current held source/artifacts, prior native artifacts, original reviewer evidence, preserved measured binaries and the 40 original pilot JSON files. All 40 pilot hashes also match the unchanged original pilot manifest. The native benchmark executable and old measured binaries were neither run nor rebuilt.

Every fixture command has a bounded wait. Public off succeeded for all owned instances; foreground restart commands were reaped; only owned temporary files were restored/removed. `cleanup-inventory.json` confirms **zero remaining native workers and zero `llmgw-quality-*` directories**. No test/build/process session remains active.

Windows/Linux runtime and whole-package Windows build acceptance remain unverified. Unchanged platform source and prior isolated Windows type/clippy evidence do not establish runtime support. The retarget test above is a direct macOS/Unix result. Setup/default command behavior, adapters, autostart, installers and login acceptance remain later tasks.

**Ready to advance: Yes, for the Native Task 1 quality gate.** The original outage is fixed and independently replayed with the same trigger. This HOLD returns control to the parent; it does not authorize Git/release or mark the whole native roadmap complete.
