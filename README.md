
<p align="center">
  <img src="assets/screenshot.png" alt="DS Pick Screenshot" width="800" />
</p>

# DS Pick

[English](#english) | [中文](#中文)

---

<h1 id="english">DS Pick — Desktop AI Coding Assistant</h1>

**DS Pick** is a native desktop GUI for [DeepSeek](https://deepseek.com/)'s AI-powered coding assistant, built with [Tauri](https://tauri.app/). It wraps the same battle-tested runtime as the `deepseek` CLI and `deepseek-tui` terminal interface, delivering a polished, windowed experience with system tray, notifications, and a rich WebView-based UI.

> **DS Pick** is the desktop product name (Tauri app in `crates/desktop/`). The monorepo also ships:
> - **`deepseek`** — CLI dispatcher
> - **`deepseek-tui`** — Terminal-based interactive UI
> - **Shared crates** — agent runtime, configuration, state, protocol, tools, and more

## Key Features

| Category | Highlights |
|----------|------------|
| **Chat & Sessions** | Multi-threaded conversation model; create, rename, archive, resume sessions; streaming responses with stop support |
| **Coding Tools** | File read/write/edit, search, directory tree, `edit_file` / `apply_patch` with diff visualization, shell command execution |
| **AI Providers** | DeepSeek (V4 Pro / Flash), NVIDIA NIM, OpenAI, OpenRouter, Novita, Fireworks, SGLang, vLLM, Ollama |
| **Sub-agents** | Multi-agent orchestration with live status panel |
| **MCP Servers** | Connect Model Context Protocol servers; browse tools, enable/disable |
| **Skills & Tasks** | Create and manage agent skills; task automation |
| **Smart Routing** | Automatic model routing rules with configuration UI |
| **Usage Dashboard** | Cost and token usage tracking with interactive charts |
| **Preview Panel** | Markdown, code, images, CSV, binary files — rendered in the workspace panel |
| **Approval Gates** | Interactive HTTP-based tool approval (approve/deny with configurable timeout) |
| **Offline Resilience** | SSE reconnection with exponential backoff, sidecar auto-restart, runtime health probes |

## Tech Stack

| Layer | Technology |
|-------|-----------|
| **Desktop Shell** | Rust + [Tauri 2](https://tauri.app/) |
| **Frontend** | React 18 + TypeScript + Tailwind CSS + Vite 6 |
| **Runtime** | Rust async (Tokio), sidecar process hosting the full `deepseek-tui` engine |
| **Markdown/Chat** | markdown-it, highlight.js, Mermaid diagrams, diff2html |
| **Terminal** | xterm.js for shell output rendering |

## Architecture

```
┌──────────────────────────────────────────────┐
│                  DS Pick (Tauri)              │
│  ┌──────────────┐  ┌──────────────────────┐  │
│  │  WebView UI  │  │   Rust Shell         │  │
│  │  React/TS    │◄─┤   commands, sidecar  │  │
│  └──────┬───────┘  └──────────┬───────────┘  │
│         │ HTTP + SSE          │ process mgmt │
│         ▼                     ▼              │
│  ┌──────────────────────────────────────┐    │
│  │     Runtime API (deepseek-tui)       │    │
│  │     /v1/threads, /v1/skills, ...     │    │
│  └──────────────────┬───────────────────┘    │
│                     │                        │
│  ┌──────────────────▼───────────────────┐    │
│  │     Shared Crates                    │    │
│  │  agent | core | config | state       │    │
│  │  tools | mcp  | protocol | hooks     │    │
│  └──────────────────────────────────────┘    │
└──────────────────────────────────────────────┘
```

## Prerequisites

- **Rust** 1.88+ (stable)
- **Node.js** 18+
- **Python** 3.8+ (for office document generation)
- Platform-specific Tauri [prerequisites](https://v2.tauri.app/start/prerequisites/)

## Quick Start

```bash
# Clone the repository
git clone https://github.com/Hmbown/DeepSeek-TUI.git
cd DeepSeek-TUI

# Build the CLI/TUI runtime first (used as sidecar)
cargo build --release -p deepseek-tui-cli -p deepseek-tui

# Copy binaries to the desktop bundle location
# (or run the prepare-bundle script)

# Install web UI dependencies
cd crates/desktop/web-ui
npm install

# Start the dev server (launches Tauri + Vite dev)
cd ..
npm run tauri dev
```

## Development

| Command | Description |
|---------|-------------|
| `cargo build --workspace` | Build all Rust crates |
| `cargo test --workspace --all-features` | Run all tests |
| `cargo clippy --workspace --all-targets --all-features` | Lint all Rust code |
| `cd crates/desktop/web-ui && npm run build` | Build the web UI (TypeScript + Vite) |
| `cd crates/desktop/web-ui && npm run dev` | Start Vite dev server (port 1420) |
| `cd crates/desktop && npm run tauri dev` | Launch DS Pick in development mode |

## Project Structure

```
DeepSeek-TUI/
├── crates/
│   ├── desktop/          # DS Pick Tauri app
│   │   ├── web-ui/       # React/TypeScript frontend
│   │   └── src/          # Rust shell (commands, sidecar)
│   ├── cli/              # deepseek CLI dispatcher
│   ├── tui/              # Terminal UI engine
│   ├── agent/            # AI agent runtime
│   ├── core/             # Core data models & engine
│   ├── config/           # Configuration management
│   ├── state/            # Persistent state (SQLite)
│   ├── protocol/         # API protocol definitions
│   ├── tools/            # Tool definitions & execution
│   ├── mcp/              # Model Context Protocol
│   ├── hooks/            # Agent hooks system
│   ├── execpolicy/       # Execution policy engine
│   ├── secrets/          # Credential storage
│   ├── app-server/       # HTTP runtime server
│   └── tui-core/         # Shared TUI core types
├── npm/deepseek-tui/     # npm distribution package
├── docs/                 # Documentation
├── assets/               # Screenshots & images
└── Cargo.toml            # Workspace manifest
```

## Configuration

Set your DeepSeek API key before first use:

```bash
deepseek login --api-key "YOUR_DEEPSEEK_API_KEY"
```

Or configure directly in `~/.deepseek/config.toml` (see [config.example.toml](config.example.toml)).

DS Pick connects to the runtime automatically; the runtime token is generated at startup and injected securely through the Tauri sidecar environment — it never touches the WebView `window` object.

## License

[MIT](LICENSE) © 2024-2025 DeepSeek CLI Contributors

> **Disclaimer:** This is an unofficial community project. Not affiliated with or endorsed by DeepSeek Inc.

---

<h1 id="中文">DS Pick — 桌面端 AI 编程助手</h1>

**DS Pick** 是基于 [Tauri](https://tauri.app/) 构建的 [DeepSeek](https://deepseek.com/) AI 编程助手原生桌面客户端。它封装了与 `deepseek` CLI 和 `deepseek-tui` 终端界面相同的经过实战检验的运行环境，提供精致的窗口化体验，包括系统托盘、通知以及基于 WebView 的丰富 UI。

> **DS Pick** 是桌面产品名称（`crates/desktop/` 下的 Tauri 应用）。此 monorepo 同时包含：
> - **`deepseek`** — CLI 调度器
> - **`deepseek-tui`** — 终端交互式界面
> - **共享 crates** — Agent 运行环境、配置、状态、协议、工具等

## 核心功能

| 类别 | 亮点 |
|------|------|
| **对话与会话** | 多线程对话模型；创建、重命名、归档、恢复会话；支持流式输出与停止 |
| **编程工具** | 文件读写/编辑、搜索、目录树、`edit_file` / `apply_patch` 差异化对比、Shell 命令执行 |
| **AI 提供商** | DeepSeek（V4 Pro / Flash）、NVIDIA NIM、OpenAI、OpenRouter、Novita、Fireworks、SGLang、vLLM、Ollama |
| **子代理** | 多代理编排，实时状态面板 |
| **MCP 服务器** | 接入 Model Context Protocol 服务器；浏览工具、启用/禁用 |
| **技能与任务** | 创建和管理 Agent 技能；任务自动化 |
| **智能路由** | 自动模型路由规则，配备配置界面 |
| **用量面板** | 费用与 Token 用量追踪，交互式图表 |
| **预览面板** | Markdown、代码、图片、CSV、二进制文件 — 在工作区面板中渲染 |
| **审批门控** | 基于 HTTP 的交互式工具审批（批准/拒绝，可配置超时） |
| **离线韧性** | SSE 指数退避重连、Sidecar 自动重启、运行时健康探测 |

## 技术栈

| 层级 | 技术 |
|------|------|
| **桌面外壳** | Rust + [Tauri 2](https://tauri.app/) |
| **前端** | React 18 + TypeScript + Tailwind CSS + Vite 6 |
| **运行环境** | Rust 异步 (Tokio)，以 Sidecar 方式托管完整 `deepseek-tui` 引擎 |
| **Markdown/聊天** | markdown-it、highlight.js、Mermaid 图表、diff2html |
| **终端** | xterm.js 渲染 Shell 输出 |

## 架构

```
┌──────────────────────────────────────────────┐
│                  DS Pick (Tauri)              │
│  ┌──────────────┐  ┌──────────────────────┐  │
│  │  WebView UI  │  │   Rust Shell         │  │
│  │  React/TS    │◄─┤   commands, sidecar  │  │
│  └──────┬───────┘  └──────────┬───────────┘  │
│         │ HTTP + SSE          │ 进程管理      │
│         ▼                     ▼              │
│  ┌──────────────────────────────────────┐    │
│  │     Runtime API (deepseek-tui)       │    │
│  │     /v1/threads, /v1/skills, ...     │    │
│  └──────────────────┬───────────────────┘    │
│                     │                        │
│  ┌──────────────────▼───────────────────┐    │
│  │     Shared Crates                    │    │
│  │  agent | core | config | state       │    │
│  │  tools | mcp  | protocol | hooks     │    │
│  └──────────────────────────────────────┘    │
└──────────────────────────────────────────────┘
```

## 环境要求

- **Rust** 1.88+（稳定版）
- **Node.js** 18+
- **Python** 3.8+（用于 Office 文档生成）
- 各平台对应的 Tauri [系统依赖](https://v2.tauri.app/start/prerequisites/)

## 快速开始

```bash
# 克隆仓库
git clone https://github.com/Hmbown/DeepSeek-TUI.git
cd DeepSeek-TUI

# 先构建 CLI/TUI 运行环境（作为 Sidecar 使用）
cargo build --release -p deepseek-tui-cli -p deepseek-tui

# 将编译产物复制到桌面打包目录
# （或运行 prepare-bundle 脚本）

# 安装 Web UI 依赖
cd crates/desktop/web-ui
npm install

# 启动开发服务器（同时启动 Tauri + Vite 开发模式）
cd ..
npm run tauri dev
```

## 开发指南

| 命令 | 说明 |
|------|------|
| `cargo build --workspace` | 构建所有 Rust crates |
| `cargo test --workspace --all-features` | 运行所有测试 |
| `cargo clippy --workspace --all-targets --all-features` | 检查所有 Rust 代码 |
| `cd crates/desktop/web-ui && npm run build` | 构建 Web UI（TypeScript + Vite） |
| `cd crates/desktop/web-ui && npm run dev` | 启动 Vite 开发服务器（端口 1420） |
| `cd crates/desktop && npm run tauri dev` | 以开发模式启动 DS Pick |

## 项目结构

```
DeepSeek-TUI/
├── crates/
│   ├── desktop/          # DS Pick Tauri 应用
│   │   ├── web-ui/       # React/TypeScript 前端
│   │   └── src/          # Rust 外壳（commands、sidecar）
│   ├── cli/              # deepseek CLI 调度器
│   ├── tui/              # 终端界面引擎
│   ├── agent/            # AI Agent 运行环境
│   ├── core/             # 核心数据模型与引擎
│   ├── config/           # 配置管理
│   ├── state/            # 持久化状态（SQLite）
│   ├── protocol/         # API 协议定义
│   ├── tools/            # 工具定义与执行
│   ├── mcp/              # Model Context Protocol
│   ├── hooks/            # Agent 钩子系统
│   ├── execpolicy/       # 执行策略引擎
│   ├── secrets/          # 凭据存储
│   ├── app-server/       # HTTP 运行环境服务器
│   └── tui-core/         # 共享 TUI 核心类型
├── npm/deepseek-tui/     # npm 发布包
├── docs/                 # 文档
├── assets/               # 截图与图片
└── Cargo.toml            # Workspace 清单
```

## 配置

首次使用前设置 DeepSeek API Key：

```bash
deepseek login --api-key "YOUR_DEEPSEEK_API_KEY"
```

或直接在 `~/.deepseek/config.toml` 中配置（参见 [config.example.toml](config.example.toml)）。

DS Pick 自动连接到运行环境；运行环境 Token 在启动时生成，通过 Tauri Sidecar 环境变量安全注入——绝不会暴露到 WebView 的 `window` 对象。

## 开源协议

[MIT](LICENSE) © 2024-2025 DeepSeek CLI Contributors

> **声明：** 本项目为非官方社区项目，与 DeepSeek Inc. 无关联，亦未获其认可。
