# llmgw

`llmgw` is a native local gateway for sharing a limited upstream LLM quota
between coding tools on one computer. It runs as one Rust executable and keeps
its config and state in the current user's directories. Docker, WSL, Python,
Node, Redis, and a Rust toolchain are not runtime requirements.

Start with [installation](docs/installation.md), then run `llmgw setup` in a
terminal. [Runtime behavior](docs/runtime-contract.md) defines lifecycle,
security, and quota boundaries. [Client compatibility](docs/client-compatibility.md)
lists the exact Pi, Claude Code, and Codex versions and protocol paths that have
been exercised.

[Preview packages](https://github.com/ryuwon-ai/deskquota/releases/tag/v0.1.0-preview.1)
are available for macOS ARM64 and Windows x64, with separate SHA-256 manifests
and installers. See the installation guide for verification and signing limits.
