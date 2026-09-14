# Native Task 1 independent QUALITY review

**QUALITY CHANGES_REQUIRED — not ready for Native Task 2.**
**execution_finished: true; state: HOLD.** One Important finding; no Critical or Minor findings. Product source and binaries were not edited or rebuilt. No Git operation or release is authorized by this review.

## Review identity and scope

Reviewed the root/research AGENTS.md, README/HARNESS/work-items entry points, both approved 2026-09-12 specifications, Native Task 1 plan, lifecycle preflight/platform decisions, and current SPEC re-review. The three working directories have no `.git`; the comparison uses filesystem archives and hashes.

- BASE: Core Task 8 `evidence/product-task8-quality-fixed-source.tar.gz`, SHA256 `c4bb57f143bc03e1c23fb51fee5bedba4d89ca01106f5f09106ec318a383cb29`.
- HEAD: `product/artifacts/native-task1/spec-fix/source-hold.tar.gz`, SHA256 `38c1d079f973a43e18bb49bee046a3216619eba96184ad71547053ce5d15a68a`. All 61 archived files match current bytes; `baseline-to-head.diff` includes product and four research helpers.
- Direct probes: current release `product/target/native/release/llmgw`, SHA256 `af6b5b405cd29183482229cb42d217470a4004266c8f109e580ffaf5af83a033`, 8,695,680 bytes. No benchmark executable was used or rebuilt.
- Independently read lifecycle/identity/process/Unix/Windows modules and connected CLI/config/server/control/admission changes. Checked helper changes against the baseline.

## Strengths (source-supported unless noted)

- Instance identity and control authentication are separate from PID ownership. The stable lifetime file lock is never unlinked; conditional nonce stop prevents a health/stop restart crossover. Operation locking stays outside the lifetime lock.
- Child spawn uses the current executable and argv, detached stdio, and bounded fixed diagnostics. Secret and runtime reads have explicit byte bounds; no new scheduler, supervisor, autostart registration, or hot-path polling was introduced.
- Config bytes remain immutable per worker; startup quota hold/shared cooldown are observable independently of readiness. The drain retains bounded control ingress without extending its deadline.
- Protected file access rejects existing weak permissions instead of repairing them. Windows unsafe code is contained in one module with owned handles/descriptors, bounded token/ACE/SID storage and cleanup. The reviewed CreateFile sharing/non-inheritance and existing-descriptor rules match the selected [Microsoft API contract](https://learn.microsoft.com/en-us/windows/win32/api/fileapi/nf-fileapi-createfilew); descriptor ownership and token buffer sizing were checked against [GetSecurityInfo](https://learn.microsoft.com/en-us/windows/win32/api/aclapi/nf-aclapi-getsecurityinfo) and [GetTokenInformation](https://learn.microsoft.com/en-us/windows/win32/api/securitybaseapi/nf-securitybaseapi-gettokeninformation). This is source/API review, not Windows execution or a proof against hostile same-user filesystem races.

## Important finding

### [P2] Refresh restart configuration after acquiring the operation lock

**File:** `product/src/lifecycle/mod.rs:198-205`, especially config load at 199 before the lock await at 201.

`restart` snapshots configuration before it can wait up to 17 seconds for another lifecycle operation. After the lock is acquired it validates credentials against that old snapshot, stops the healthy worker, and starts a child using the old fingerprint. The child reads current bytes at `mod.rs:210-214`; any edit during the lock wait makes that launch fail. Even a valid comment-only save therefore leaves the gateway stopped and cancels/drains its existing work unnecessarily. Malformed TOML written during the wait also bypasses the intended validation-before-stop preservation behavior.

**Direct counterexample:** `probe-restart-race.py` starts an owned worker, holds its existing operation lock, spawns `restart`, confirms it is waiting, changes the config, then releases the lock. Both the malformed edit and the valid comment edit produce exit 1 / `worker_start_failed`; the old worker exits and status is `stopped`. A later explicit `on` recovers. Results are preserved in `restart-race-results.json`; this was run against the exact HEAD release above, not inferred from code alone.

**Required correction:** resolve the instance sufficiently to acquire its operation lock, then read and validate the current canonical configuration and credentials under that lock before stopping the old worker. Preserve the existing child fingerprint guard. Required regressions: malformed edit during the lock wait returns a validation failure while retaining the old complete identity; a valid edit during that wait restarts successfully with the latest fingerprint. This recommendation does not require an atomic guarantee against an uncooperative editor throughout drain/spawn, config IPC, or a supervisor redesign.

The normal invalid-before-restart path already preserves the old identity; the defect is the avoidable stale snapshot across lifecycle serialization, not lack of configuration validation in general.

## Direct execution and limitations

| Check | Result |
| --- | --- |
| Invalid config edit while restart waits on operation lock | Reproduced defect: stopped after failed restart |
| Valid comment edit while restart waits on operation lock | Reproduced defect: stopped after failed restart |
| Invalid config already present before restart | PASS: validation failure, entire old identity preserved |
| 8,193-byte runtime record | PASS: status/off reject quickly, original worker preserved after record restoration |
| FIFO data-token in stopped instance | PASS: rejected in 4ms, no blocking read, status remains stopped |
| Unrelated owned helper | PASS: survived all product commands, then explicitly terminated/reaped by fixture |

The two Python scripts use only current release CLI commands, owned temporary files/processes and loopback configuration. No data/model endpoint was invoked, no real provider was called, and no client/login/user config or cloud resource was changed. These are behavior probes, not latency or efficiency measurements. No additional cargo build or full suite was necessary. Existing 22-per-profile current lifecycle results and older 211-per-profile results remain their original authors' evidence; neither was relabeled as reviewer execution.

Windows/Linux runtime, Windows full-package build, installation and login acceptance remain unverified. The isolated Windows module type/clippy result does not establish whole-package Windows support; the missing SDK headers are an environment gap, not an additional code defect. Setup/default command behavior, adapters, autostart and installers remain later tasks.

## Preservation and cleanup

`identities-before.json` and `identities-after.json` match for all 61 captured source/artifact entries. `held-artifact-audit.json` independently matches all 78 SPEC-reference entries including current/prior source and binary identities. All 40 original pilot run JSON hashes match the original pilot manifest (`pilot-preservation.json`); no pilot was rerun. All 61 HEAD archive members also match current files.

Both probes restore config/record as needed, invoke public off, and remove only their own temporary directories. The unrelated helper was explicitly reaped. `cleanup-inventory.json` records zero remaining native workers and zero `llmgw-quality-*` temporary directories. No continuing build, test or process session remains. Original failing observations are preserved; no product fix was attempted in this review.

**Ready to advance: No.** Fix the one observed restart serialization bug, preserve these counterexamples, then request quality re-review on the new held source/binary. This decision concerns Native Task 1 only and neither authorizes Git/release actions nor completes the native roadmap.
