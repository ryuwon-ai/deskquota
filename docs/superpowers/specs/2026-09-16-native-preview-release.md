# Native preview distribution

User approved public binaries and real API validation on 2026-09-16. Reuse the
existing archive installer and acceptance probes; no gateway runtime changes.

- Build the default, BPE-free executable on GitHub's macOS 14 ARM64 and Windows
  2022 x64 runners with Rust 1.88.0 and the committed Cargo.lock. Windows uses
  MSVC with static CRT; end users need no compiler or development-tool PATH.
- Extend the existing deterministic packager to ZIP; retain its five-member
  allowlist, captured binary hash, exclusive output and per-archive checksum.
- Run Rust tests, packaging checks, and native installer/runtime probes before
  uploading artifacts. Public release assets include both archives, their
  checksums, and the existing installers with separate checksums.
- CI has read-only permissions; a maintainer publishes verified artifacts from
  a successful run as `v0.1.0-preview.1`, pinned to its tested source commit.
  No automatic publishing from untrusted PRs or mutable latest downloads.
- Publish as a preview: no Apple notarization or Windows Authenticode identity
  is configured. Never bypass endpoint security. Linux/Intel Mac/BPE assets
  remain source-build options until separately validated.
- Real NVIDIA checks use only fixed public tasks, bounded requests/tokens/time,
  no raw outputs or credentials in evidence, and no retries in either client
  or gateway. An unavailable API credential is a remaining external dependency,
  never a successful provider test. API task checks and installed agent checks
  are distinct evidence classes.

Acceptance: actual CI success, artifact readback and checksum verification,
native install/reinstall/bad-checksum behavior, independent review, and EN/KR
entry points. Rollback: withdraw the preview; reinstall a prior verified binary.
