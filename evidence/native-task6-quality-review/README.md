# Native Task 6 code quality review

Verdict: **CHANGES_REQUESTED** after the independent SPEC PASS. Two P1 quality
findings remain. No product source, package, held target, user configuration,
authentication, startup registration, profile, registry, keychain, model,
network service, or VCS state was changed by this review.

## Findings

1. **P1 — `product/scripts/package_native.py:40,72,79`: returned binary
   identity can describe bytes other than the packaged member.** The archive is
   built from one `binary.read_bytes()` result, but `binary_sha256` reopens the
   source path after archive creation. The isolated probe deterministically
   changed that path between the reads: the archive retained SHA-256
   `66bb09e4699bf6d7b7033b395b33bdc2d4152f592ee2150fb996275683f7b6be`,
   while the returned identity was
   `b01e58dd4236667fb0254a1b9e67bde1c972b9a6fabd3d6e3333d7d6b71824a5`.
   Hash the already captured `contents["llmgw"]` bytes so the reported identity
   names the member actually verified in the tar.

2. **P1 — `product/packaging/install.ps1:97-115`: recovery can delete a
   concurrently written target.** After successful `File.Replace(B, target,
   backup)`, another writer can replace target B with C before the staged-hash
   check. The catch sees C is not prior A, then lines 112-115 unconditionally
   delete C and move backup A into place. This contradicts the documented
   no-concurrent-writer and never-blindly-overwrite recovery boundary. Preserve
   all recovery files and return `replace_failed_recovery_unconfirmed` when the
   current target has an unknown identity. PowerShell was unavailable, so this
   is an exact source-supported interleaving, not Windows runtime proof.

Machine-readable details, root causes, reproductions, impact, and minimal
directions are in `findings.json`.

## What passed

- The current 101-file source manifest and 102-member source archive have zero
  live/archive drift. Their SHA-256 values remain
  `f228071555f45c1774b7c2f8e059f2adce6eef77c10674191e04713feb0d4c3f` and
  `b8b2843f9dfdbeb18ce4368af9609f9a087bc95d171cc9324d4f968ab26d383f`.
- The five-member package remains
  `ba03203e9416a4eb75ca972fb0b3e5eba8618c46b2c26c7f65e2b7ffff387a22`.
  Its 10,011,264-byte binary is
  `cf7c436c442c426a6e9a1d485a5261fe5e5a05c8b131b130e679e00eec2c646b`,
  and all four packaged documents equal the current frozen documents.
- Accepted Task 5 to current is exactly nine added and six changed paths, with
  no removal. The only runtime Rust delta is the first-run non-TTY diagnostic
  string plus its contract assertion. No setup watcher, discovery loop, secret
  helper, telemetry, runtime dependency, public URL, profile writer, or PATH
  registry writer was added.
- The installer bounds archive members and link-like entries, keeps explicit
  archive/checksum inputs, and limits PATH behavior to preview/none. The support
  and client tables preserve macOS-only runtime evidence and Windows/Linux,
  signing/quarantine, clean-account, login, and low-end limitations.
- Prior-preservation 1,646 rows, final-v2 44 rows including its HOLD, three held
  build extras, doc-fix 12 rows including its HOLD, original SPEC 17 rows,
  rereview 12 rows, and the two parent audit files were unchanged.

## Verification accounting and limits

This review ran one new isolated runtime scenario, which reproduced Q1, and one
POSIX shell syntax parse, which passed. It did not rerun Cargo, the 365 Rust
tests, the 64-test Python matrix, three client flows, or five native lifecycle
cycles. Their immutable final-v2 records were inspected and preserved; they are
not counted as fresh tests. The direct wizard sample count of one remains a
sample rather than a peak, and the separate command-tree peak is not a direct
PID peak. Windows/PowerShell, Linux, clean OS accounts, low-end hardware,
signing/quarantine, and real login remain unverified.

The original SPEC wrapper's corrected raw false aggregate and the rereview's
preserved literal-audit error remain reviewer-tool errors, not product failures.
Historical Task 5 missing diagnostics and overwritten provisional debug objects
remain unresolved and were neither reconstructed nor reclassified.

One reviewer command refreshed five ignored, unheld `__pycache__` files. It did
not affect source101, package5, or preserved evidence; nothing was deleted,
restored, or reconstructed. The exact paths and the automatically rejected
direct temp-removal attempt are retained in `review-incident.json`; the owned
temporary extraction was moved to Trash and its original path is absent.

`execution_finished=true`, `runtime_ownership_released=true`,
`owned_workers_remaining=0`. The next step is a bounded fix and fresh quality
rereview of Q1 and Q2 before the separate final native integration review.
