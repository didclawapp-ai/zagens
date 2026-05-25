# 运行时架构（SSOT 附图）

> **叙述与排期：** 见 [RUNTIME_EVOLUTION_ROADMAP.md](./RUNTIME_EVOLUTION_ROADMAP.md)（**v2.0-final**；门控 **A → A+ → P2 → F**）  
> **HTTP 契约：** [API_DESIGN.md](./API_DESIGN.md)  
> **最后更新：** 2026-05-22（§17 审核：P2 类型部分入 `deepseek-core`，Engine 仍在 tui）

## 0. 三层模型（融合但不合并）

| 层 | 仓库落点 | 职责 |
|----|----------|------|
| **L1 底层** | 今：`crates/tui`；**P2 后：** `deepseek-core` + tui 适配 | Agent turn、会话 — **先 A+A+，再 P2** |
| **L2 契约** | `runtime_api` `/v1/*`、SSE 子集、`runtime_proxy`、Tauri IPC | 桌面/TUI **融合面** |
| **L3 壳** | `crates/desktop` WebView + Tauri；`tui/` ratatui | 只消费 L2，**不**内嵌 L1 |

---

## 1. 系统总览

```mermaid
flowchart TB
  subgraph users [用户入口]
    U1[终端用户]
    U2[Zagens 用户]
    U3[脚本 / CI]
  end

  subgraph cli_pkg ["crates/cli — deepseek"]
    CLI_DISPATCH[命令分发]
    CLI_MGMT[config / auth / thread / model / sandbox]
    CLI_APPSRV["deepseek app-server 实验"]
    CLI_DELEGATE["delegate_to_tui"]
  end

  subgraph desktop_pkg ["crates/desktop — Zagens"]
    TAURI[Tauri Shell]
    WEB[React web-ui]
    PROXY[runtime_proxy]
  end

  subgraph tui_bin ["crates/tui — deepseek-tui 生产运行时"]
    MAIN[main.rs]
    TUI_UI[ratatui]
    RT_API[runtime_api /v1]
    RT_THREADS[runtime_threads]
    ENGINE[core/engine]
  end

  subgraph exp [实验路径 — 非产品主链]
    APP_SRV[app-server]
    CORE_RT[core::Runtime queued]
  end

  U1 --> CLI_DISPATCH
  U2 --> WEB
  CLI_DISPATCH --> CLI_DELEGATE
  CLI_DISPATCH --> CLI_MGMT
  CLI_DISPATCH --> CLI_APPSRV
  CLI_DELEGATE --> MAIN
  CLI_APPSRV --> APP_SRV --> CORE_RT
  WEB --> PROXY --> RT_API
  TAURI --> MAIN
  MAIN --> TUI_UI
  MAIN --> RT_API
  RT_API --> RT_THREADS --> ENGINE
  TUI_UI --> ENGINE
```

**图例：** 绿色路径 = 生产；`app-server` = 仅 `deepseek app-server` 子命令。

---

## 2. 生产路径内部（deepseek-tui）

```mermaid
flowchart LR
  HTTP[HTTP /v1/* SSE]
  TUI_IN[TUI EngineHandle Op]
  ROUTER[runtime_api]
  MGR[RuntimeThreadManager]
  ENG[Engine + turn_loop]
  LLM[client/chat SSE]
  TOOLS[tools/* + mcp.rs]

  HTTP --> ROUTER --> MGR --> ENG
  TUI_IN --> ENG
  ENG --> LLM
  ENG --> TOOLS
  MGR --> BCAST[broadcast events]
  BCAST --> ROUTER
```

---

## 3. Workspace crate 依赖

```mermaid
flowchart BT
  TUI[deepseek-tui]
  DESK[deepseek-desktop]
  CLI[deepseek CLI]
  ASRV[app-server]
  CORE[deepseek-core]

  TUI --> CFG[config]
  TUI --> SEC[secrets]
  TUI --> TOOLS[tools]
  DESK --> CFG
  DESK --> SEC
  CLI --> CFG
  CLI --> SEC
  CLI --> ASRV
  ASRV --> CORE
  TOOLS --> PROTO[protocol]
```

`deepseek-tui` **不依赖** `deepseek-core`。

---

## 4. CLI 子命令路由

| 类型 | 示例命令 | 路径 |
|------|----------|------|
| 转调 tui | `run`, `exec`, `serve`, `resume`, … | `deepseek-tui` 生产链 |
| CLI 内建 | `config`, `thread list`, `model`, `sandbox` | workspace 库 |
| 实验 | `app-server` | `core::Runtime` |
