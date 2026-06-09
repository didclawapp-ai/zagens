<p align="center">
  <img src="assets/screenshot.png" alt="Zagens 截图" width="800" />
</p>

# Zagens — 桌面 Agent 控制台

**[English](README.md)** · **[日本語](README.ja.md)** · **[Português (BR)](README.pt-BR.md)** | 中文

长程 Agent 任务容易**半途停下或过早「声称完成」**；写代码和改 Office 往往**各用一套工具**；本地执行还需要**回放、审批和可审计性**——而不只是多开一个聊天窗口。

**Zagens** 是跑在你机器上的**桌面 Agent 控制台**：Code 与 Office 工作区共用本地 **runtime sidecar**，支持按轮 **会话回放**、长程任务的**分层完成门禁**，以及托盘、通知、嵌入式终端等桌面原生能力。使用自备 API Key 连接 [DeepSeek](https://deepseek.com/) 或其他 OpenAI 兼容提供商。

> **作者语：** 不要相信 AI Agent 能做任何事情，它是有边界的；我们能做的，就是拓展这种边界。

> **许可：** [MIT](LICENSE)。Runtime 谱系：[NOTICE.md](NOTICE.md) · [third-party/deepseek-tui/](third-party/deepseek-tui/)。**与 DeepSeek 公司无关联。** 以下以 **Zagens v0.7.3** 为准 — 见 [CHANGELOG.md](CHANGELOG.md)。

| 资源 | 链接 |
|------|------|
| 用户文档 | [zagens.com/docs](https://zagens.com/docs) |
| 安装包 | [GitHub Releases](https://github.com/didclawapp-ai/zagens/releases)（最新 **`zagens-v0.7.3`**）· [zagens.com/download](https://zagens.com/download) |
| 设计规格 | [`docs/README.md`](docs/README.md) |
| 贡献指南 | [`CONTRIBUTING.md`](CONTRIBUTING.md) · [`LOCAL_DEV_VERIFY.md`](LOCAL_DEV_VERIFY.md) |
| 安全策略 | [`SECURITY.md`](SECURITY.md) |

---

## 适合谁 / 不太适合谁

| 更适合 | 不太适合 |
|--------|----------|
| 想要**独立桌面 Harness**（不绑死某一 IDE 插件）的开发者 | 托管 SaaS、包月模型、零配置云端 |
| 做**长程代码重构**或**Office 交付物**、希望同一工作流的人 | 纯聊天、无工具、无工作区、无回放 |
| 在意**本地 sidecar**、MCP/技能、UI 内**执行审批**的用户 | 完全自主、无护栏的 YOLO Agent |
| 当前 **Windows 桌面**用户；macOS/Linux 可用 **CLI** 或源码构建 | 零安装的移动端 / 纯浏览器体验 |

---

## 三条差异化

**1. Harness，不是聊天壳** — 长程代码任务用**可组合完成门禁**（操作者 / 模型 / 工具链分层），而不是「模型说做完了就算完」。规格：[LHT](docs/harness/LONG_HORIZON_CODE_TASKS.md) · 夹具：[`fixtures/harness/`](fixtures/harness/)。

**2. 桌面原生控制面** — [Tauri 2](https://tauri.app/) UI + 本机 loopback **sidecar**（`zagens-runtime`）：托盘、通知、diff、**会话回放**、Code 工作区 **PTY**、HTTP 工具审批。与无 GUI 的 **`zagens`** CLI 同一引擎。

**3. Code + Office，一套 runtime** — **Code / Office** 任务类型共享配置与工具面，但提示词与工具集不同；切换类型会**新开会话**以保持 KV 稳定（[架构说明](docs/task-type-prompt-architecture.md)）。Office：`read_file` / **`write_office`**（xlsx 用 Rust；docx/pptx/pdf 用捆绑 Python）。

另有：**CRAFT 多代理**（子代理、fix-loop 裁决、P1 黑板 — [说明](docs/craft-v2-improvements.md)）、懒加载**符号索引**（`.zagens/symbols.json`）、MCP、技能、Hooks、定时任务。

---

## 我们重点解决什么问题

| 痛点 | Zagens 的做法 |
|------|----------------|
| Agent 中途停手或过早标记完成 | **分层完成门禁** + 长程任务面板（[组合式 Harness](docs/harness/COMPOSABLE_HARNESS.md)） |
| IDE 插件与终端 Agent 会话割裂 | 统一 **sidecar** + SQLite 线程、分叉/恢复、**回放**、工作区快照 |
| 表格与文档在编码 Agent 流程之外 | **Office 模式** + `write_office` + 桌面预览 |
| 本地跑工具又不能盲信 | 执行策略、网络规则、路径规范化、审批 UI、运行时 Token 不进 WebView（[沙箱矩阵](docs/tech/SANDBOX_CAPABILITY_MATRIX.md)） |

---

## 当前已具备（v0.7.3）

**桌面：** 多会话聊天（流式/停止/思考流）、文件树与预览与 diff、PTY 终端（Code）、子代理面板、任务/技能 UI、MCP 与路由规则、用量图表、keyring 存 Key、中/英/日/葡 UI。

**Runtime：** 线程、MCP、技能（`~/.zagens/skills`）、生命周期 Hooks、多提供商路由、视觉（`describe_image`）。

**工具（代表）：** 文件（`read_file`、`write_file`、`edit_file` …）、git、`exec_shell`、`write_office`、可选 `web_search` / `fetch_url`、记忆工具。完整列表：`crates/runtime-server/src/tools/` · [CHANGELOG.md](CHANGELOG.md)。

---

## 已知限制（使用前请了解）

我们更愿意写清边界，而不是堆功能清单。

| 主题 | 现状 |
|------|------|
| **桌面安装包** | **Windows** 安装包见 [Releases](https://github.com/didclawapp-ai/zagens/releases)。**macOS / Linux 桌面包** — 规划中。三平台 **CLI** 已发。 |
| **OS 级沙箱** | **macOS Seatbelt** — 在 `sandbox-exec` 可用时强制执行。**Linux / Windows** — 策略已声明，**尚未 OS 级强制**（degraded 模式；非 macOS 桌面 UI 会提示）。详见 [`SANDBOX_CAPABILITY_MATRIX.md`](docs/tech/SANDBOX_CAPABILITY_MATRIX.md)。 |
| **模型与 Key** | 需自备 API Key；连接 DeepSeek 等 OpenAI 兼容端点 — **我们不托管模型**。 |
| **长程与多代理** | 门禁与 CRAFT **可用且在持续演进**，边界场景与新门禁类型在活跃开发中。 |
| **Office 深度** | 核心读写路径已有；企业连接器、语音及部分场景模板为**后续方向**（[Office 场景](docs/desktop/OFFICE_SCENARIOS.md)）。 |

安全问题请走 [`SECURITY.md`](SECURITY.md)。

---

## 我们在往哪走

公开设计规格见 [`docs/`](docs/README.md)。方向包括：

- **平台对齐** — macOS/Linux 桌面安装包；Linux/Windows 更强 OS 沙箱。
- **可信长任务** — 更严完成门禁、Harness 夹具、可回放的操作者工作流。
- **Office 工作流** — 更丰富的场景覆盖，且不脱离统一 runtime。
- **安全加固** — 见 [CHANGELOG](CHANGELOG.md) 与 [SECURITY.md](SECURITY.md)。

---

## 快速开始

**预编译（Windows）：** [GitHub Releases `zagens-v0.7.3`](https://github.com/didclawapp-ai/zagens/releases) — 安装 zip + CLI。SmartScreen：[SMARTSCREEN.md](docs/desktop/SMARTSCREEN.md)。

**从源码构建：**

```bash
git clone https://github.com/didclawapp-ai/zagens.git
cd zagens

cargo build -p zagens-cli          # sidecar 复制到 crates/desktop/binaries/

cd crates/desktop/web-ui && npm install
cd .. && cargo tauri dev

# API Key：Zagens 设置，或 ~/.zagens/config.toml
```

**无 GUI CLI**（与桌面同引擎）：

```bash
cargo install zagens-cli --version 0.7.3 --bin zagens --locked

zagens doctor
zagens exec '总结 src/ 变更' --json
zagens exec '重构 auth 模块' --auto
zagens serve --http --port 7878
```

预编译 CLI 与 SHA-256：[Releases](https://github.com/didclawapp-ai/zagens/releases)。配置样例：[config.example.toml](config.example.toml)。

---

## 架构

```
┌──────────────────────────────────────────────────────────────┐
│                     Zagens (Tauri 2)                         │
│  ┌─────────────────┐  ┌───────────────────────────────────┐  │
│  │   WebView UI    │  │         Rust 外壳                 │  │
│  │   React / TS    │◄─┤  commands、sidecar 监督器、托盘     │  │
│  └────────┬────────┘  └───────────────┬───────────────────┘  │
│           │ HTTP + SSE                │                       │
│           ▼                           ▼                       │
│  ┌─────────────────────────────────────────────────────────┐  │
│  │     Runtime API（嵌入式 sidecar，loopback HTTP/SSE）      │  │
│  │  /v1/threads、/v1/skills、/v1/symbol-index 等           │  │
│  └───────────────────────┬─────────────────────────────────┘  │
│                          ▼                                    │
│  ┌─────────────────────────────────────────────────────────┐  │
│  │  共享 crates：agent、core、config、state、tools、mcp     │  │
│  └─────────────────────────────────────────────────────────┘  │
└──────────────────────────────────────────────────────────────┘
```

完整边界：[`docs/tech/RUNTIME_ARCHITECTURE.md`](docs/tech/RUNTIME_ARCHITECTURE.md) · HTTP 契约：[`docs/tech/API_DESIGN.md`](docs/tech/API_DESIGN.md)。

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
| `bash scripts/ci/verify-lint.sh` | CI Lint 镜像 |
| `bash scripts/ci/verify-workspace.sh` | Lint + 全 workspace 测试 |
| `cargo test --workspace --all-features` | 运行全部测试 |
| `cd crates/desktop && cargo tauri dev` | 开发模式启动桌面 |

Windows：`pwsh -File scripts/ci/verify-lint.ps1`

```
zagens/
├── crates/desktop/       # Tauri 应用
├── crates/runtime-server/ # Sidecar HTTP/SSE
├── docs/                 # 公开设计规格
├── fixtures/harness/     # LHT / Office 夹具
└── config.example.toml
```

---

## 许可

[MIT](LICENSE) — Copyright (c) 2024-2026 Zagens Contributors。额外归属：[NOTICE.md](NOTICE.md) · [third-party/deepseek-tui/LICENSE](third-party/deepseek-tui/LICENSE)。
