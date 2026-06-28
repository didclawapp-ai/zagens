<p align="center">
  <img src="assets/screenshot.png" alt="Zagens 截图" width="800" />
</p>

# Zagens — 面向 DeepSeek V4 的开源 Agent harness

**[English](README.md)** · **[日本語](README.ja.md)** · **[Português (BR)](README.pt-BR.md)** | 中文

长程 Agent 任务容易**半途停下或过早「声称完成」**；写代码和改 Office 往往**各用一套工具**；本地执行还需要**回放、审批和可审计性**——而不只是多开一个聊天窗口。

**Zagens** 是面向 **[DeepSeek V4](https://deepseek.com/)** 的开源 Agent harness。

> **作者语：** 不要相信 AI Agent 能做任何事情，它是有边界的；我们能做的，就是拓展这种边界。

> **许可：** [MIT](LICENSE)。Runtime 谱系：[NOTICE.md](NOTICE.md) · [third-party/deepseek-tui/](third-party/deepseek-tui/)。以下以 **Zagens v0.8.5** 为准 — 见 [CHANGELOG.md](CHANGELOG.md)。

| 资源 | 链接 |
|------|------|
| 用户文档 | [zagens.com/docs](https://zagens.com/docs) |
| 安装包 | [GitHub Releases](https://github.com/didclawapp-ai/zagens/releases)（最新 **`zagens-v0.8.5`**）· [zagens.com/download](https://zagens.com/download) |
| 设计规格 | [`docs/README.md`](docs/README.md) |
| 贡献指南 | [`CONTRIBUTING.md`](CONTRIBUTING.md) · [`LOCAL_DEV_VERIFY.md`](LOCAL_DEV_VERIFY.md) |
| 安全策略 | [`SECURITY.md`](SECURITY.md) |

---

## 适合谁 / 不太适合谁

| 更适合 | 不太适合 |
|--------|----------|
| **DeepSeek 深度用户** — 日常用 DeepSeek API / V4 做编码与 Agent 工作流，想要比官方工具更强的本地 Agent 平台 | 托管 SaaS、包月模型、零配置云端 |
| 想要**独立 Agent 平台**（桌面 / 终端 / CLI，不绑死某一 IDE 插件）的开发者 | 纯聊天、无工具、无工作区、无回放 |
| **终端优先**用户（macOS / Linux / Windows）— 全屏 **`zagens-tui`**，与桌面同引擎 | 完全自主、无护栏的 YOLO Agent |
| 做**长程代码重构**或**Office 交付物**、希望同一工作流的人 | 零安装的移动端 / 纯浏览器体验 |
| 在意**本地 sidecar**、MCP/技能、UI 内**执行审批**的用户 | 只需网页 Copilot、不需要本地执行能力的团队 |
| 当前 **Windows 桌面**用户；macOS/Linux 可用 **TUI**、**CLI** 或源码构建 | |

---

## 三条差异化

**1. Harness，不是聊天壳** — 长程代码任务用**可组合完成门禁**（操作者 / 模型 / 工具链分层），而不是「模型说做完了就算完」。规格：[LHT](docs/harness/LONG_HORIZON_CODE_TASKS.md) · 夹具：[`fixtures/harness/`](fixtures/harness/)。

**2. 多入口，一套引擎** — [Tauri 2](https://tauri.app/) 桌面 **或** 全屏 **`zagens-tui`**（ratatui）**或** 无 GUI 的 **`zagens`** CLI — 均运行 **Kernel V3**（`LiveTurnMachine` + `EffectInterpreter`，事件溯源 turn、log-first 会话恢复）。桌面提供托盘、WebView 面板、嵌入式 PTY 与 sidecar 监督；TUI 在终端内提供三栏 transcript/composer/inspector 与 LHT 面板。

**3. Code + Office，一套 runtime** — **Code / Office** 任务类型共享配置与工具面，但提示词与工具集不同；切换类型会**新开会话**以保持 KV 稳定（[架构说明](docs/task-type-prompt-architecture.md)）。Office：`read_file` / **`write_office`**（xlsx 用 Rust；docx/pptx/pdf 用捆绑 Python）。

另有：**CRAFT 多代理**（子代理、fix-loop 裁决、P1 黑板 — [说明](docs/craft-v2-improvements.md)）、懒加载**符号索引**（`.zagens/symbols.json`）、MCP、技能、Hooks、定时任务、**`batch_edit`** / **`refactor_imports`** 批量代码工具。

---

## 我们重点解决什么问题

| 痛点 | Zagens 的做法 |
|------|----------------|
| Agent 中途停手或过早标记完成 | **分层完成门禁** + 长程任务面板（[组合式 Harness](docs/harness/COMPOSABLE_HARNESS.md)） |
| IDE 插件与终端 Agent 会话割裂 | 统一 **sidecar** + SQLite 线程、分叉/恢复、**回放**、工作区快照 |
| 表格与文档在编码 Agent 流程之外 | **Office 模式** + `write_office` + 桌面预览 |
| 本地跑工具又不能盲信 | 执行策略、网络规则、路径规范化、审批 UI、运行时 Token 不进 WebView（[沙箱矩阵](docs/tech/SANDBOX_CAPABILITY_MATRIX.md)） |

---

## 当前已具备（v0.8.5）

**上下文过渡（P0–P4）：** **Context Explorer**（TUI **Context** 面板 + 桌面 token 分解）；messages 层 `[COMPACTED_HISTORY]` 压缩摘要；模型切换时 seam 热重载；容量 **`VerifyAndReplan`** 回退链。**`zagens doctor`** 上下文诊断；**`zagens://open`** 深链接。

**Kernel V3 引擎：** 事件溯源 turn 循环 — `sessions.db` 中的 `KernelEvent` 日志、`LiveTurnMachine` 规划、`EffectInterpreter` IO、golden 重放夹具。规格：[AGENT_KERNEL_V3.md](docs/tech/AGENT_KERNEL_V3.md)。

**桌面（Tauri）：** **Dusk** 第三主题；**git worktree** 并行会话；**检查点/回滚 UI** 与 **channels**；多会话并行流式；**模型接入面板**；会话级配置覆盖；集成 PTY 终端（光标/ANSI/URL 链接）；**Kernel Trace Report** 导出。中/英/日/葡 UI。

**终端 TUI（`zagens-tui`）：** 全屏三栏 — 会话侧栏、流式 transcript、composer（`/model`、`/lht`）、审批弹窗、inspector（文件 / diff / checklist / **context** / agents / MCP）、可折叠 LHT 下 pane、主题预设、会话恢复（`--fresh` 新建）。与桌面共用 runtime 线程与 Kernel V3 路径。

**Runtime：** 线程、MCP、技能、Hooks、多提供商路由、视觉；**`GET/PUT/DELETE /v1/threads/{id}/config`**；全局 **`thread.status`** SSE；**`POST /v1/threads/{id}/events`** 通道注入。

**工具（代表）：** 文件、git、`exec_shell`、`write_office`、可选 `web_search` / `fetch_url`、记忆工具。完整列表：`crates/runtime-server/src/tools/` · [CHANGELOG.md](CHANGELOG.md)。

---

## 已知限制（使用前请了解）

我们更愿意写清边界，而不是堆功能清单。

| 主题 | 现状 |
|------|------|
| **桌面安装包** | **Windows** 安装包见 [Releases](https://github.com/didclawapp-ai/zagens/releases)。**macOS / Linux 桌面包** — 规划中。三平台 **`zagens` CLI** 与 **`zagens-tui`** 已发。 |
| **OS 级沙箱** | **macOS Seatbelt** — 在 `sandbox-exec` 可用时强制执行。**Windows** — 原生沙箱已实现（`elevated` 推荐：`zagens sandbox setup` 后强制隔离；`unelevated` 为工作区写隔离）。设置 → **Sandbox** 首次引导。**Linux** — 策略已声明，**尚未 OS 级强制**（degraded）。详见 [`SANDBOX_CAPABILITY_MATRIX.md`](docs/tech/SANDBOX_CAPABILITY_MATRIX.md)。 |
| **模型与 Key** | 面向 **DeepSeek V4**（Pro / Flash）深度优化；需自备 API Key。亦支持其他 OpenAI 兼容端点 — **我们不托管模型**。 |
| **长程与多代理** | 门禁与 CRAFT **可用且在持续演进**，边界场景与新门禁类型在活跃开发中。 |
| **Office 深度** | 核心读写路径已有；企业连接器、语音及部分场景模板为**后续方向**（[Office 场景](docs/desktop/OFFICE_SCENARIOS.md)）。 |

安全问题请走 [`SECURITY.md`](SECURITY.md)。

---

## 我们在往哪走

公开设计规格见 [`docs/`](docs/README.md)。方向包括：

- **平台对齐** — macOS/Linux 桌面安装包；**Linux** 原生沙箱（Landlock/bwrap）。Windows 原生沙箱已在 0.7.x 落地。
- **可信长任务** — 更严完成门禁、Harness 夹具、可回放的操作者工作流。
- **Office 工作流** — 更丰富的场景覆盖，且不脱离统一 runtime。
- **安全加固** — 见 [CHANGELOG](CHANGELOG.md) 与 [SECURITY.md](SECURITY.md)。

---

## 快速开始

### Zagens 桌面（Windows）

[GitHub Releases](https://github.com/didclawapp-ai/zagens/releases) 提供 **Windows** 桌面安装包（`*-setup.exe.zip`）。macOS / Linux 桌面包规划中。SmartScreen：[SMARTSCREEN.md](docs/desktop/SMARTSCREEN.md)。

### CLI 与 TUI — 按平台

| 入口 | Linux | macOS | Windows |
|------|-------|-------|---------|
| **`zagens-tui`**（全屏终端 UI） | ✅ | ✅ | ✅ |
| **`zagens`**（无 GUI CLI） | ✅ | ✅ | ✅ |
| **桌面应用** | —（用 TUI） | —（用 TUI） | ✅ 安装包 |

可通过 **预编译包**（[Releases `zagens-v0.8.5`](https://github.com/didclawapp-ai/zagens/releases/tag/zagens-v0.8.5)）、**`cargo install`**（crates.io）或 **源码构建**（见下）安装。

**Rust 前置**（仅 `cargo install` / 源码需要）：安装 [rustup](https://rustup.rs/)（Rust **1.88+**；CI 使用 1.96）。Linux/macOS 执行 `source "$HOME/.cargo/env"`，Windows 重开终端。

#### Linux（Ubuntu / Debian）

```bash
sudo apt update
sudo apt install -y build-essential curl pkg-config libssl-dev libdbus-1-dev
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
source "$HOME/.cargo/env"

# TUI（首次编译约 10–30 分钟）
cargo install zagens-cli --version 0.8.5 --bin zagens-tui --features tui --locked

# 无 GUI CLI（可选）
cargo install zagens-cli --version 0.8.5 --bin zagens --locked
```

**预编译**（无需 Rust）：从 [Releases](https://github.com/didclawapp-ai/zagens/releases/tag/zagens-v0.8.5) 下载 `zagens-tui-x86_64-unknown-linux-gnu` 和/或 `zagens-x86_64-unknown-linux-gnu`，校验对应 `.sha256`，`chmod +x` 后放入 `PATH` 目录。

```bash
zagens-tui              # 恢复上次会话
zagens-tui --fresh      # 新建会话
```

#### macOS

```bash
xcode-select --install    # 若缺少 C 工具链
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
source "$HOME/.cargo/env"

cargo install zagens-cli --version 0.8.5 --bin zagens-tui --features tui --locked
cargo install zagens-cli --version 0.8.5 --bin zagens --locked   # 可选
```

**预编译：** [Releases](https://github.com/didclawapp-ai/zagens/releases/tag/zagens-v0.8.5) 上的 `zagens-tui-x86_64-apple-darwin`（Intel）或 `zagens-tui-aarch64-apple-darwin`（Apple Silicon）。

#### Windows

**预编译（最快）：** [Releases](https://github.com/didclawapp-ai/zagens/releases/tag/zagens-v0.8.5) — `zagens-tui-x86_64-pc-windows-msvc.exe`、`zagens-x86_64-pc-windows-msvc.exe`（及 `.sha256`）。将目录加入 `PATH`，或把 `.exe` 复制到已在 `PATH` 中的文件夹。

**crates.io**（先安装 [Rust for Windows](https://rustup.rs/)）：

```powershell
cargo install zagens-cli --version 0.8.5 --bin zagens-tui --features tui --locked
cargo install zagens-cli --version 0.8.5 --bin zagens --locked
```

### crates.io（全平台）

```bash
cargo install zagens-cli --version 0.8.5 --bin zagens-tui --features tui --locked   # TUI
cargo install zagens-cli --version 0.8.5 --bin zagens --locked                   # CLI
cargo install zagens-cli --version 0.8.5 --bin zagens-runtime --locked           # HTTP sidecar（可选）
```

### 从源码 — 桌面

```bash
git clone https://github.com/didclawapp-ai/zagens.git
cd zagens

cargo build -p zagens-cli          # 将 zagens-runtime 复制到 crates/desktop/binaries/

cd crates/desktop/web-ui && npm install
cd .. && cargo tauri dev

# API Key：Zagens 设置，或 ~/.zagens/config.toml
```

### 从源码 — 终端 TUI

```bash
cargo build -p zagens-cli --features tui --bin zagens-tui
./target/debug/zagens-tui          # 恢复上次会话；--fresh 新建会话
```

**API Key：** `DEEPSEEK_API_KEY`、`~/.zagens/config.toml`、TUI 内 `/api-key` / 首次引导，或 `zagens login --api-key <key>`。旧版 `~/.deepseek/config.toml` 仍可读取，新安装默认使用 `~/.zagens/`。

**CLI 示例：**

```bash
zagens doctor
zagens exec '总结 src/ 变更' --json
zagens exec '重构 auth 模块' --auto
zagens serve --http --port 7878
```

配置样例：[config.example.toml](config.example.toml)。

---

## 架构

```
┌──────────────────┐  ┌──────────────────┐  ┌──────────────────┐
│  Zagens 桌面     │  │   zagens-tui     │  │  zagens CLI      │
│  Tauri + WebView │  │  ratatui 终端    │  │  exec / serve    │
└────────┬─────────┘  └────────┬─────────┘  └────────┬─────────┘
         │ HTTP+SSE（loopback）│ 进程内              │ 进程内 / HTTP
         ▼                     ▼                     ▼
┌─────────────────────────────────────────────────────────────────┐
│  zagens-runtime sidecar  ·  Kernel V3 turn 引擎                 │
│  LiveTurnMachine → EffectInterpreter → V3TurnHost               │
│  /v1/threads · MCP · 技能 · 工具 · kernel_events 日志            │
└───────────────────────────────┬─────────────────────────────────┘
                                ▼
         zagens-core · runtime-orchestrator · runtime-adapters
```

完整边界：[`docs/tech/RUNTIME_ARCHITECTURE.md`](docs/tech/RUNTIME_ARCHITECTURE.md) · Kernel V3：[`docs/tech/AGENT_KERNEL_V3.md`](docs/tech/AGENT_KERNEL_V3.md) · HTTP 契约：[`docs/tech/API_DESIGN.md`](docs/tech/API_DESIGN.md)。

### 安全模式（`sandbox_mode`）

| 模式 | 说明 |
|------|------|
| `read-only` | 不允许 Shell 执行和文件写入 |
| `workspace-write` | Shell 与写入限于工作区（推荐默认） |
| `danger-full-access` | 完整文件系统访问 — 请谨慎 |
| `external-sandbox` | 将 `exec_shell` 路由到 OpenSandbox 兼容 API |

审批策略（`on-request` / `untrusted` / `never`）、按域名网络规则、OS keyring 存凭据。运行时 Token **不会**进入 WebView。

---

## 开发

**环境：** Rust 1.88+（MSRV；CI 固定 **1.96**）、Node.js 20 LTS、Python 3.8+、[Tauri CLI 2](https://v2.tauri.app/start/prerequisites/)。

见 **[CONTRIBUTING.md](CONTRIBUTING.md)** 与 **[LOCAL_DEV_VERIFY.md](LOCAL_DEV_VERIFY.md)**。

| 命令 | 说明 |
|------|-------------|
| `just check` | **PR 门禁：** verify + 全量测试 + web-check（[`justfile`](justfile) L2） |
| `just verify-all` | **推送门禁：** verify + 测试 + multi-session + lockfile（L3） |
| `just verify` | CI Lint 镜像 — toolchain + prebuild + fmt + clippy（L1） |
| `just web-check` | Web UI：tsc + ESLint + Vitest |
| `just --list` | 全部 recipe（层级说明见 justfile 头部） |
| `bash scripts/ci/verify-lint.sh` | 同 `just verify`（直接脚本） |
| `bash scripts/ci/verify-workspace.sh` | 同 `just verify-all` |
| `cargo test --workspace --all-features` | 仅 Rust 测试（`just test-all` 会额外 prebuild） |
| `cd crates/desktop && cargo tauri dev` | 开发模式启动桌面 |

Windows：`just verify` 或 `pwsh -File scripts/ci/verify-lint.ps1`。Cursor：**运行任务 → Zagens:***

```
zagens/
├── crates/desktop/        # Tauri 桌面应用
├── crates/runtime-server/ # zagens-runtime sidecar · zagens CLI · zagens-tui（feature `tui`）
├── crates/core/           # Kernel V3 引擎（LiveTurnMachine、kernel events）
├── docs/                  # 公开设计规格
├── fixtures/harness/      # LHT / kernel 重放夹具
└── config.example.toml
```

---

## 许可

[MIT](LICENSE) — Copyright (c) 2024-2026 Zagens Contributors。额外归属：[NOTICE.md](NOTICE.md) · [third-party/deepseek-tui/LICENSE](third-party/deepseek-tui/LICENSE)。
