<p align="center">
  <img src="assets/screenshot.png" alt="DS Pick Screenshot" width="800" />
</p>

# DS Pick

[English](#english) | [中文](#中文)

---

<h1 id="english">DS Pick — Desktop AI Coding Assistant</h1>

**DS Pick** is a native desktop GUI for [DeepSeek](https://deepseek.com/)'s AI-powered coding assistant, built with [Tauri 2](https://tauri.app/). It shares the same agent runtime as the `deepseek` CLI and `deepseek-tui` terminal UI—one config, one tool registry—with a windowed shell: system tray, turn notifications, workspace previews, session replay, and an embedded terminal in Code workspaces.

> **DS Pick** is the desktop product name (Tauri app in `crates/desktop/`). The monorepo also ships:
> - **`deepseek`** — CLI dispatcher (run, doctor, models, sessions, resume, fork, init, setup, exec, review, apply, eval, mcp, features, serve, login, logout, auth, config, model, thread, sandbox, app-server, mcp-server, metrics, update, completion, completions)
> - **`deepseek-tui`** — Terminal-based interactive UI
> - **Shared crates** — agent runtime, configuration, state, protocol, tools, MCP, hooks, execution policy, and more

---

## Table of Contents

- [What Makes This Project Different](#what-makes-this-project-different)
- [DS Pick (Desktop UI)](#ds-pick-desktop-ui)
- [Shared Runtime (CLI / TUI / Sidecar)](#shared-runtime-cli--tui--sidecar)
- [Tools (Runtime)](#tools-runtime)
- [AI Providers & Models](#ai-providers--models)
- [Security & Sandbox](#security--sandbox)
- [Tech Stack](#tech-stack)
- [Architecture](#architecture)
- [Prerequisites](#prerequisites)
- [Quick Start](#quick-start)
- [Development](#development)
- [Project Structure](#project-structure)
- [Configuration](#configuration)
- [License](#license)

> **Unofficial community project** — not affiliated with DeepSeek Inc. Capabilities below are what this repo ships today; the terminal TUI remains the most feature-complete surface. Desktop parity notes: [docs/desktop/TUI_DS_PICK_GAP.md](docs/desktop/TUI_DS_PICK_GAP.md). Harness / Agent 栈定位备忘: [docs/desktop/HARNESS.md](docs/desktop/HARNESS.md).

---

## What Makes This Project Different

| Theme | What you get |
|-------|----------------|
| **One engine, three surfaces** | The same agent runtime powers **`deepseek-tui`**, the **`deepseek`** CLI, and the DS Pick sidecar (`deepseek-tui serve --http`). Shared `~/.deepseek/config.toml`, sessions, and tools. |
| **Code vs Office modes** | **Code** and **Office** task types use different tool surfaces and prompts; switching modes starts a **new session** so model KV stays stable ([task-type architecture](docs/task-type-prompt-architecture.md)). |
| **CRAFT multi-agent** | Sub-agents with role-specific tool sets, structured fix-loop verdicts (PASS / BLOCKER / MAJOR / FAIL), and a **P1 blackboard** for handoffs ([CRAFT notes](docs/craft-v2-improvements.md)). |
| **Symbol index + bridges** | Lazy per-workspace index (`.deepseek/symbols.json`) for Rust / TS / Go / Python / C++ with call hints and **Tauri command bridges**; desktop Index panel + rebuild API. |
| **Office in the loop** | `read_file` extracts Office/PDF text; **`write_office`** builds `.xlsx` (Rust) and `.docx` / `.pptx` / `.pdf` (bundled Python). Workspace preview for common formats. |
| **Safety-first execution** | Layered exec policy, bash arity dictionary, per-domain network rules, path canonicalization, and **HTTP tool approval** in the desktop UI (default 120s timeout). |
| **Desktop-native shell** | System tray, turn-complete notifications, session replay, diff panel (diff2html), **embedded PTY** in Code workspaces, sidecar supervisor with health probes and crash backoff. |
| **DeepSeek V4–oriented** | Reasoning tiers (`off`→`max`), context compaction, workshop routing for large tool outputs, hallucination guardrails in base prompts ([prompt patch](docs/prompt-hallucination-patch.md)). |

---

## DS Pick (Desktop UI)

Features you interact with in the **v0.4.3** window (see [CHANGELOG.md](CHANGELOG.md)):

| Area | Shipped in DS Pick |
|------|-------------------|
| **Chat** | Multi-session sidebar, streaming + stop, thinking stream, context-usage bar, session/thread JSON export (Composer menu) |
| **Workspace panel** | File tree, previews (Markdown, code, images, CSV, Office text, Mermaid, hex for binary), **diff2html**, snapshot restore, **xterm.js PTY** (Code mode) |
| **Replay** | Turn-by-turn replay of tool calls and outputs |
| **Agents** | Sub-agent status panel (SSE); checklist sidebar when the model uses `checklist_write` |
| **Tasks & skills** | Background tasks + skill create/import/install UI; **scheduled automation list hidden** (API retained) |
| **Settings rail** | MCP panel, routing rules UI, usage/cost charts, model health, system settings (API key via keyring, vision, sub-agent limits) |
| **Shell** | System tray, native notifications when minimized, zh-Hans + English UI ([I18N plan](docs/desktop/I18N_PLAN.md)) |

**Not desktop-parity yet (runtime may still support):** rich TUI slash-command palette, per-message edit, “open in Explorer”, some advanced thread ops — see [TUI_DS_PICK_GAP.md](docs/desktop/TUI_DS_PICK_GAP.md).

---

## Shared Runtime (CLI / TUI / Sidecar)

Available through the agent on **all surfaces** (desktop uses the same sidecar as `deepseek serve --http`):

- **Sessions & threads** — SQLite-backed persistence, resume/fork, workspace snapshots (side-git under `~/.deepseek/snapshots/`)
- **MCP** — stdio servers, per-tool enable/disable, allow/deny filters
- **Skills** — `~/.deepseek/skills`, `SKILL.md`, `/skill install` (TUI/CLI), desktop `POST /v1/skills` + import/install APIs
- **Hooks** — lifecycle shell commands (`session_start`, `tool_call_before`, `shell_env`, …) with stdout / JSONL / webhook sinks
- **Providers** — multi-provider config, profiles, runtime provider switch; routing rules file (`routing_rules.json`)
- **Metrics** — `deepseek metrics` rollups from audit log + session store
- **Vision** — `describe_image` via configurable OpenAI-compatible vision endpoints (default **Qwen3-VL** in recent releases)

Layered system prompt: compile-time `prompts/base.md`, mode deltas (Agent / Plan / YOLO), `.deepseek/pick-rules.md`, project `AGENTS.md` (`deepseek init`), skills catalog, optional user memory.

---

## Tools (Runtime)

The runtime exposes a **broad tool registry** (not every tool has a dedicated desktop panel). Feature flags in `[features]` and task type trim what the model can call.

| Category | Representative tools / behavior |
|----------|-----------------------------------|
| **Files** | `read_file`, `write_file`, `edit_file`, `apply_patch`, `glob_files`, `grep_files` (ripgrep + symbol index) |
| **Repo** | `git_status`, `git_diff`, `git_log`, … |
| **Shell** | `exec_shell`, background `task_shell_*`, optional external sandbox |
| **Code quality** | LSP diagnostics after edits; `review`, `run_tests`, `diagnostics` |
| **Office** | `write_office`, Office/PDF via `read_file` |
| **Web** | `web_search`, `fetch_url`, `web.run` (when enabled) |
| **Vision** | `describe_image` |
| **Memory** | `remember`, `note`, opt-in `<user_memory>` |
| **Execution** | `code_execution`, `rlm` (long-context REPL helper) |

Implementation: `crates/tui/src/tools/`. HTTP routes: [`docs/tech/API_DESIGN.md`](docs/tech/API_DESIGN.md). Annotated config: [config.example.toml](config.example.toml).

---

## AI Providers & Models

Configured in `~/.deepseek/config.toml` ([config.example.toml](config.example.toml)). Model IDs change with upstream APIs — treat the table as **examples**, not guarantees.

| Provider | Example model IDs | API endpoint (typical) |
|----------|-------------------|-------------------------|
| **DeepSeek** | `deepseek-v4-pro`, `deepseek-v4-flash` | `api.deepseek.com/beta` |
| **NVIDIA NIM** | `deepseek-ai/deepseek-v4-pro`, … | `integrate.api.nvidia.com/v1` |
| **Fireworks AI** | `accounts/fireworks/models/deepseek-v4-pro` | `api.fireworks.ai/inference/v1` |
| **OpenRouter** | Provider catalog | `openrouter.ai/api/v1` |
| **Self-hosted** | SGLang / vLLM / Ollama tags | `localhost` OpenAI-compatible `/v1` |
| **Other** | Any OpenAI-compatible gateway | Custom `base_url` + headers |

**Also supported:**
- Store credentials for multiple providers simultaneously
- Runtime provider switch (TUI `/provider …`; desktop settings / shared config)
- Per-provider credentials, optional profiles (`[profiles.*]`)
- Env overrides (`DEEPSEEK_API_KEY`, etc.) and OS keyring (desktop sidecar)

---

## Security & Sandbox

### Execution Policy

| Mode | Description |
|------|------------|
| `read-only` | No shell execution, no file writes |
| `workspace-write` | Shell and writes allowed within the workspace directory only |
| `danger-full-access` | Full filesystem access (use with caution) |
| `external-sandbox` | Route all `exec_shell` calls through an OpenSandbox-compatible HTTP API |

### Approval System

| Feature | Description |
|---------|-------------|
| **Policy levels** | `on-request` (ask before risky ops), `untrusted` (ask before writes), `never` (auto-approve all) |
| **Interactive approval** | HTTP-based approve/deny with configurable timeout (default 120s) |
| **Bash arity dictionary** | Command prefix matching with arity awareness — `git status` is allowed but `git push` is not by default |
| **Layered rulesets** | Builtin → Agent → User priority; deny wins over allow |
| **Session approvals** | Remember approval decisions for the session |

### Network Policy

| Feature | Description |
|---------|-------------|
| **Per-domain rules** | Allow/deny lists for `fetch_url`, `web_search`, and MCP HTTP transport |
| **Wildcard support** | `.example.com` matches all subdomains |
| **Default modes** | `allow`, `deny`, or `prompt` (ask on first use) |
| **Audit logging** | One line per network call to `~/.deepseek/audit.log` |

### Additional Safeguards

| Feature | Description |
|---------|-------------|
| **Path canonicalization** | All file paths are canonicalized; `..` escape attempts are blocked |
| **Runtime token isolation** | Desktop sidecar token never touches the WebView `window` object |
| **CSP headers** | Content Security Policy enforced on the WebView |
| **Secrets management** | Credentials stored via OS keyring or encrypted config |

---

## Tech Stack

| Layer | Technology |
|-------|-----------|
| **Desktop Shell** | Rust + [Tauri 2](https://tauri.app/) |
| **Frontend** | React 18 + TypeScript (`strict: true`) + Tailwind CSS + Vite 6 |
| **Runtime** | Rust async (Tokio), sidecar process hosting the full `deepseek-tui` engine |
| **State** | SQLite via `deepseek-state` crate |
| **Markdown/Chat** | markdown-it, highlight.js, Mermaid diagrams, diff2html |
| **Terminal** | xterm.js for interactive PTY and shell output rendering |
| **Office** | Pure Rust XLSX engine; Python (`python-docx`, `python-pptx`, ReportLab) for DOCX/PPTX/PDF |
| **Symbol Index** | Regex-based lazy per-workspace index (`.deepseek/symbols.json`); Rust/TS/Go/Python/C++ and Tauri command bridges |

---

## Architecture

```
┌──────────────────────────────────────────────────────────────┐
│                     DS Pick (Tauri 2)                         │
│  ┌─────────────────┐  ┌───────────────────────────────────┐  │
│  │   WebView UI    │  │         Rust Shell                │  │
│  │   React / TS    │◄─┤  commands, sidecar supervisor,    │  │
│  │   Tailwind CSS  │  │  system tray, notifications       │  │
│  └────────┬────────┘  └───────────────┬───────────────────┘  │
│           │ HTTP + SSE                │ process mgmt          │
│           ▼                           ▼                       │
│  ┌─────────────────────────────────────────────────────────┐  │
│  │          Runtime API (deepseek-tui sidecar)              │  │
│  │  /v1/threads, /v1/skills, /v1/symbol-index, ...         │  │
│  │  Stream: thinking tokens + assistant text + tool calls   │  │
│  └───────────────────────┬─────────────────────────────────┘  │
│                          │                                    │
│  ┌───────────────────────▼─────────────────────────────────┐  │
│  │              Shared Crates                               │  │
│  │  ┌──────────┐ ┌──────────┐ ┌──────────┐ ┌──────────┐   │  │
│  │  │  agent   │ │   core   │ │  config  │ │  state   │   │  │
│  │  │  tools   │ │   mcp    │ │ protocol │ │  hooks   │   │  │
│  │  │execpolicy│ │ secrets  │ │ app-svr  │ │ tui-core │   │  │
│  │  └──────────┘ └──────────┘ └──────────┘ └──────────┘   │  │
│  └─────────────────────────────────────────────────────────┘  │
│                          │                                    │
│  ┌───────────────────────▼─────────────────────────────────┐  │
│  │              External Services                           │  │
│  │  DeepSeek API · NVIDIA NIM · Fireworks · SGLang/vLLM    │  │
│  │  Ollama (local) · MCP servers · Vision API · Sandbox     │  │
│  └─────────────────────────────────────────────────────────┘  │
└──────────────────────────────────────────────────────────────┘
```

### Runtime API (Phase 2+)

The sidecar (`deepseek-tui` binary, `serve --http` on loopback) exposes a local HTTP/SSE API. Authoritative route list: [`docs/tech/API_DESIGN.md`](docs/tech/API_DESIGN.md) and `crates/tui/src/runtime_api.rs` (`build_router`). Example endpoints on `127.0.0.1`:

| Endpoint | Purpose |
|----------|---------|
| `POST /v1/threads` | Create conversation thread |
| `GET /v1/threads` | List threads |
| `POST /v1/threads/:id/messages` | Send message (SSE streaming response) |
| `POST /v1/threads/:id/stop` | Stop active generation |
| `POST /v1/skills` | Create/manage skills |
| `GET /v1/workspace/status` | Workspace status and metadata |
| `POST /v1/symbol-index/rebuild` | Trigger symbol index rebuild |
| `POST /v1/threads/:id/turns/:turnId/resolve-approval` | Resolve pending approval request |

**SSE stream events:**
- `thinking` — Chain-of-thought reasoning tokens
- `text` — Assistant text delta
- `tool_call` — Tool invocation with arguments
- `tool_result` — Tool execution result
- `approval.required` — Pending tool approval (interactive)
- `done` — Turn complete (with usage stats)
- `error` — Error with code and message

---

## Runtime details

| Topic | Summary |
|-------|---------|
| **Prompt stack** | `prompts/base.md` → personality → mode (Agent / Plan / YOLO) → approval policy → workspace (`.deepseek/pick-rules.md`, `AGENTS.md`) → skills catalog + optional user memory |
| **Hooks** | `[hooks]` in config — e.g. `session_start`, `tool_call_before` / `after`, `shell_env` (parse `KEY=VALUE` into env); output to stdout, JSONL, or webhooks |
| **Snapshots** | Side-git under `~/.deepseek/snapshots/` (not your repo `.git`); restore with `/restore` or `revert_turn`; prune by `max_age_days` |
| **Context** | Auto-compaction (L1/L2/L3), optional Flash seams, workshop routing for oversized tool output, capacity pressure — `[context]`, `[workshop]`, `[capacity]` in [config.example.toml](config.example.toml) |

---

## Prerequisites

- **Rust** 1.88+ (stable)
- **Node.js** 20 LTS recommended (18+ may work for `web-ui` only)
- **Python** 3.8+ (for office document generation, code execution, RLM)
- **[Tauri CLI 2](https://v2.tauri.app/start/prerequisites/)** — `cargo install tauri-cli --version "^2"` (once per machine)
- Platform-specific Tauri [system dependencies](https://v2.tauri.app/start/prerequisites/)

**Versions:** DS Pick desktop **v0.4.3** (`crates/desktop/`); workspace CLI/TUI crates **0.8.15** (root `Cargo.toml`).

## Quick Start

```bash
# Clone the repository
git clone https://github.com/Hmbown/DeepSeek-TUI.git
cd DeepSeek-TUI

# Sidecar binary (build.rs copies debug/release into crates/desktop/binaries/)
cargo build -p deepseek-tui

# Web UI deps
cd crates/desktop/web-ui
npm install

# Dev: Tauri + Vite (beforeDevCommand starts web-ui on :1420)
cd ..
cargo tauri dev

# Configure API key (separate terminal or after first launch)
deepseek login --api-key "YOUR_DEEPSEEK_API_KEY"
```

Release / installer build (bundles sidecar + Python): from `crates/desktop`, run `npm run bundle:prepare` then `cargo tauri build`.

## Development

| Command | Description |
|---------|-------------|
| `cargo build --workspace` | Build all Rust crates |
| `cargo test --workspace --all-features` | Run all tests |
| `cargo clippy --workspace --all-targets --all-features` | Lint all Rust code |
| `cd crates/desktop/web-ui && npm run build` | Build the web UI (TypeScript + Vite) |
| `cd crates/desktop/web-ui && npm run dev` | Start Vite dev server (port 1420) |
| `cd crates/desktop && cargo tauri dev` | Launch DS Pick in development mode |
| `cd crates/desktop && npm run bundle:prepare` | Production web-ui + release sidecar + bundled Python |

## Project Structure

```
DeepSeek-TUI/
├── crates/
│   ├── desktop/          # DS Pick Tauri app
│   │   ├── web-ui/       # React/TypeScript frontend
│   │   │   ├── src/
│   │   │   │   ├── api/        # Runtime API client (SSE, REST)
│   │   │   │   ├── components/ # Chat, preview, terminal, diff, panels
│   │   │   │   ├── hooks/      # React hooks
│   │   │   │   ├── i18n/       # Internationalization (zh-Hans, en)
│   │   │   │   ├── lib/        # Utilities
│   │   │   │   ├── styles/     # Tailwind + custom styles
│   │   │   │   └── types/      # TypeScript type definitions
│   │   │   └── ...
│   │   └── src/          # Rust shell (commands, sidecar supervisor, system tray)
│   ├── cli/              # deepseek CLI dispatcher
│   ├── tui/              # Terminal UI engine (prompts, rendering, session mgmt)
│   ├── agent/            # AI agent runtime (model registry, resolution)
│   ├── core/             # Core data models & engine (threads, jobs, runtime)
│   ├── config/           # Configuration management (profiles, providers, vision)
│   ├── state/            # Persistent state (SQLite store)
│   ├── protocol/         # API protocol definitions (SSE events, messages)
│   ├── tools/            # Tool definitions, execution, registry
│   ├── mcp/              # Model Context Protocol (server mgmt, stdio transport)
│   ├── hooks/            # Agent hooks system (lifecycle, webhook, JSONL sinks)
│   ├── execpolicy/       # Execution policy engine (layered rulesets, bash arity)
│   ├── secrets/          # Credential storage (keyring, encrypted config)
│   ├── app-server/       # Experimental — not used by DS Pick (see deepseek app-server)
│   └── tui-core/         # Shared TUI core types
├── npm/deepseek-tui/     # npm distribution package
├── docs/                 # Documentation
│   ├── desktop/          # DS Pick (DEV_NOTES.md, implementation plans)
│   ├── tech/             # API_DESIGN.md, architecture notes
│   └── tui/              # TUI/CLI reference docs
├── assets/               # Screenshots & images
├── scripts/              # Build & release scripts
├── vendor/               # Vendored dependencies
├── config.example.toml   # Annotated configuration reference
└── Cargo.toml            # Workspace manifest
```

## Configuration

Set your DeepSeek API key before first use:

```bash
deepseek login --api-key "YOUR_DEEPSEEK_API_KEY"
```

Or configure directly in `~/.deepseek/config.toml` (see [config.example.toml](config.example.toml)).

**Key configuration sections:**

| Section | Purpose |
|---------|---------|
| `provider` / `api_key` / `base_url` | Active AI provider and credentials |
| `default_text_model` | Default model (e.g. `deepseek-v4-pro`) |
| `reasoning_effort` | Thinking mode: `off` / `low` / `medium` / `high` / `max` |
| `cost_currency` | `usd` or `cny` |
| `[providers.*]` | Per-provider credentials (deepseek, nvidia_nim, fireworks, sglang, vllm, ollama) |
| `[profiles.*]` | Named environment profiles |
| `allow_shell` / `approval_policy` / `sandbox_mode` | Security settings |
| `[network]` | Per-domain allow/deny rules |
| `[tui]` | Terminal UI preferences (locale, status items, mouse, OSC 8 links) |
| `[lsp]` | LSP server configuration per language |
| `[snapshots]` | Workspace snapshot settings |
| `[hooks]` | Lifecycle hook commands |
| `[notifications]` | Desktop notification settings |
| `[vision]` | Vision/OCR bridge configuration |
| `[memory]` | User memory feature toggle |
| `[skills]` | Community skill registry URL |
| `[context]` | Compaction thresholds (L1/L2/L3) |
| `[workshop]` | Large-output routing thresholds |
| `[capacity]` | Runtime pressure guardrails |
| `[retry]` | API retry configuration |
| `[subagents]` | Max concurrent sub-agents |
| `[features]` | Feature flags (shell_tool, subagents, web_search, apply_patch, mcp, exec_policy) |

DS Pick connects to the runtime automatically; the runtime token is generated at startup and injected securely through the Tauri sidecar environment — it never touches the WebView `window` object.

---

## CLI Quick Reference

```
deepseek [OPTIONS] [PROMPT]          # Interactive or one-shot session
deepseek run [PROMPT]                # Run via TUI binary
deepseek doctor                      # Run diagnostics
deepseek models                      # List available API models
deepseek sessions                    # List saved sessions
deepseek resume <id>                 # Resume a session
deepseek fork <id>                   # Fork a session
deepseek init                        # Create AGENTS.md in current directory
deepseek setup                       # Bootstrap MCP config and skills dirs
deepseek exec <prompt>               # Non-interactive agent command
deepseek review                      # AI code review over git diff
deepseek apply <patch>               # Apply patch to working tree
deepseek eval                        # Offline evaluation harness
deepseek mcp [args]                  # Manage MCP servers
deepseek features                    # Inspect feature flags
deepseek serve [--port N]            # Start local HTTP/SSE server
deepseek login --api-key <KEY>       # Save API key
deepseek logout                      # Remove saved auth
deepseek config [get|set|list]       # Read/write config
deepseek model [list|resolve]        # Model registry ops
deepseek thread [list|info|...]      # Thread/session management
deepseek sandbox <command>           # Evaluate sandbox policy
deepseek metrics                     # Usage rollup
deepseek update                      # Check for updates
deepseek completion <shell>          # Generate shell completions
```

## License

[MIT](LICENSE) © 2024-2025 DeepSeek CLI Contributors

> **Disclaimer:** This is an unofficial community project. Not affiliated with or endorsed by DeepSeek Inc.

---

<h1 id="中文">DS Pick — 桌面端 AI 编程助手</h1>

**DS Pick** 是基于 [Tauri 2](https://tauri.app/) 构建的 [DeepSeek](https://deepseek.com/) AI 编程助手原生桌面客户端。它封装了与 `deepseek` CLI 和 `deepseek-tui` 终端界面相同的经过实战检验的运行环境，提供精致的窗口化体验，包括系统托盘、通知以及基于 WebView 的丰富 UI。

> **DS Pick** 是桌面产品名称（`crates/desktop/` 下的 Tauri 应用）。此 monorepo 同时包含：
> - **`deepseek`** — CLI 调度器（run、doctor、models、sessions、resume、fork、init、setup、exec、review、apply、eval、mcp、features、serve、login、logout、auth、config、model、thread、sandbox、app-server、mcp-server、metrics、update、completion、completions）
> - **`deepseek-tui`** — 终端交互式界面
> - **共享 crates** — Agent 运行环境、配置、状态、协议、工具、MCP、Hooks、执行策略等

---

## 目录

- [项目特色](#项目特色)
- [DS Pick 桌面端](#ds-pick-桌面端)
- [共享运行环境](#共享运行环境)
- [安全与沙箱](#安全与沙箱)
- [技术栈](#技术栈)
- [架构](#架构)
- [环境要求](#环境要求)
- [快速开始](#快速开始)
- [开发指南](#开发指南)
- [项目结构](#项目结构)
- [配置](#配置)
- [开源协议](#开源协议)

> **非官方社区项目**，与 DeepSeek 公司无关。功能以本仓库 **v0.4.3** 为准；终端 TUI 仍是最完整交互面。桌面差距：[TUI_DS_PICK_GAP.md](docs/desktop/TUI_DS_PICK_GAP.md)。详细英文表见 [What Makes This Project Different](#what-makes-this-project-different)。

---

## 项目特色

| 方向 | 说明 |
|------|------|
| **一套引擎，三种入口** | `deepseek-tui`、`deepseek` CLI、DS Pick Sidecar 共用配置、会话与工具。 |
| **Code / Office 分场景** | 不同工具面与提示词；切换任务类型会**新开会话**以保持 KV 稳定。 |
| **CRAFT 多代理** | 角色化子代理、结构化 fix-loop 裁决、P1 黑板交接。 |
| **符号索引** | 懒加载 `.deepseek/symbols.json`，含 Tauri 命令桥接；桌面索引面板。 |
| **Office 流水线** | `read_file` / `write_office`（xlsx 用 Rust，docx/pptx/pdf 用捆绑 Python）。 |
| **安全优先** | 分层执行策略、bash 词典、域名网络规则、桌面 HTTP 工具审批。 |
| **桌面体验** | 托盘、完成通知、会话回放、diff 面板、Code 工作区 **PTY 终端**、Sidecar 监督重启。 |

---

## DS Pick 桌面端

**v0.4.3** 窗口内已具备：多会话聊天（流式/停止/思考/上下文条）、工作区预览与 **diff2html**、会话回放、子代理与清单侧栏、任务与技能（**定时自动化列表未展示**）、MCP/路由/用量/系统设置、托盘与通知、中/英 UI。

尚未与 TUI 完全对齐的交互见 [TUI_DS_PICK_GAP.md](docs/desktop/TUI_DS_PICK_GAP.md)。

---

## 共享运行环境

CLI / TUI / Sidecar 共用：SQLite 会话与线程、MCP、技能目录与安装、Hooks、多提供商与 `routing_rules.json`、`deepseek metrics`、可选视觉 `describe_image`、工作区快照、分层系统提示（`pick-rules.md`、`AGENTS.md`）。

工具实现：`crates/tui/src/tools/`；HTTP 契约：[`docs/tech/API_DESIGN.md`](docs/tech/API_DESIGN.md)。模型与提供商见英文 [AI Providers](#ai-providers--models)。

---

## 安全与沙箱

### 执行策略

| 模式 | 说明 |
|------|------|
| `read-only` | 不允许 Shell 执行和文件写入 |
| `workspace-write` | Shell 和写入仅限工作区目录内 |
| `danger-full-access` | 完整文件系统访问（请谨慎使用） |
| `external-sandbox` | 将所有 `exec_shell` 调用路由到兼容 OpenSandbox 的 HTTP API |

### 审批系统

| 功能 | 说明 |
|------|------|
| **策略级别** | `on-request`（高风险操作前询问）、`untrusted`（写入前询问）、`never`（全部自动批准） |
| **交互式审批** | 基于 HTTP 的批准/拒绝，可配置超时（默认 120 秒） |
| **Bash 参数词典** | 命令前缀匹配，含参数数量感知——`git status` 允许但 `git push` 默认不允许 |
| **分层规则集** | 内置 → Agent → 用户优先级；deny 优先于 allow |
| **会话级批准** | 在会话内记住批准决定 |

### 网络策略

| 功能 | 说明 |
|------|------|
| **按域名规则** | `fetch_url`、`web_search` 和 MCP HTTP 传输的 allow/deny 列表 |
| **通配符支持** | `.example.com` 匹配所有子域名 |
| **默认模式** | `allow`、`deny` 或 `prompt`（首次使用时询问） |
| **审计日志** | 每次网络调用一行记录到 `~/.deepseek/audit.log` |

### 额外保障

| 功能 | 说明 |
|------|------|
| **路径规范化** | 所有文件路径均经规范化；`..` 逃逸尝试被阻止 |
| **运行时 Token 隔离** | 桌面 Sidecar Token 绝不暴露到 WebView 的 `window` 对象 |
| **CSP 头** | WebView 强制内容安全策略 |
| **凭据管理** | 通过操作系统 keyring 或加密配置存储凭证 |

---

## 技术栈

| 层级 | 技术 |
|------|------|
| **桌面外壳** | Rust + [Tauri 2](https://tauri.app/) |
| **前端** | React 18 + TypeScript（`strict: true`）+ Tailwind CSS + Vite 6 |
| **运行环境** | Rust 异步（Tokio），以 Sidecar 方式托管完整 `deepseek-tui` 引擎 |
| **状态存储** | 基于 `deepseek-state` crate 的 SQLite |
| **Markdown/聊天** | markdown-it、highlight.js、Mermaid 图表、diff2html |
| **终端** | xterm.js 用于交互式 PTY 和 Shell 输出渲染 |
| **Office** | 纯 Rust XLSX 引擎；Python（`python-docx`、`python-pptx`、ReportLab）处理 DOCX/PPTX/PDF |
| **符号索引** | 按工作区懒加载正则索引（`.deepseek/symbols.json`）；Rust/TS/Go/Python/C++ 与 Tauri 桥接 |

---

## 架构

```
┌──────────────────────────────────────────────────────────────┐
│                     DS Pick (Tauri 2)                         │
│  ┌─────────────────┐  ┌───────────────────────────────────┐  │
│  │   WebView UI    │  │         Rust 外壳                 │  │
│  │   React / TS    │◄─┤  commands、sidecar 监督器、       │  │
│  │   Tailwind CSS  │  │  系统托盘、通知                   │  │
│  └────────┬────────┘  └───────────────┬───────────────────┘  │
│           │ HTTP + SSE                │ 进程管理              │
│           ▼                           ▼                       │
│  ┌─────────────────────────────────────────────────────────┐  │
│  │          Runtime API (deepseek-tui sidecar)              │  │
│  │  /v1/threads, /v1/skills, /v1/symbol-index, ...         │  │
│  │  流式：思考 Token + 助手文本 + 工具调用                   │  │
│  └───────────────────────┬─────────────────────────────────┘  │
│                          │                                    │
│  ┌───────────────────────▼─────────────────────────────────┐  │
│  │              共享 Crates                                 │  │
│  │  ┌──────────┐ ┌──────────┐ ┌──────────┐ ┌──────────┐   │  │
│  │  │  agent   │ │   core   │ │  config  │ │  state   │   │  │
│  │  │  tools   │ │   mcp    │ │ protocol │ │  hooks   │   │  │
│  │  │execpolicy│ │ secrets  │ │ app-svr  │ │ tui-core │   │  │
│  │  └──────────┘ └──────────┘ └──────────┘ └──────────┘   │  │
│  └─────────────────────────────────────────────────────────┘  │
│                          │                                    │
│  ┌───────────────────────▼─────────────────────────────────┐  │
│  │              外部服务                                    │  │
│  │  DeepSeek API · NVIDIA NIM · Fireworks · SGLang/vLLM    │  │
│  │  Ollama（本地）· MCP 服务器 · 视觉 API · 沙箱            │  │
│  └─────────────────────────────────────────────────────────┘  │
└──────────────────────────────────────────────────────────────┘
```

### Runtime API（Phase 2+）

Sidecar（`deepseek-tui` 二进制，`serve --http`）在 loopback 暴露 HTTP/SSE。完整路由见 [`docs/tech/API_DESIGN.md`](docs/tech/API_DESIGN.md) 与 `crates/tui/src/runtime_api.rs`。示例端点（`127.0.0.1`）：

| 端点 | 用途 |
|------|------|
| `POST /v1/threads` | 创建对话线程 |
| `GET /v1/threads` | 列出线程 |
| `POST /v1/threads/:id/messages` | 发送消息（SSE 流式响应） |
| `POST /v1/threads/:id/stop` | 停止当前生成 |
| `POST /v1/skills` | 创建/管理技能 |
| `GET /v1/workspace/status` | 工作区状态与元数据 |
| `POST /v1/symbol-index/rebuild` | 触发符号索引重建 |
| `POST /v1/threads/:id/turns/:turnId/resolve-approval` | 解决待处理审批请求 |

**SSE 流事件：**
- `thinking` — 思维链推理 Token
- `text` — 助手文本增量
- `tool_call` — 工具调用及参数
- `tool_result` — 工具执行结果
- `approval.required` — 待处理工具审批（交互式）
- `done` — 轮次完成（含用量统计）
- `error` — 错误，含代码和消息

---

运行时提示词分层、Hooks、快照与上下文压缩见英文 [Runtime details](#runtime-details)。

---

## 环境要求

- **Rust** 1.88+（稳定版）
- **Node.js** 建议 20 LTS（仅 web-ui 时 18+ 可能可用）
- **Python** 3.8+（用于 Office 文档生成、代码执行、RLM）
- **[Tauri CLI 2](https://v2.tauri.app/start/prerequisites/)** — `cargo install tauri-cli --version "^2"`
- 各平台 Tauri [系统依赖](https://v2.tauri.app/start/prerequisites/)

**版本：** DS Pick 桌面 **v0.4.3**；workspace CLI/TUI **0.8.15**（根 `Cargo.toml`）。

## 快速开始

```bash
# 克隆仓库
git clone https://github.com/Hmbown/DeepSeek-TUI.git
cd DeepSeek-TUI

# Sidecar（build.rs 会从 target/ 复制到 crates/desktop/binaries/）
cargo build -p deepseek-tui

# Web UI 依赖
cd crates/desktop/web-ui
npm install

# 开发：Tauri + Vite（beforeDevCommand 启动 :1420）
cd ..
cargo tauri dev

# API Key（另开终端或首次启动后）
deepseek login --api-key "YOUR_DEEPSEEK_API_KEY"
```

发布安装包：在 `crates/desktop` 执行 `npm run bundle:prepare`，再 `cargo tauri build`。

## 开发指南

| 命令 | 说明 |
|------|------|
| `cargo build --workspace` | 构建所有 Rust crates |
| `cargo test --workspace --all-features` | 运行所有测试 |
| `cargo clippy --workspace --all-targets --all-features` | 检查所有 Rust 代码 |
| `cd crates/desktop/web-ui && npm run build` | 构建 Web UI（TypeScript + Vite） |
| `cd crates/desktop/web-ui && npm run dev` | 启动 Vite 开发服务器（端口 1420） |
| `cd crates/desktop && cargo tauri dev` | 以开发模式启动 DS Pick |
| `cd crates/desktop && npm run bundle:prepare` | 生产 web-ui + release sidecar + 捆绑 Python |

## 项目结构

```
DeepSeek-TUI/
├── crates/
│   ├── desktop/          # DS Pick Tauri 应用
│   │   ├── web-ui/       # React/TypeScript 前端
│   │   │   ├── src/
│   │   │   │   ├── api/        # Runtime API 客户端（SSE、REST）
│   │   │   │   ├── components/ # 聊天、预览、终端、Diff、面板
│   │   │   │   ├── hooks/      # React Hooks
│   │   │   │   ├── i18n/       # 国际化（zh-Hans、en）
│   │   │   │   ├── lib/        # 工具函数
│   │   │   │   ├── styles/     # Tailwind + 自定义样式
│   │   │   │   └── types/      # TypeScript 类型定义
│   │   │   └── ...
│   │   └── src/          # Rust 外壳（commands、sidecar 监督器、系统托盘）
│   ├── cli/              # deepseek CLI 调度器
│   ├── tui/              # 终端界面引擎（提示词、渲染、会话管理）
│   ├── agent/            # AI Agent 运行环境（模型注册、解析）
│   ├── core/             # 核心数据模型与引擎（线程、任务、运行环境）
│   ├── config/           # 配置管理（配置文件、提供商、视觉）
│   ├── state/            # 持久化状态（SQLite 存储）
│   ├── protocol/         # API 协议定义（SSE 事件、消息）
│   ├── tools/            # 工具定义、执行、注册表
│   ├── mcp/              # 模型上下文协议（服务器管理、stdio 传输）
│   ├── hooks/            # Agent 钩子系统（生命周期、Webhook、JSONL 接收器）
│   ├── execpolicy/       # 执行策略引擎（分层规则集、bash 参数词典）
│   ├── secrets/          # 凭据存储（keyring、加密配置）
│   ├── app-server/       # Experimental — not used by DS Pick（实验性，不用于 DS Pick）
│   └── tui-core/         # 共享 TUI 核心类型
├── npm/deepseek-tui/     # npm 发布包
├── docs/                 # 文档
│   ├── desktop/          # DS Pick（DEV_NOTES.md、实施计划）
│   ├── tech/             # API_DESIGN.md 等
│   └── tui/              # TUI/CLI 参考
├── assets/               # 截图与图片
├── scripts/              # 构建与发布脚本
├── vendor/               # 内置依赖
├── config.example.toml   # 带注释的配置参考
└── Cargo.toml            # Workspace 清单
```

## 配置

首次使用前设置 DeepSeek API Key：

```bash
deepseek login --api-key "YOUR_DEEPSEEK_API_KEY"
```

或直接在 `~/.deepseek/config.toml` 中配置（参见 [config.example.toml](config.example.toml)）。

**主要配置段：**

| 配置段 | 用途 |
|--------|------|
| `provider` / `api_key` / `base_url` | 当前 AI 提供商与凭证 |
| `default_text_model` | 默认模型（如 `deepseek-v4-pro`） |
| `reasoning_effort` | 思考模式：`off` / `low` / `medium` / `high` / `max` |
| `cost_currency` | `usd` 或 `cny` |
| `[providers.*]` | 各提供商凭证（deepseek、nvidia_nim、fireworks、sglang、vllm、ollama） |
| `[profiles.*]` | 命名环境配置文件 |
| `allow_shell` / `approval_policy` / `sandbox_mode` | 安全设置 |
| `[network]` | 按域名 allow/deny 规则 |
| `[tui]` | 终端 UI 偏好（语言、状态栏项目、鼠标、OSC 8 链接） |
| `[lsp]` | 各语言的 LSP 服务器配置 |
| `[snapshots]` | 工作区快照设置 |
| `[hooks]` | 生命周期钩子命令 |
| `[notifications]` | 桌面通知设置 |
| `[vision]` | 视觉/OCR bridge 配置 |
| `[memory]` | 用户记忆功能开关 |
| `[skills]` | 社区技能注册表 URL |
| `[context]` | 压缩阈值（L1/L2/L3） |
| `[workshop]` | 大输出路由阈值 |
| `[capacity]` | 运行时压力防护 |
| `[retry]` | API 重试配置 |
| `[subagents]` | 最大并发子代理数 |
| `[features]` | 功能开关（shell_tool、subagents、web_search、apply_patch、mcp、exec_policy） |

DS Pick 自动连接到运行环境；运行环境 Token 在启动时生成，通过 Tauri Sidecar 环境变量安全注入——绝不会暴露到 WebView 的 `window` 对象。

---

## CLI 快速参考

```
deepseek [OPTIONS] [PROMPT]          # 交互式或一次性会话
deepseek run [PROMPT]                # 通过 TUI 二进制运行
deepseek doctor                      # 运行诊断
deepseek models                      # 列出可用 API 模型
deepseek sessions                    # 列出已保存会话
deepseek resume <id>                 # 恢复会话
deepseek fork <id>                   # 派生会话
deepseek init                        # 在当前目录创建 AGENTS.md
deepseek setup                       # 初始化 MCP 配置和技能目录
deepseek exec <prompt>               # 非交互式 Agent 命令
deepseek review                      # 基于 AI 的 Git Diff 代码评审
deepseek apply <patch>               # 应用补丁到工作树
deepseek eval                        # 离线评估工具
deepseek mcp [args]                  # 管理 MCP 服务器
deepseek features                    # 查看功能开关
deepseek serve [--port N]            # 启动本地 HTTP/SSE 服务器
deepseek login --api-key <KEY>       # 保存 API Key
deepseek logout                      # 移除已保存的认证信息
deepseek config [get|set|list]       # 读取/写入配置
deepseek model [list|resolve]        # 模型注册表操作
deepseek thread [list|info|...]      # 线程/会话管理
deepseek sandbox <command>           # 评估沙箱策略
deepseek metrics                     # 用量汇总
deepseek update                      # 检查更新
deepseek completion <shell>          # 生成 Shell 自动补全脚本
```

## 开源协议

[MIT](LICENSE) © 2024-2025 DeepSeek CLI Contributors

> **声明：** 本项目为非官方社区项目，与 DeepSeek Inc. 无关联，亦未获其认可。
