# DeskQuota / llmgw

A native local gateway for agents and scripts sharing one limited LLM endpoint.
One executable; Docker, WSL, Python, Node and Redis are not runtime requirements.

```sh
llmgw setup
llmgw on
llmgw status
llmgw off
```

Setup previews endpoint, protocol, model, authentication and quota settings before
saving. `llmgw doctor` checks locally; `llmgw restart` applies saved configuration.
`llmgw autostart on` previews user-login startup and prints how to apply it.

Numeric RPM/TPM values enforce a local rolling 60-second budget. `unknown` adds no
local quota cap. Only traffic through this instance is accounted for. New settings
use actual usage settlement; missing usage retains the reservation. Input token
counts are estimates. Startup hold defaults to 60 seconds and is configurable.

Exact caching and gateway replay are opt-in. The gateway cannot increase provider
quota or observe every outside consumer. Long waits can exceed client deadlines.

Client connections change base URLs and configuration files. Review `connect`'s
preview before applying it; supported managed profiles are version-checked.
Model discovery and capabilities can differ. `off` leaves client settings in
place; `disconnect` restores only managed values that have not changed since.
The endpoint must support the client's API format; there is no protocol conversion.
`/responses/compact` is unsupported, and known-TPM inspection rejects opaque
compaction inputs. Standard API credentials are required for authenticated APIs.

## Installation

Use the archive and installer from the same release, with their matching SHA-256
files. Verify the installer before executing it. The installer checks the archive
before replacing the binary and prints PATH instructions without editing profiles
or registry PATH. Offline installation uses the same local files. Follow platform
policy for unsigned previews; never bypass security controls to install.

If replacement reports an unconfirmed recovery, preserve the target, backup and
candidate paths and stop. Do not blindly retry, overwrite or delete recovery files.

See the project README for installation commands, supported platforms, configuration
and source-build instructions:

- English: https://github.com/ryuwon-ai/deskquota#readme
- 한국어: https://github.com/ryuwon-ai/deskquota/blob/develop/README.ko.md

## License

MIT OR Apache-2.0. Both license texts accompany native packages and live at the
repository root. This directory contains the product source and regression tests.
