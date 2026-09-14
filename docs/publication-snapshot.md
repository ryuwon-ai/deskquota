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
