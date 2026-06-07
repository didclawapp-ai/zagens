<p align="center">
  <img src="assets/screenshot.png" alt="Zagens Screenshot" width="800" />
</p>

# Zagens

**[中文](README.zh-CN.md)** | English

**Zagens** is a **desktop agent harness** for code and office workspaces — a standalone product built with [Tauri 2](https://tauri.app/). It connects to [DeepSeek](https://deepseek.com/) and other OpenAI-compatible providers through an embedded agent sidecar: system tray, turn notifications, workspace previews, session replay, and an embedded terminal in Code workspaces.

> **License:** [MIT](LICENSE). Runtime lineage attribution: [NOTICE.md](NOTICE.md) and [third-party/deepseek-tui/](third-party/deepseek-tui/).

| Resource | Link |
|----------|------|
| User guides | [zagens.com/docs](https://zagens.com/docs) |
| Design specs | [`docs/README.md`](docs/README.md) |
| Contributing | [`CONTRIBUTING.md`](CONTRIBUTING.md) · [`LOCAL_DEV_VERIFY.md`](LOCAL_DEV_VERIFY.md) |
| Security | [`SECURITY.md`](SECURITY.md) |

> **Not affiliated with DeepSeek Inc.** Capabilities below reflect **Zagens v0.7.0**. See [CHANGELOG.md](CHANGELOG.md) for version history.

---

## What makes Zagens different

| Theme | What you get |
|-------|----------------|
| **Desktop + sidecar** | UI talks to a local **runtime sidecar** over HTTP/SSE. Shared `~/.zagens/config.toml`, sessions, and tools. |
| **Code vs Office modes** | Different tool surfaces and prompts; switching task types starts a **new session** for stable model KV ([architecture](docs/task-type-prompt-architecture.md)). |
| **CRAFT multi-agent** | Sub-agents with role-specific tools, structured fix-loop verdicts, and a **P1 blackboard** for handoffs ([CRAFT notes](docs/craft-v2-improvements.md)). |
| **Symbol index** | Lazy per-workspace index (`.deepseek/symbols.json`) with call hints and desktop rebuild API. |
| **Office in the loop** | `read_file` for Office/PDF; **`write_office`** for `.xlsx` (Rust) and `.docx` / `.pptx` / `.pdf` (bundled Python). |
| **Safety-first execution** | Layered exec policy, bash arity dictionary, network rules, path canonicalization, HTTP tool approval in the UI. |
| **Desktop-native shell** | System tray, notifications, session replay, diff panel, **embedded PTY** in Code workspaces, sidecar health probes. |

---

## Desktop UI (v0.7.0)

| Area | Shipped |
|------|---------|
| **Chat** | Multi-session sidebar, streaming + stop, thinking stream, context bar, session export |
| **Workspace** | File tree, previews, diff2html, snapshot restore, **xterm.js PTY** (Code mode) |
| **Replay** | Turn-by-turn tool call replay |
| **Agents** | Sub-agent panel (SSE); checklist sidebar |
| **Tasks & skills** | Background tasks + skill UI |
| **Settings** | MCP, routing rules, usage charts, API key via keyring, zh-Hans / en / ja / pt-BR UI |

See [CHANGELOG.md](CHANGELOG.md) for the full release history.

---

## Agent runtime (sidecar)

The embedded **`deepseek-runtime`** process powers chat, tools, and settings:

- **Sessions & threads** — SQLite persistence, resume/fork, workspace snapshots
- **MCP** — stdio servers, per-tool filters
- **Skills** — `~/.zagens/skills`, desktop import/install APIs
- **Hooks** — lifecycle shell commands with stdout / JSONL / webhook sinks
- **Providers** — multi-provider config, profiles, routing rules
- **Vision** — `describe_image` via configurable vision endpoints

HTTP contract: [`docs/tech/API_DESIGN.md`](docs/tech/API_DESIGN.md). Annotated config: [config.example.toml](config.example.toml).

### Representative tools

| Category | Tools |
|----------|-------|
| **Files** | `read_file`, `write_file`, `edit_file`, `apply_patch`, `glob_files`, `grep_files` |
| **Repo** | `git_status`, `git_diff`, `git_log`, … |
| **Shell** | `exec_shell`, background tasks, optional external sandbox |
| **Office** | `write_office`, Office/PDF via `read_file` |
| **Web** | `web_search`, `fetch_url` (when enabled) |
| **Memory** | `remember`, `note`, opt-in user memory |

Implementation: `crates/runtime-server/src/tools/`.

---

## Security & sandbox

| Mode | Description |
|------|-------------|
| `read-only` | No shell execution or file writes |
| `workspace-write` | Shell and writes within the workspace only |
| `danger-full-access` | Full filesystem access (use with caution) |
| `external-sandbox` | Route `exec_shell` through an OpenSandbox-compatible API |

Approval policies (`on-request` / `untrusted` / `never`), per-domain network rules, path canonicalization, runtime token isolation (never in WebView), and OS keyring for credentials. Details in [`docs/tech/SANDBOX_CAPABILITY_MATRIX.md`](docs/tech/SANDBOX_CAPABILITY_MATRIX.md).

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

Full diagram and boundaries: [`docs/tech/RUNTIME_ARCHITECTURE.md`](docs/tech/RUNTIME_ARCHITECTURE.md).

---

## Prerequisites

- **Rust** 1.88+ (MSRV); dev/CI pin **1.96** — [`rust-toolchain.toml`](rust-toolchain.toml)
- **Node.js** 20 LTS (18+ may work for web-ui only)
- **Python** 3.8+ (office docs, code execution, RLM)
- **[Tauri CLI 2](https://v2.tauri.app/start/prerequisites/)** — `cargo install tauri-cli --version "^2"`

**Versions:** Zagens desktop **v0.7.0**; embedded runtime crates **0.8.15** (root `Cargo.toml`).

---

## Quick start

```bash
git clone https://github.com/zagens/zagens
cd zagens

# Sidecar (build.rs copies into crates/desktop/binaries/)
cargo build -p deepseek-runtime-server

cd crates/desktop/web-ui && npm install
cd .. && cargo tauri dev

# API key: Zagens Settings, or ~/.zagens/config.toml
```

**Release build (Windows):** from `crates/desktop`, run `npm run bundle:prepare` then `cargo tauri build`. CI release: push tag `zagens-vX.Y.Z`.

---

## Development

See **[CONTRIBUTING.md](CONTRIBUTING.md)** and **[LOCAL_DEV_VERIFY.md](LOCAL_DEV_VERIFY.md)** for lint hooks and CI parity.

| Command | Description |
|---------|-------------|
| `bash scripts/ci/verify-lint.sh` | CI lint mirror (fmt + strict clippy) |
| `bash scripts/ci/verify-workspace.sh` | Lint + full workspace tests |
| `cargo test --workspace --all-features` | Run all tests |
| `cd crates/desktop/web-ui && npm run build` | Build web UI |
| `cd crates/desktop && cargo tauri dev` | Launch Zagens in dev mode |

---

## Project structure

```
zagens/
├── crates/
│   ├── desktop/          # Tauri app (web-ui + Rust shell)
│   ├── runtime-server/   # Sidecar HTTP/SSE runtime
│   ├── core/             # Engine, turn loop
│   ├── tools/            # Tool registry & execution
│   └── …                 # agent, config, state, mcp, hooks, …
├── docs/                 # Public contributor documentation
├── third-party/          # Third-party license texts
├── config.example.toml   # Configuration reference
└── Cargo.toml            # Workspace manifest
```

---

## Configuration

Set your API key in **Zagens → Settings** or `~/.zagens/config.toml` ([config.example.toml](config.example.toml)).

Key sections: `provider` / `[providers.*]`, `default_text_model`, `reasoning_effort`, `sandbox_mode`, `approval_policy`, `[network]`, `[hooks]`, `[context]`, `[features]`.

The runtime token is generated at startup and injected via the Tauri sidecar environment — it never reaches the WebView.

---

## License

[MIT](LICENSE) — Copyright (c) 2024-2026 Zagens Contributors.

Additional attribution for the embedded runtime lineage: [NOTICE.md](NOTICE.md) and [third-party/deepseek-tui/LICENSE](third-party/deepseek-tui/LICENSE).
