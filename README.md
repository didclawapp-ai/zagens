<p align="center">
  <img src="assets/screenshot.png" alt="Zagens Screenshot" width="800" />
</p>

# Zagens

**[中文](README.zh-CN.md)** · **[日本語](README.ja.md)** · **[Português (BR)](README.pt-BR.md)** | English

Long-horizon agent work tends to **stall or “claim done” too early**. Code and office files often live in **separate tools**. Local agents need **replay, approval, and auditability** — not just another chat window.

**Zagens** is a **desktop agent harness** that runs on your machine: one local **runtime sidecar** for Code and Office workspaces, turn-by-turn **session replay**, layered **completion gates** for long tasks, and desktop-native shell (tray, notifications, embedded terminal). Connect [DeepSeek](https://deepseek.com/) or any OpenAI-compatible provider with your own API key.

> **From the authors:** Don’t believe an AI agent can do anything — it has boundaries. What we can do is expand those boundaries.

> **License:** [MIT](LICENSE). Runtime lineage: [NOTICE.md](NOTICE.md) · [third-party/deepseek-tui/](third-party/deepseek-tui/). **Not affiliated with DeepSeek Inc.** Capabilities below reflect **Zagens v0.7.3** — see [CHANGELOG.md](CHANGELOG.md).

| Resource | Link |
|----------|------|
| User guides | [zagens.com/docs](https://zagens.com/docs) |
| Downloads | [GitHub Releases](https://github.com/didclawapp-ai/zagens/releases) (latest **`zagens-v0.7.3`**) · [zagens.com/download](https://zagens.com/download) |
| Design specs | [`docs/README.md`](docs/README.md) |
| Contributing | [`CONTRIBUTING.md`](CONTRIBUTING.md) · [`LOCAL_DEV_VERIFY.md`](LOCAL_DEV_VERIFY.md) |
| Security | [`SECURITY.md`](SECURITY.md) |

---

## Who Zagens is for

| Good fit | Less fit |
|----------|----------|
| Developers who want a **standalone desktop harness** (not locked into one IDE extension) | A hosted SaaS with managed models and billing |
| Teams working on **long code refactors** or **Office deliverables** in the same workflow | “Chat only” — no tools, no workspace, no replay |
| People who care about **local sidecar architecture**, MCP/skills, and **exec approval** in the UI | Fully autonomous YOLO agents with no guardrails |
| Windows desktop users today; macOS/Linux via **CLI** or source build | Zero-setup mobile or browser-only experience |

---

## Three things that define Zagens

**1. Harness, not a chat shell** — Long-horizon code tasks use **composable completion gates** (operator / model / toolchain layers), not “the model said it’s finished.” Spec: [LHT](docs/harness/LONG_HORIZON_CODE_TASKS.md) · fixtures: [`fixtures/harness/`](fixtures/harness/).

**2. Desktop-native control plane** — [Tauri 2](https://tauri.app/) UI over a loopback **sidecar** (`zagens-runtime`): system tray, notifications, diff panel, **session replay**, embedded **PTY** in Code workspaces, HTTP tool approval. Same engine as the headless **`zagens`** CLI.

**3. Code + Office, one runtime** — **Code** and **Office** task types share tools and config but use different tool surfaces and prompts; switching types starts a **new session** for stable model KV ([architecture](docs/task-type-prompt-architecture.md)). Office: `read_file` / **`write_office`** (xlsx via Rust; docx/pptx/pdf via bundled Python).

Also shipped: **CRAFT multi-agent** (sub-agents, fix-loop verdicts, P1 blackboard — [notes](docs/craft-v2-improvements.md)), lazy **symbol index** (`.zagens/symbols.json`), MCP, skills, hooks, scheduled tasks.

---

## Problems we focus on

| Pain | How Zagens approaches it |
|------|---------------------------|
| Agent stops mid-task or marks work complete prematurely | Layered **completion gates** + long-horizon task panel ([composable harness](docs/harness/COMPOSABLE_HARNESS.md)) |
| IDE plugins vs terminal agents don’t share one session story | Single **sidecar** + SQLite threads, fork/resume, **replay**, workspace snapshots |
| Spreadsheets and docs are outside the coding agent loop | **Office mode** with `write_office` and file previews in the desktop shell |
| Running tools locally without blind trust | Exec policy, network rules, path canonicalization, approval UI, runtime token kept out of the WebView ([sandbox matrix](docs/tech/SANDBOX_CAPABILITY_MATRIX.md)) |

---

## Shipped today (v0.7.3)

**Desktop:** multi-session chat (stream/stop/thinking), file tree + previews + diff, PTY terminal (Code), sub-agent panel, tasks/skills UI, MCP + routing rules, usage charts, keyring API key, UI in zh-Hans / en / ja / pt-BR.

**Runtime:** threads, MCP, skills (`~/.zagens/skills`), lifecycle hooks, multi-provider routing, vision (`describe_image`).

**Tools (representative):** files (`read_file`, `write_file`, `edit_file`, …), git, `exec_shell`, `write_office`, optional `web_search` / `fetch_url`, memory tools. Full list: `crates/runtime-server/src/tools/` · [CHANGELOG.md](CHANGELOG.md).

---

## Known limits (read before you rely on us)

We prefer honest scope over marketing checklists.

| Topic | Status |
|-------|--------|
| **Desktop installers** | **Windows** installer on [Releases](https://github.com/didclawapp-ai/zagens/releases). **macOS / Linux desktop packages** — planned. CLI binaries for all three platforms ship today. |
| **OS sandbox enforcement** | **macOS Seatbelt** — enforced when `sandbox-exec` is available. **Windows** — native sandbox enforced after setup (`elevated` recommended: profile read isolation + WFP; `unelevated` fallback: workspace write isolation only). Settings → **Sandbox** first-run wizard. **Linux** — policy declared, **not OS-enforced yet** (degraded). Details: [`SANDBOX_CAPABILITY_MATRIX.md`](docs/tech/SANDBOX_CAPABILITY_MATRIX.md). |
| **Providers** | You bring API keys; we connect to DeepSeek and other OpenAI-compatible endpoints — we do not host models. |
| **Long-horizon & multi-agent** | Gates and CRAFT are **production-usable but still evolving**; edge cases and new gate types land in active development. |
| **Office depth** | Core read/write paths work; enterprise connectors, voice, and some scenario templates are **future** ([Office scenarios](docs/desktop/OFFICE_SCENARIOS.md)). |

Report security issues via [`SECURITY.md`](SECURITY.md).

---

## Where we’re headed

Public design specs live under [`docs/`](docs/README.md). Directionally:

- **Platform parity** — macOS/Linux desktop installers; **Linux** native sandbox (Landlock/bwrap). Windows native sandbox shipped in 0.7.x.
- **Trustworthy long tasks** — tighter completion gates, harness fixtures, and replay-friendly operator workflows.
- **Office workflows** — richer scenario coverage without splitting away from the shared runtime.
- **Hardening** — security and exec-policy improvements tracked in [CHANGELOG](CHANGELOG.md) and [SECURITY.md](SECURITY.md).

---

## Quick start

**Pre-built (Windows):** [GitHub Releases `zagens-v0.7.3`](https://github.com/didclawapp-ai/zagens/releases) — installer zip + CLI. Unblock SmartScreen: [SMARTSCREEN.md](docs/desktop/SMARTSCREEN.md).

**From source:**

```bash
git clone https://github.com/didclawapp-ai/zagens.git
cd zagens

cargo build -p zagens-cli          # sidecar copied into crates/desktop/binaries/

cd crates/desktop/web-ui && npm install
cd .. && cargo tauri dev

# API key: Zagens Settings, or ~/.zagens/config.toml
```

**Headless CLI** (same runtime as desktop):

```bash
cargo install zagens-cli --version 0.7.3 --bin zagens --locked

zagens doctor
zagens exec 'summarize src/' --json
zagens exec 'refactor auth module' --auto
zagens serve --http --port 7878
```

Prebuilt CLI + SHA-256 sidecars: [Releases](https://github.com/didclawapp-ai/zagens/releases). Config reference: [config.example.toml](config.example.toml).

---

## Architecture

```
┌──────────────────────────────────────────────────────────────┐
│                     Zagens (Tauri 2)                         │
│  ┌─────────────────┐  ┌───────────────────────────────────┐  │
│  │   WebView UI    │  │         Rust Shell                │  │
│  │   React / TS    │◄─┤  commands, sidecar supervisor,    │  │
│  └────────┬────────┘  └───────────────┬───────────────────┘  │
│           │ HTTP + SSE                │                       │
│           ▼                           ▼                       │
│  ┌─────────────────────────────────────────────────────────┐  │
│  │     Runtime API (embedded sidecar, loopback HTTP/SSE)    │  │
│  │  /v1/threads, /v1/skills, /v1/symbol-index, ...         │  │
│  └───────────────────────┬─────────────────────────────────┘  │
│                          ▼                                    │
│  ┌─────────────────────────────────────────────────────────┐  │
│  │  Shared crates: agent, core, config, state, tools, mcp   │  │
│  └─────────────────────────────────────────────────────────┘  │
└──────────────────────────────────────────────────────────────┘
```

Full boundaries: [`docs/tech/RUNTIME_ARCHITECTURE.md`](docs/tech/RUNTIME_ARCHITECTURE.md) · HTTP contract: [`docs/tech/API_DESIGN.md`](docs/tech/API_DESIGN.md).

### Security modes (`sandbox_mode`)

| Mode | Description |
|------|-------------|
| `read-only` | No shell execution or file writes |
| `workspace-write` | Shell and writes within the workspace (recommended default) |
| `danger-full-access` | Full filesystem access — use with caution |
| `external-sandbox` | Route `exec_shell` through an OpenSandbox-compatible API |

Approval policies (`on-request` / `untrusted` / `never`), per-domain network rules, OS keyring for credentials. Runtime token never enters the WebView.

---

## Development

**Prerequisites:** Rust 1.88+ (MSRV; CI pins **1.96**), Node.js 20 LTS, Python 3.8+, [Tauri CLI 2](https://v2.tauri.app/start/prerequisites/).

See **[CONTRIBUTING.md](CONTRIBUTING.md)** and **[LOCAL_DEV_VERIFY.md](LOCAL_DEV_VERIFY.md)**.

| Command | Description |
|---------|-------------|
| `bash scripts/ci/verify-lint.sh` | CI lint mirror |
| `bash scripts/ci/verify-workspace.sh` | Lint + full workspace tests |
| `cargo test --workspace --all-features` | Run all tests |
| `cd crates/desktop && cargo tauri dev` | Launch desktop in dev mode |

Windows: `pwsh -File scripts/ci/verify-lint.ps1`

```
zagens/
├── crates/desktop/       # Tauri app
├── crates/runtime-server/ # Sidecar HTTP/SSE
├── docs/                 # Public design specs
├── fixtures/harness/     # LHT / Office harness fixtures
└── config.example.toml
```

---

## License

[MIT](LICENSE) — Copyright (c) 2024-2026 Zagens Contributors. Additional attribution: [NOTICE.md](NOTICE.md) · [third-party/deepseek-tui/LICENSE](third-party/deepseek-tui/LICENSE).
