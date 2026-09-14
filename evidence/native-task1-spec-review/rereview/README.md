# Native Task 1 SPEC re-review

**SPEC PASS. HOLD / execution_finished.** The previously reproduced P2 is fixed; the Windows fixture setup gap is corrected in source. No further SPEC finding arose from this bounded re-review. This is not a QUALITY verdict or Windows/Linux runtime acceptance.

Current held release SHA256: `af6b5b405cd29183482229cb42d217470a4004266c8f109e580ffaf5af83a033`. Current source archive SHA256: `38c1d079f973a43e18bb49bee046a3216619eba96184ad71547053ce5d15a68a`. All 61 archive entries match working files; the actual delta against prior hold is exactly lifecycle/mod.rs, lifecycle_contract.rs and runtime-contract.md. Platform, dependencies, control/server/admission and helpers remain unchanged.

## Direct verification

- Replayed the original independent counterexample and surrounding original probe unchanged, with only its repository-root lookup adjusted for the deeper evidence output directory. `probe-relocation.diff` records that sole one-line relocation. Prior evidence was not overwritten. A running worker plus malformed saved TOML now produces `restart_required` / exit1, while status retains `pending_restart=true` and the original worker. See `probe-results.json`.
- An additional focused current-release probe verifies the entire identity survives malformed `on` and rejected malformed `restart`. A synthetic nonblocking upstream listener observes zero connections. See `adjacent-results.json`.
- Two malformed NEW/stopped on calls produce no data/control tokens. Also verified a previously running, cleanly stopped instance whose owned token files were removed: malformed on does not recreate tokens and status remains stopped. Fresh/stopped parsing still precedes token provisioning.
- The replay also preserves concurrent on success, rejection of all six corrupted identity fields, stale nonce409, stale PID handling, unrelated owned helper survival, and inherited Mac ACL rejection before token writes.

## Source assessment

`lifecycle/mod.rs` now resolves canonical identity and serializes the operation before consulting authenticated running status; malformed disk contents are interpreted as a pending change without parsing a fresh launch. Only stopped state reaches `LoadedConfig::load` and then startup/token provisioning. `restart` still validates config and credentials before its stop call. No fallback, supervisor, unsafe-scope expansion or configuration migration was introduced.

The common stale-PID test now calls public on/off to create the state directory and valid protected runtime file. It reads and rewrites that existing file, changes only identity.pid/state, and verifies stopped plus stale_runtime=true before on. Existing-file truncation does not create a new inherited Windows DACL. Thus the previously identified test setup obstruction is removed without weakening production ACL rejection. Actual Windows execution is still unverified, and later platform acceptance must run this case.

Read the retained malformed RED (1 pass / 1 fail), GREEN and final22-case debug/release records as implementation evidence. This reviewer did not rebuild or rerun broad Rust suites, Pi, SSE, fairness/retry, benchmarks or footprint. Prior211-suite and earlier drain/probe evidence retain their prior-hold identity and are not asserted as execution on this new release.

## Preservation and cleanup

`identity-before.json` and `identity-after.json` verify every reviewed held source/helper/binary/archive and recorded prior artifact remains unchanged. `summary.json` records 78 checked entries, zero remaining native executable processes and zero fixture directories. Both probes close their worker port and remove owned temporary paths; unrelated helper survival is verified before its own explicit cleanup. The replay's ACL-case final off1 is expected because its state is deliberately rejected and no worker exists then.

Only files under this re-review evidence directory were created. No product edit, build, dependency install, Git operation, real provider/model request, real client configuration change, autostart or OS registration occurred. Execution has ended and the current product remains on HOLD for the parent's next review stage.
