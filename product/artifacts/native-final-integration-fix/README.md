# Native final integration I1 fix — FINAL_HOLD

Fresh setup SaveOnly now initializes the existing protected, config-scoped data
and control token files under lifecycle operation and worker locks. It starts no
worker and writes no client or OS registration. Preview and cancellation remain
write-free. Existing tokens are validated and remain byte-identical; missing or
invalid state beneath a running worker is never repaired.

The final Rust suite has 370 passed, 0 failed across
17 result rows. Focused actual-binary tests exercised
Pi 0.84.2, Claude Code 2.1.63, and Codex 0.154.0 from fresh setup SaveOnly through
preview hash, reviewed restart, authenticated desired-fingerprint readiness,
client apply, disconnect, and authenticated off using isolated temporary homes.

One retained harness failure expected the Codex profile at `.codex/config.toml`
instead of its verified `.codex/llmgw.config.toml` path. The panic orphaned one
proven-owned debug worker after the old Fixture removed state; the exact process
received bounded SIGTERM and cleanup is recorded. The final harness preserves
state from activation until authenticated off plus parsed stopped status.

No performance/accounting matrix, real upstream, real client profile, login,
model process, OS registration, VCS, or prior artifact was changed. Final
packaged-client observations remain for the parent-coordinated rereview.
