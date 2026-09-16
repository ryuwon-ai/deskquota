# Native preview release plan

1. Extend `product/scripts/package_native.py` with stdlib ZIP output, sharing
   captured members and checksums. Extend its existing unittest file to cover
   deterministic ZIP bytes, binary identity, exclusive output and bad inputs.
2. Add `.github/workflows/native.yml`: pinned official checkout/upload actions,
   explicit native targets, Rust 1.88.0, locked default tests/build, packager,
   installer and synthetic runtime acceptance, artifact uploads. No release token.
3. Update EN/KR README, product README and installation guide with versioned
   asset names, checksum-first install commands and preview signing limits.
4. Commit release preparation to `develop` after local checks and independent
   review. Execute actual CI; inspect failures before any retry. Download its
   successful artifacts and exercise the released binary on the local Mac and,
   when reachable, the previously authorized Windows lab over SSH.
5. Tag the tested commit, publish a preview with exact artifact hashes and run
   URL, verify GitHub asset readback, and record actual results separately from
   untested OS/security-policy combinations.
6. Independently prepare and run bounded NVIDIA task checks when an authorized
   free test credential is available. Reuse smoke transport; preserve no-content
   logs. Record failures as well as passes; do not claim statistical superiority.
