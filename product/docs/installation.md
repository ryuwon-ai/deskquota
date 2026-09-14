# Install llmgw

llmgw is distributed as a target-specific archive plus a one-entry SHA-256
manifest. The archive contains `llmgw` (`llmgw.exe` on Windows), this README,
and the installation, runtime, and client compatibility documents. It does not
contain source, build targets, test artifacts, caches, or user configuration.

No public release location exists yet. The installers have no built-in registry
or download URL. Obtain the archive and matching manifest through the delivery
channel named by the person or system providing the build. Do not turn the
examples below into a public download one-liner until a public release exists.

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
`-PathAction None` suppresses the preview. Windows execution, Authenticode,
SmartScreen, and user-PATH behavior remain unverified until an actual Windows
artifact is tested on Windows; do not use an execution-policy bypass or an
antivirus exception.

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

## First setup and lifecycle

`llmgw setup` opens the terminal wizard. It does not download or start a model,
reuse a subscription login, inspect a keychain, call inference, or change a
client file before the final reviewed apply. Model listing is a separate user
choice. Final apply, including Save only, creates the protected config-scoped
state directory and local data/control tokens without starting a worker or
changing a client. Existing token bytes are preserved. For an older or
hand-written config without initialized state, `llmgw run` and `llmgw on` use
the same protected lifecycle provisioning path.

After saving a config:

```sh
llmgw on
llmgw status
llmgw off
```

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
| Windows x64 | Unverified | installer source review only; PowerShell was unavailable on the host |
| Linux x64/arm64 | Unverified | no Linux runtime, systemd user manager, or target artifact was exercised |
| Clean native account without development runtimes | Unverified | restricted PATH on the development account is a separate smoke |
| Login, low-end PC, signing/notarization/quarantine | Unverified | no actual user-login cycle, low-end host, signed public artifact, or quarantine flow was available |

The verified client combinations are Pi 0.84.2 over OpenAI Chat Completions,
Claude Code 2.1.63 over Anthropic Messages, and Codex 0.154.0 over OpenAI
Responses with WebSocket disabled. See the detailed listing, selection, tool,
disconnect, and gateway-off results in [client compatibility](client-compatibility.md).
