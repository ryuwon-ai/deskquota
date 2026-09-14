# Native final integration rereview — I1

**Verdict: PASS_WITH_PLATFORM_LIMITATIONS.** No remaining actionable finding was found in the bounded I1 delta or its adjacent setup/client lifecycle seams.

## Directly executed

The frozen macOS arm64 binary `16de493c…10fa7` ran in an isolated HOME with owned loopback ports. A real PTY setup **Save only** created distinct 64-byte data/control tokens under 0700/0600 permissions while status remained stopped and worker count remained zero. Pi preview produced a 64-character reviewed hash without changing client bytes or token identity. Reusing that exact hash with `--restart` established the desired authenticated fingerprint before the client patch. Disconnect preserved the user's JSON values and removed the owned llmgw provider; authenticated off returned stopped. Upstream calls, leaked protected token values, remaining workers, and remaining fixture paths were all zero.

The Pi executable in this rereview was an isolated synthetic `--version` fixture reporting 0.84.2. This proves the actual llmgw CLI integration path, not execution of an installed Pi client. The installer-owned client run remains separate.

## Source and retained evidence

The setup apply now calls the common config-scoped credential path under the operation lock, rechecks the exact saved fingerprint, provisions only while the worker lock is available, and validates existing protected credentials in both stopped and running branches. Setup publishes config and pending metadata before initialization and reports that boundary explicitly on failure; Save only returns before worker activation. The client path still binds the preview hash to the gateway fingerprint/client snapshot and reaches `apply_reviewed` only after fresh authenticated readiness.

All 101 current source rows match the frozen manifest, and the 101-member source archive matches it exactly. The release binary, artifact HOLD, original integration review HOLD, and its size correction retain their expected SHA-256 identities. The parent-retained Rust result is 370 passed / 0 failed; Cargo was not rerun here.

The first reviewer attempt used an overly strict byte-for-byte JSON whitespace assertion after disconnect. It completed authenticated cleanup with no worker or scratch left. The final run checks the documented owned-key and user-value restoration contract and passes.

## Limits

This rereview does not add Windows, Linux, macOS x64, real-login, signing/notarization/quarantine, clean-account, performance-percentile, low-end-memory, or installed-client evidence. Those remain platform or later-run limitations rather than new defects.
