<p align="center">
  <img src="assets/screenshot.png" alt="Zagens 截图" width="800" />
</p>

# Zagens — 桌面 Agent 控制台

中文 | **[English](README.md)**

**Zagens** 是面向代码与办公工作区的**桌面 Agent 控制台**——基于 [Tauri 2](https://tauri.app/) 的独立产品，通过嵌入式 agent sidecar 连接 [DeepSeek](https://deepseek.com/) 及其他 OpenAI 兼容提供商：系统托盘、完成通知、工作区预览、会话回放、Code 工作区嵌入式终端。

> **许可：** [MIT](LICENSE)。Runtime 谱系归属说明见 [NOTICE.md](NOTICE.md) 与 [third-party/deepseek-tui/](third-party/deepseek-tui/)。

| 资源 | 链接 |
|------|------|
| 用户文档 | [zagens.com/docs](https://zagens.com/docs) |
| 安装包 | [zagens.com/download](https://zagens.com/download) · [GitHub Releases](https://github.com/didclawapp-ai/zagens/releases) |
| 设计规格 | [`docs/README.md`](docs/README.md) |
| 贡献指南 | [`CONTRIBUTING.md`](CONTRIBUTING.md) · [`LOCAL_DEV_VERIFY.md`](LOCAL_DEV_VERIFY.md) |
| 安全策略 | [`SECURITY.md`](SECURITY.md) |

> **与 DeepSeek 公司无关联。** 以下功能以 **Zagens v0.7.0** 为准。版本历史见 [CHANGELOG.md](CHANGELOG.md)。

---

## 项目特色

| 方向 | 说明 |
|------|------|
| **桌面 + Sidecar** | UI 通过本地 **runtime sidecar**（HTTP/SSE）通信；共用 `~/.zagens/config.toml`、会话与工具。 |
| **Code / Office 分场景** | 不同工具面与提示词；切换任务类型会**新开会话**以保持 KV 稳定（[架构说明](docs/task-type-prompt-architecture.md)）。 |
| **组合式 Harness** | 长程代码任务与分层完成门禁（操作者 / 模型 / 工具链）；夹具见 [`fixtures/harness/`](fixtures/harness/)（[LHT 规格](docs/harness/LONG_HORIZON_CODE_TASKS.md)）。 |
| **CRAFT 多代理** | 角色化子代理、结构化 fix-loop 裁决、P1 黑板交接（[CRAFT 说明](docs/craft-v2-improvements.md)）。 |
| **符号索引** | 懒加载 `.deepseek/symbols.json`，含桌面索引面板与重建 API。 |
| **Office 流水线** | `read_file` / `write_office`（xlsx 用 Rust，docx/pptx/pdf 用捆绑 Python）。 |
| **安全优先** | 分层执行策略、bash 词典、域名网络规则、桌面 HTTP 工具审批。 |
| **桌面体验** | 托盘、完成通知、会话回放、diff 面板、Code 工作区 **PTY 终端**、Sidecar 健康探测与重启。 |

---

## 桌面端（v0.7.0）

| 区域 | 已具备 |
|------|--------|
| **聊天** | 多会话侧栏、流式/停止/思考流、上下文条、会话导出 |
| **工作区** | 文件树、预览、diff2html、快照恢复、**xterm.js PTY**（Code 模式） |
| **回放** | 按轮次回放工具调用 |
| **代理** | 子代理面板（SSE）；清单侧栏 |
| **任务与技能** | 后台任务 + 技能创建/导入 UI |
| **设置** | MCP、路由规则、用量图表、keyring 存 API Key、中/英/日/葡 UI |

完整发版记录见 [CHANGELOG.md](CHANGELOG.md)。

---

## Agent Runtime（Sidecar）

嵌入式 **`deepseek-runtime`** 进程提供聊天、工具与设置能力：

- **会话与线程** — SQLite 持久化、恢复/分叉、工作区快照
- **MCP** — stdio 服务、按工具过滤
- **技能** — `~/.zagens/skills`、桌面导入/安装 API
- **Hooks** — 生命周期 shell 命令（stdout / JSONL / webhook）
- **多提供商** — 配置、profile、`routing_rules.json`
- **视觉** — 可配置端点的 `describe_image`

HTTP 契约：[`docs/tech/API_DESIGN.md`](docs/tech/API_DESIGN.md)。配置样例：[config.example.toml](config.example.toml)。

### 代表性工具

| 类别 | 工具 |
|------|------|
| **文件** | `read_file`、`write_file`、`edit_file`、`apply_patch`、`glob_files`、`grep_files` |
| **仓库** | `git_status`、`git_diff`、`git_log` 等 |
| **Shell** | `exec_shell`、后台任务、可选外部沙箱 |
| **Office** | `write_office`、`read_file` 读 Office/PDF |
| **网络** | `web_search`、`fetch_url`（启用时） |
| **记忆** | `remember`、`note`、可选用户记忆 |

实现路径：`crates/runtime-server/src/tools/`。

---

## 安全与沙箱

| 模式 | 说明 |
|------|------|
| `read-only` | 不允许 Shell 执行和文件写入 |
| `workspace-write` | Shell 和写入仅限工作区目录 |
| `danger-full-access` | 完整文件系统访问（请谨慎） |
| `external-sandbox` | 将 `exec_shell` 路由到 OpenSandbox 兼容 API |

审批策略（`on-request` / `untrusted` / `never`）、按域名网络规则、路径规范化、运行时 Token 隔离（不进 WebView）、OS keyring 存凭据。详见 [`docs/tech/SANDBOX_CAPABILITY_MATRIX.md`](docs/tech/SANDBOX_CAPABILITY_MATRIX.md)。

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

完整边界说明：[`docs/tech/RUNTIME_ARCHITECTURE.md`](docs/tech/RUNTIME_ARCHITECTURE.md)。

---

## 环境要求

- **Rust** 1.88+（MSRV）；开发/CI 固定 **1.96** — [`rust-toolchain.toml`](rust-toolchain.toml)
- **Node.js** 建议 20 LTS（仅 web-ui 时 18+ 可能可用）
- **Python** 3.8+（Office 文档、代码执行、RLM）
- **[Tauri CLI 2](https://v2.tauri.app/start/prerequisites/)** — `cargo install tauri-cli --version "^2"`

**版本：** Zagens 桌面 **v0.7.0**；嵌入式 runtime **0.8.15**（根 `Cargo.toml`）。

---

## 快速开始

**预编译安装包：** 目前仅 Windows — [zagens.com/download](https://zagens.com/download) 或 [GitHub Releases](https://github.com/didclawapp-ai/zagens/releases)。macOS / Linux 安装包规划中。

**从源码构建：**

```bash
git clone https://github.com/didclawapp-ai/zagens.git
cd zagens

# Sidecar（build.rs 会复制到 crates/desktop/binaries/）
cargo build -p deepseek-runtime-server

cd crates/desktop/web-ui && npm install
cd .. && cargo tauri dev

# API Key：Zagens 设置，或 ~/.zagens/config.toml
```

**发布安装包（Windows）：** 在 `crates/desktop` 执行 `npm run bundle:prepare`，再 `cargo tauri build`。CI 发布：推送标签 `zagens-vX.Y.Z`。

---

## 开发指南

本地 Lint 与 pre-push hook 见 **[CONTRIBUTING.md](CONTRIBUTING.md)** 与 **[LOCAL_DEV_VERIFY.md](LOCAL_DEV_VERIFY.md)**。

| 命令 | 说明 |
|------|------|
| `bash scripts/ci/verify-lint.sh` | CI Lint 镜像（fmt + 严格 clippy） |
| `bash scripts/ci/verify-workspace.sh` | Lint + 全 workspace 测试 |
| `cargo test --workspace --all-features` | 运行全部测试 |
| `cd crates/desktop/web-ui && npm run build` | 构建 Web UI |
| `cd crates/desktop && cargo tauri dev` | 开发模式启动 Zagens |

Windows（PowerShell）：`pwsh -File scripts/ci/verify-lint.ps1`

---

## 项目结构

```
zagens/
├── crates/
│   ├── desktop/          # Tauri 应用（web-ui + Rust 外壳）
│   ├── runtime-server/   # Sidecar HTTP/SSE runtime
│   ├── core/             # 引擎与 turn loop
│   ├── tools/            # 工具注册与执行
│   └── …                 # agent、config、state、mcp、hooks 等
├── docs/                 # 公开设计规格（英文）
├── fixtures/harness/     # LHT / Office harness 夹具（可执行资产）
├── third-party/          # 第三方许可证
├── config.example.toml   # 配置参考
└── Cargo.toml            # Workspace 清单
```

---

## 配置

在 **Zagens → 设置** 或 `~/.zagens/config.toml` 中配置 API Key（参见 [config.example.toml](config.example.toml)）。

主要配置段：`provider` / `[providers.*]`、`default_text_model`、`reasoning_effort`、`sandbox_mode`、`approval_policy`、`[network]`、`[hooks]`、`[context]`、`[features]`。

运行时 Token 在启动时生成，经 Tauri Sidecar 环境变量注入 — **不会**进入 WebView。

---

## 许可

[MIT](LICENSE) — Copyright (c) 2024-2026 Zagens Contributors。

嵌入式 runtime 谱系额外归属说明：[NOTICE.md](NOTICE.md) 与 [third-party/deepseek-tui/LICENSE](third-party/deepseek-tui/LICENSE)。
