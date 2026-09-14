# Native final integration review

Verdict: **WITH FIXES**. One Important integration defect blocks the documented
fresh `SaveOnly` -> `connect` preview -> `--restart` -> client-patch flow. No
other actionable integration finding was identified in the bounded review.

## Important finding I1: a fresh SaveOnly config cannot reach connect preview

- Source: `product/src/cli.rs:503` reads the config-scoped data token before
  building or printing the client plan. A fresh SaveOnly apply returns without
  lifecycle activation or token provisioning (`product/src/setup/persist.rs:214-222`);
  token creation belongs to lifecycle start (`product/src/lifecycle/mod.rs:144-147`).
- Contract: `product/docs/client-compatibility.md:26-30` says preview alone
  changes no gateway or client file. Lines 58-66 say a stopped worker can be
  previewed and then started by repeating the command with `--restart`, and a
  setup SaveOnly result leaves selected clients pending.
- Direct reproduction: the packaged-runtime binary SHA
  `cf7c436c442c426a6e9a1d485a5261fe5e5a05c8b131b130e679e00eec2c646b`
  ran against the valid read-only `product/examples/fixture.toml`, whose
  config-scoped state directory and data token were absent. Pi 0.84.2 was
  version-probed with a temporary isolated HOME. `connect pi` exited 1, wrote
  zero stdout bytes, and reported the missing `data-token` before any preview.
  The state directory remained absent and the isolated client home contained
  zero files.
- Impact: the user cannot obtain the required preview hash, so the documented
  `--restart` apply command is unreachable after a fresh SaveOnly setup. The
  workaround `llmgw on` first contradicts the advertised connect-controlled
  activation sequence and is not disclosed in the command example.
- Required correction: preserve no-change preview semantics while separating
  the reviewed public client plan from token materialization. On explicit
  apply, establish authenticated readiness, read the resulting data token,
  revalidate the reviewed client/config inputs, and then write the client
  patch. Add a packaged-binary regression covering fresh SaveOnly, preview,
  exact hash plus `--restart`, readiness, and client write ordering.

## Review boundary

Actual code was traced across package documentation, config resolution, setup
apply, lifecycle token/readiness ordering, client preview/apply, disconnect,
status, and autostart intent. Root's final-document audit was reused for the
101-source/102-archive/five-member-package binding and zero broken packaged
links. Core HTTP, quota, fairness, streaming, and performance policy were not
redesigned or rerun.

No Cargo build, full test matrix, client inference, installer, actual user
configuration, credential, OS registration, login, PATH, model, paid API,
cloud, or VCS action occurred. Windows, Linux, macOS x64, clean-account,
real-login, signing/quarantine, low-end, and performance-advantage claims remain
unverified. `execution_finished=true`; `runtime_ownership_released=true`;
`owned_workers_remaining=0`.
