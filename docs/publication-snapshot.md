# Initial public snapshot

DeskQuota was published from the existing research workspace. The current source,
authored documentation, benchmark scripts, and selected text evidence are included.
`RESEARCH.md` preserves the original research index unchanged.

Reference checkouts, downloaded and extracted papers, Python environments, Cargo
build output, and packaged executables remain local. Reference and paper manifests
record their origins. Historical evidence may refer to these excluded artifacts
or to the original developer filesystem. A historical path or acceptance record
is not a published download or a fresh execution result.

## Secret scan review

The initial candidate was scanned with Gitleaks 8.30.1. Its eleven findings were
reviewed before publication:

- Eight findings in `identity-before.json` and `identity-after.json` are SHA-256
  file identities. Each was recomputed and matched the referenced log file.
- The localhost private key in `product/tests/fixtures/setup-localhost-key.pem`
  is deliberately public test data, paired with the `llmgw-test-ca` certificate.
  The two historical `source.diff` copies contain the exact same key.

`.gitleaksignore` lists only those file/rule/line fingerprints. It does not exempt
entire test directories or generic API-key detection. The fixture must never be
used for a deployed endpoint. Original evidence bytes were preserved.

## Publication validation

README local links and GitHub Markdown rendering were checked. The publication
changes branding and documentation; it does not change the gateway runtime.
Benchmark and compatibility claims remain limited to their linked evidence.

## 2026-09-15 implementation update

The follow-up includes bounded exact caching, standard client authentication,
final JSON/SSE usage settlement, configurable startup hold, Windows source
fixes, and the associated tests, documentation and competitor comparisons.
Selected text evidence includes successful runs, controls and failed attempts;
reference runtimes, bytecode, build output and executables remain local.

Before committing, the 70 source/manifests matched the recorded release build
with 427 passing Rust tests, zero failures and one ignored profiling check.
The release binary hash also matched. This rechecks artifact identity; it is
not a new test run or native Windows validation.

Gitleaks 8.30.1 identified one additional SHA-256 file identity in
`evidence/company-feedback-2026-09-15/final-integrity.json`. It was recomputed
against `windows-after-auth.json` and matched. Only that exact file/rule/line
fingerprint was added to `.gitleaksignore`; original evidence bytes remain intact.
