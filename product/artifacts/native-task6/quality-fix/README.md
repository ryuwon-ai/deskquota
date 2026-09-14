# Native Task 6 QUALITY fix

Status: **QUALITY FIX COMPLETE — ready for the same QUALITY reviewer**. Four packaging/test paths changed from the frozen SPEC documentation fix:

- `product/scripts/package_native.py`
- `product/packaging/install.ps1`
- `product/tests/test_package_native.py`
- `product/tests/test_installers.py`

## Q1

`package_native.py` now calculates `binary_sha256` from the already captured `contents["llmgw"]` bytes used to write and verify the archive. A deterministic interleaving regression replaces the fixture input path after the archive hash is computed. The old implementation reported the replacement path's hash and failed; the fixed implementation reports the archived member's hash. Independent package inspection confirms identity/member/held binary SHA `cf7c436c442c426a6e9a1d485a5261fe5e5a05c8b131b130e679e00eec2c646b`.

## Q2

If `File.Replace` throws, `install.ps1` now performs no target, backup, or candidate copy, move, removal, cleanup, or automatic restoration. `preserveRecovery` remains true and the error reports the original replacement error plus target, destination-local backup, and candidate paths. This cannot delete an unknown concurrent target and matches the existing manual stop/preserve/recover documentation. PowerShell and Windows execution remain unverified.

## Fresh focused verification

- RED: two regressions failed against the old source; the raw output is retained in `targeted-red.log`.
- GREEN: all five package-builder and PowerShell static tests passed after the last source edit; raw output is retained in `targeted-final-green-v2.log`.
- Source: 101 files and 102 archive members; the exact manifest is embedded and live/archive drift is zero.
- Package: five unique regular members; docs equal current source, binary equals the held final-v2 executable, and the archive is byte-identical to the SPEC doc-fix package because its payload did not change.
- Prior preservation: 1,680 rows were verified with zero drift before the fix.

## Identities

- Source manifest SHA-256: `d00ff4d106cd374857d4738441c3406a8085e8344d3c52e845f81a82763a88c7`
- Source archive SHA-256: `0d8897fcd9300f433740f3fbd24edc042f8b7706e18715b50c5fc8a90de3ec2b`
- Package SHA-256: `ba03203e9416a4eb75ca972fb0b3e5eba8618c46b2c26c7f65e2b7ffff387a22`
- Package checksum manifest SHA-256: `c9ca619d26d310d7dbe06431c7a3d959541bc3c6c134899322d7e3c8b4d3ac91`
- Packaged binary SHA-256: `cf7c436c442c426a6e9a1d485a5261fe5e5a05c8b131b130e679e00eec2c646b`

## Retained evidence and limits

Rust, Cargo, product docs, runtime source, and the compiled binary are unchanged. No Cargo/build, Rust suite, full Python matrix, client flow, native measurement, or installation smoke was rerun. The retained final-v2 Rust 365 and Python 64 results apply to the unchanged compiled inputs and binary and are not new executions. No user configuration, authentication, PATH/profile/registry, OS registration, model, package installation, paid network, cloud, public release, or VCS action occurred.

`execution_finished=true`; `runtime_ownership_released=true`; `owned_workers_remaining=0`.
