# Backfill experiment design review

2026-09-13, independent `backfill_design_review` agent, read-only source/spec/plan review. Verdict: PASS for benchmark-only experiment after corrections; not runtime or performance evidence.

The reviewer found that equality at `T == now + WINDOW` can allow a new request to expire and recharge late usage before the protected head enters. The spec now requires strict inequality. Known-TPM active started entries with absent or expired TPM intervals must also decline, including metadata. Use the ledger's normalized time, preserve unstarted reservations, and reserve two entry/ID positions cumulatively. Positive seed must be completed before the barrier trace.

Implementer subsequently identified the same absent-expiry risk in a newly admitted zero-token candidate. The spec additionally rejects that case when TPM is known. This narrowing and its runnable counterexample are subject to implementation review.

No reviewer files, builds, processes, or Git state were changed. The existing 8192-entry/16-root scan bound does not prove low overhead; measurement remains required.
