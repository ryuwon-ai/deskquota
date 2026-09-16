# Install llmgw

llmgw is distributed as a target-specific archive plus a one-entry SHA-256
manifest. The archive contains `llmgw` (`llmgw.exe` on Windows), this README,
and the installation, runtime, and client compatibility documents. It does not
contain source, build targets, test artifacts, caches, or user configuration.

Download the default, BPE-free build from the
[v0.1.0-preview.1 release](https://github.com/ryuwon-ai/deskquota/releases/tag/v0.1.0-preview.1):

| Platform | Archive | Installer |
|---|---|---|
| macOS ARM64 (Apple silicon) | `llmgw-macos-arm64.tar.gz` | `install.sh` |
| Windows x64 | `llmgw-windows-x64.zip` | `install.ps1` |

Every archive and installer has a separate `.sha256` file. Download the installer,
its checksum, the archive and its checksum from that same versioned release.
Check the installer before running it, from the download directory:

```sh
shasum -a 256 -c install.sh.sha256
sh install.sh --archive ./llmgw-macos-arm64.tar.gz \
  --checksum-manifest ./llmgw-macos-arm64.tar.gz.sha256 \
  --install-dir "$HOME/.local/bin" --path-action preview
```

On Windows:

```powershell
$expected = ((Get-Content ./install.ps1.sha256 -Raw).Trim() -split '\s+')[0]
if ((Get-FileHash ./install.ps1 -Algorithm SHA256).Hash -ne $expected) { throw 'installer_checksum_mismatch' }
& ./install.ps1 -Archive ./llmgw-windows-x64.zip `
  -ChecksumManifest ./llmgw-windows-x64.zip.sha256 `
  -InstallDir "$env:LOCALAPPDATA\Programs\llmgw\bin" -PathAction Preview
```

After reviewing the printed PATH instructions, run `llmgw setup`, then `llmgw on`.
If PATH is not configured, run the installed executable by its full path.
No shell profile or registry PATH is modified by the installer. Offline transfer
works with the same four files; the destination needs no Internet for installation.

**Preview signing:** macOS has an ad-hoc linker signature, not an Apple Developer ID
signature or notarization. Windows has no Authenticode signature. Checksums detect
corruption but do not establish a publisher identity. Follow your OS and company
policy; do not disable Gatekeeper, SmartScreen, antivirus, or PowerShell policy to
install. Signing and clean-machine policy acceptance remain release limitations.

These packages contain the default estimator; BPE is an explicit source-build
feature. Intel macOS and Linux packages are not included in this preview.
The native workflow builds from the committed lockfile on macOS and Windows,
then tests and uploads artifacts. Release notes link the exact successful run and
source commit; a maintainer publishes those artifacts without rebuilding them.

## macOS and Linux

When `install.sh` is delivered beside the archive, install from local files
without network access:

```sh
sh packaging/install.sh \
  --archive /absolute/path/llmgw-TARGET.tar.gz \
  --checksum-manifest /absolute/path/llmgw-TARGET.tar.gz.sha256 \
  --install-dir "$HOME/.local/bin" \
  --path-action preview
```

The same command accepts an explicitly supplied `http://` or `https://` URL for
each input. It downloads nothing else. Before changing the installed binary it
checks the manifest, rejects duplicate, linked, traversing, and unexpected
archive entries, extracts only `llmgw` into a temporary stage, and stages the
verified bytes in the destination directory. A missing artifact, wrong hash,
invalid archive, or destination failure leaves an existing installed executable
in place. Reinstalling the same verified artifact is supported.

If only the archive and manifest are available, verify and extract them with OS
tools; no repository checkout or Python runtime is needed. Run the checksum
command from the directory containing both files. On macOS:

```sh
shasum -a 256 -c llmgw-TARGET.tar.gz.sha256
```

On Linux:

```sh
sha256sum -c llmgw-TARGET.tar.gz.sha256
```

Only after that command succeeds, list the archive and confirm the exact five
members before extracting:

```sh
(
  set -eu
  tar -tzf llmgw-TARGET.tar.gz
  # Expected exactly: llmgw, README.md, docs/installation.md,
  # docs/runtime-contract.md, docs/client-compatibility.md
  stage="$(mktemp -d)"
  trap 'rm -rf -- "$stage"' EXIT HUP INT TERM
  tar -xOzf llmgw-TARGET.tar.gz llmgw > "$stage/llmgw"
  chmod 755 "$stage/llmgw"
  mkdir -p "$HOME/.local/bin"
  candidate="$(mktemp "$HOME/.local/bin/.llmgw-install.XXXXXX")"
  cp "$stage/llmgw" "$candidate"
  chmod 755 "$candidate"
  mv -f "$candidate" "$HOME/.local/bin/llmgw"
)
```

Stop if the checksum or exact member list differs. The manifest authenticates
bytes only to the extent that its delivery channel is trusted.

The installer never edits a shell profile. It expands the selected directory,
quotes it as a POSIX literal, and prints one line in this form:

```sh
export PATH='/absolute/home/.local/bin':"$PATH"
```

Apply that line in the appropriate shell profile yourself, open a new shell,
and verify that the new shell resolves the installed command:

```sh
command -v llmgw
llmgw --version
llmgw setup
```

For a custom install directory, use that exact directory in the PATH line and
quote it according to the selected shell. `--path-action none` suppresses the
preview. Running the executable by absolute path proves only the file can run;
it does not prove a new shell can find `llmgw` by name.

On macOS, use an artifact that follows the release's signing and notarization
policy. Do not remove quarantine attributes or add antivirus exceptions to make
an untrusted artifact run. Linux packaging and library support must be verified
on its actual target before being marked supported.

## Windows PowerShell

The Windows archive is a ZIP containing `llmgw.exe` and the same four user
documents. When `install.ps1` is delivered beside it, run the script without
changing PowerShell execution policy:

```powershell
& .\packaging\install.ps1 `
  -Archive C:\absolute\path\llmgw-TARGET.zip `
  -ChecksumManifest C:\absolute\path\llmgw-TARGET.zip.sha256 `
  -InstallDir "$env:LOCALAPPDATA\Programs\llmgw\bin" `
  -PathAction Preview
```

The script verifies the ZIP before replacing `llmgw.exe`. It prints the exact
directory to add to the user PATH, but it does not read or write the user PATH
registry. Add the directory through the normal Windows user environment UI,
open a new PowerShell window, then run `Get-Command llmgw` and `llmgw setup`.
`-PathAction None` suppresses the preview. On one Windows 10 Education x64 host,
installation, reinstall, checksum rejection, and execution with only Windows
directories on PATH passed; both user and machine PATH registry values stayed
unchanged. This does not verify a clean non-admin account, manually adding PATH,
Authenticode, or SmartScreen. Do not use an execution-policy bypass or an
antivirus exception. See the [native validation report](../../reports/windows-native-validation-2026-09-15.md).

If the installer reports `replace_failed_recovery_unconfirmed`, stop and keep
the reported target, destination-local `.llmgw-backup-*`, and candidate files.
Do not rerun the installer or delete its recovery files. The failed replacement
may have left `llmgw.exe` absent while the previous executable exists only at
the reported backup path. Record the complete original and recovery errors and
all three paths. Before restoring anything, confirm that the target is still
absent, no other installer or process is writing it, and the backup matches the
known previous binary by its trusted checksum or release identity. Only then
copy the verified previous executable back to the reported target; move it only
if another recovery copy remains. Keep the backup and candidate until the
restored target's checksum, version, and execution have been verified. Never
blindly overwrite an existing target. If the previous binary's identity or
writer state cannot be established, preserve every reported file and provide
the exact errors and paths when requesting support.

For an archive-only offline install, compare `(Get-FileHash -Algorithm SHA256
.\llmgw-TARGET.zip).Hash` with the one manifest entry, use `Expand-Archive` into
a new temporary directory, and confirm that the five expected files are the
only extracted files before copying `llmgw.exe` to the user-writable install
directory. Do not continue when the checksum or member list differs.

### Building from source on Windows

Running a supplied `llmgw.exe` does not require Rust, MSVC Build Tools, MinGW,
Docker, or WSL. Building from source has separate requirements. The repository
pins Rust 1.88.0; select `x86_64-pc-windows-gnu` explicitly when using an approved
WinLibs/MinGW-w64 installation instead of MSVC. Rust's target standard library
does not supply the complete C build environment. Use one matching x64 toolchain
with GCC/G++, Windows headers and libraries, and Binutils (`as`, `ar`, `ld`,
`dlltool`). [Rust GNU target requirements](https://doc.rust-lang.org/rustc/platform-support/windows-gnu.html)

In a new PowerShell session, after provisioning that toolchain and the pinned
Rust GNU host/target through your approved software channel:

```powershell
$winlibsBin = 'C:\Tools\winlibs\mingw64\bin' # Replace with your approved location.
$env:PATH = "$winlibsBin;$env:PATH"
Get-Command gcc, g++, as, ar, ld, dlltool
gcc --version
as --version
dlltool --version
rustc +1.88.0-x86_64-pc-windows-gnu --version
$env:CARGO_TARGET_DIR = "$env:LOCALAPPDATA\llmgw-build"
$env:CARGO_BUILD_JOBS = '2'
cargo +1.88.0-x86_64-pc-windows-gnu build --locked --release --target x86_64-pc-windows-gnu
```

If Rust selects its bundled `dlltool` but assembly fails, confirm that the
matching assembler is present and resolvable; adding only `dlltool.exe` is
insufficient. GNU documents its assembler dependency and `--as` selection.
[Binutils dlltool](https://sourceware.org/binutils/docs/binutils/dlltool.html)
For an explicitly selected external toolchain, the following repository-local
`.cargo/config.toml` settings select its linker and import-library tool. Adjust
the paths; preserve any other existing target settings:

```toml
[target.x86_64-pc-windows-gnu]
linker = 'C:\Tools\winlibs\mingw64\bin\gcc.exe'
rustflags = ["-C", "link-self-contained=no", "-C", 'dlltool=C:\Tools\winlibs\mingw64\bin\dlltool.exe']
```

The TLS dependency also builds C code. For its x64 non-FIPS build, NASM or the
documented prebuilt NASM objects are required; `AWS_LC_SYS_PREBUILT_NASM=1` is
an available build setting when NASM is absent. Do not use the debug-only
no-assembly option as a release workaround. [AWS-LC Windows requirements](https://aws.github.io/aws-lc-rs/requirements/windows.html)

Config-file replacement uses the opened handle's volume and full 128-bit file
ID, attributes, and bytes. Creation time and length are not identity checks;
filesystems that cannot provide the required ID fail closed.
[Microsoft file identity contract](https://learn.microsoft.com/en-us/windows/win32/api/winbase/ns-winbase-file_id_info)
The full product was built and tested natively on Windows 10 Education x64 with
Rust 1.88.0 GNU, existing MinGW-w64 GCC 8.1.0 / Binutils 2.30, and prebuilt NASM
objects: 404 tests passed, 2 were ignored, and the default-feature release build
passed. The example WinLibs location above is illustrative; the tested toolchain
was at `C:\mingw64\bin`. This is evidence for that combination, not a recommendation
to install an old GCC or a claim about every WinLibs release.

The reusable acceptance check uses Python 3.11+ only as a test driver. From
`product`, point it at your built binary and a new work directory outside
OneDrive; it installs into that directory and calls a synthetic loopback server:

```powershell
py -3.11 scripts\verify_windows.py `
  --binary "$env:CARGO_TARGET_DIR\x86_64-pc-windows-gnu\release\llmgw.exe" `
  --work "$env:LOCALAPPDATA\llmgw-acceptance-new" `
  --output "$env:LOCALAPPDATA\llmgw-acceptance.json"
```

It checks install/reinstall/checksum failure, PATH preservation, `on/status/off`,
standard Authorization forwarding, JSON/SSE exact-cache replay, usage settlement,
and idle connection resource counters. It makes no real-provider requests.

## First setup and lifecycle

`llmgw setup` opens the terminal wizard. It does not download or start a model,
reuse a subscription login, inspect a keychain, call inference, or change a
client file before the final reviewed apply. Model listing is a separate user
choice. Final apply, including Save only, creates the protected config-scoped
state directory and local control token without starting a worker or
changing a client. Existing token bytes are preserved. For an older or
hand-written config without initialized state, `llmgw run` and `llmgw on` use
the same protected lifecycle provisioning path.

The default executable includes only UTF-8 byte estimation and skips tokenizer
questions. To include offline BPE vocabularies in the same executable, build
with `cargo build --locked --release --features bpe` (or add `--features bpe` to
the existing source-install command). No runtime download or additional service
is needed. An explicitly identified BPE-enabled archive uses the same installer
and commands; this project has not published a release URL.

In a BPE-enabled build, the model step still defaults to UTF-8 bytes. Explicit BPE
choices (`cl100k_base`, `o200k_base`) add an adjustable framing allowance and
vocabulary memory; select them only when they match the upstream. This is a
serialized JSON estimate, not an exact server token count. Existing model
choices survive setup reruns; renamed models return to byte defaults. See the
[runtime contract](runtime-contract.md#token-estimation-and-output-bounds) for
configuration and limitations.

For each RPM/TPM setting, choose the enforcement you want: **Known** adds a local
rolling 60-second cap; **Unknown** leaves that cap off and defers to the upstream,
whose limit remains unverified; **Unlimited** explicitly leaves the local quota
cap off. Concurrency and shared 429 cooldown remain active in all three cases.
Entering a provider's advertised number does not automatically reproduce its
refill policy. Keep a known cap when you want that additional local budget.

A default build rejects an explicit BPE configuration before new startup or
setup saving. Choose a BPE-enabled executable or explicitly change the estimator;
there is no automatic fallback. `status` and `off` can still inspect and stop an
existing BPE-configured worker. An unsupported `restart` fails before stopping it.

After saving a config:

```sh
llmgw on
llmgw status
llmgw off
```

While running, `llmgw status` shows local quota modes, capacities, debits and
holds, plus a representative queue reason and the protected root. These values
explain local admission; they are not the provider's remaining allowance or a
per-request wait estimate. Unknown/unlimited capacity is shown as `n/a`.
Use `llmgw status --json` for the existing machine-readable status fields.

`off` leaves client URLs and login intent in place. Connected clients will fail
while the gateway is off; run `llmgw on` again or use the matching
`llmgw disconnect pi|claude|codex` command to restore only llmgw-owned settings.

`llmgw autostart on` and every `connect` command first print the exact target,
impact, and preview hash. Applying either change requires the matching hash.
Autostart does not start the current worker. Client backup and restore behavior,
including preserved unrelated edits, is described in
[client compatibility](client-compatibility.md).

## Current support status

| Target or flow | Status | What was exercised |
|---|---|---|
| macOS arm64 installer | Verified on the Apple M4 development host | local archive, loopback HTTP fixture, checksum failure, missing artifact, invalid entries, destination failure, reinstall, PATH absent control, and a fresh child-shell PATH smoke |
| macOS arm64 runtime | Verified on the same development host | isolated setup save-only, repeated `on`/authenticated `status`/`off`, and three installed clients listed below |
| macOS x64 | Unverified | no x64 artifact or runtime host was exercised |
| Windows x64 GNU | Verified on one Windows 10 Education x64 admin account | Rust 1.88 GNU build, 404 tests, ZIP install/reinstall/checksum rejection, interactive setup, lifecycle, JSON/SSE cache and usage, restricted-PATH execution |
| Windows user-login task | Registration and manual scheduler run verified | UTF-16 XML, current-user identity, non-elevated worker token, owned status, stop and task removal; no actual login event |
| Linux x64/arm64 | Unverified | no Linux runtime, systemd user manager, or target artifact was exercised |
| Clean native account without development runtimes | Unverified | restricted PATH on the development account is a separate smoke |
| Login, low-end PC, signing/notarization/quarantine | Unverified | no actual user-login cycle, low-end host, signed public artifact, or quarantine flow was available |

The current profiles accept Pi 0.84.2, Claude Code 2.1.76, and Codex 0.154.0.
Windows Claude 2.1.76 passed managed connection, an exact Read result, and
disconnect. Windows Codex completed profile loading and a Responses round trip,
but its tool command was blocked by native client policy. Pi and the earlier
Claude 2.1.63/Codex native tool results were exercised on macOS, with the
historical limits described in the compatibility matrix. See the detailed listing, selection, tool,
disconnect, and gateway-off results in [client compatibility](client-compatibility.md).

The Windows user-login task smoke is independently runnable with Python 3.11+
while the current user is logged in. It uses a new isolated directory, registers
one temporary task and manually asks Task Scheduler to run it, then stops the
worker and removes the registration. It does not test logout/login or reboot:

```powershell
py -3.11 scripts/verify_windows_autostart.py `
  --binary C:\approved\llmgw.exe `
  --work C:\approved\new-isolated-autostart-check
```

See the [2026-09-16 follow-up](../../reports/windows-followup-and-improvements-2026-09-16.md)
for the exact native-client and Task Scheduler evidence.
