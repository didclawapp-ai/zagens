# Security Policy

## Supported versions

Security fixes are provided for the **current release line** of Zagens desktop (see [CHANGELOG.md](CHANGELOG.md) and [GitHub Releases](https://github.com/didclawapp-ai/zagens/releases)). Older preview tags may not receive patches.

## Reporting a vulnerability

**Please do not open public GitHub issues for security vulnerabilities.**

Report privately to the maintainers (use GitHub **Private vulnerability reporting** if enabled on this repository, or contact via the security channel listed on [zagens.com](https://zagens.com)).

Include:

- Affected version or commit
- Steps to reproduce
- Impact assessment (data exposure, RCE, sandbox escape, etc.)
- Proof-of-concept if available

We aim to acknowledge reports within **7 business days** and will coordinate disclosure timing with the reporter.

## Scope

In scope:

- Zagens desktop (`crates/desktop/`) — WebView, Tauri IPC, sidecar supervision
- Embedded runtime (`crates/runtime-server/` and related crates) — tool execution, sandbox, network policy, secrets handling
- Supply-chain and release artifacts built from this repository

Out of scope:

- Third-party LLM provider APIs and their key management on your machine
- Issues in user-supplied MCP servers, skills, or workspace content
- The separate website / CMS repository

## Safe defaults

When triaging or reproducing:

- Do not commit API keys, tokens, or session dumps
- Use `~/.zagens/config.toml` or OS keyring — see [`.env.example`](.env.example) and [`config.example.toml`](config.example.toml)
- Prefer `workspace-write` or `read-only` sandbox modes for untrusted workspaces
