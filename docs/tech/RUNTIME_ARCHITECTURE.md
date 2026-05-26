# 运行时架构（SSOT 附图）

> **叙述与排期：** [RUNTIME_EVOLUTION_ROADMAP.md](./RUNTIME_EVOLUTION_ROADMAP.md)（**v2.0-final**；门控 **A → A+ → P2 → F** 已闭合；**§3 现状快照**）  
> **HTTP / IPC 契约：** [API_DESIGN.md](./API_DESIGN.md)  
> **架构评估 / 定型判定：** [adr/ARCHITECTURE_ASSESSMENT_2026-05-25.md](./adr/ARCHITECTURE_ASSESSMENT_2026-05-25.md)（**§1 = 10/10 架构定型**，2026-05-26 — M-series + D6–D8 + D1 闭合）  
> **OpenAPI / TS 类型：** [openapi/zagens-runtime-v1.openapi.json](./openapi/zagens-runtime-v1.openapi.json) · [adr/D8_OPENAPI_TS_GENERATION.md](./adr/D8_OPENAPI_TS_GENERATION.md)  
> **实施后快照：** [adr/IMPLEMENTATION_SUMMARY_2026-05-24.md](./adr/IMPLEMENTATION_SUMMARY_2026-05-24.md)  
> **D6 sidecar：** [adr/D6_RUNTIME_SERVER.md](./adr/D6_RUNTIME_SERVER.md) · [D6_IMPLEMENTATION_PLAN.md](./adr/D6_IMPLEMENTATION_PLAN.md)（Phase A/A+ ✅）· Phase B spike：[D6_PHASE_B_SPIKE.md](./adr/D6_PHASE_B_SPIKE.md)  
> **最后更新：** 2026-05-26（D6 Phase A+ binary 契约测、M-series Engine 入核、`deepseek-runtime` sidecar — 与代码复核对齐）

> **如何读这份文档：** §1 是**顶层系统总览**（一张图把外部世界 → 桌面壳 → sidecar → LLM 一次看完）；§2 是**Sidecar 内部数据流**（HTTP → Manager → Engine → turn_loop）；§5 是**L2 双通道**（Tauri IPC vs Runtime HTTP/SSE）；§8 是**典型发消息时序图**。其余小节为旁注（crate 依赖、持久化、监督、CLI、模块索引）。所有图节点上的源文件路径均可直接对照代码核验。

---

## 0. 三层模型（融合但不合并）

| 层 | 仓库落点 | 职责 |
|----|----------|------|
| **L1 底层** | `deepseek-core`（`Engine` struct、`op_loop`、`turn_loop`、Session、`TurnEnginePort`）+ `deepseek-tui` lib（工具/MCP/LSP 宿主、`platform_dispatch` shim；`runtime_api` / `runtime_threads` 源码仍在此 crate） | Agent turn 逻辑在 core；tui 提供平台侧接线与 HTTP 服务实现 |
| **L1 生产 binary** | **`deepseek-runtime`**（`crates/runtime-server`）— 链接 `deepseek-tui` **无** `tui-ui` / ratatui | Zagens 嵌入 sidecar；见 [D6_RUNTIME_SERVER.md](./adr/D6_RUNTIME_SERVER.md) |
| **L2 契约** | `runtime_api/`、`runtime_proxy`、Tauri IPC（`commands.rs`）、OpenAPI | 桌面/TUI **融合面**；Bearer 不出 WebView |
| **L3 壳** | **Zagens** `crates/desktop`（唯一用户产品）；`crates/tui/src/tui/` ratatui（**maintenance/freeze**）；CLI `deepseek`（dev/headless） | 只消费 L2，**不**内嵌 L1 |

**硬约束：** Agent turn 只在 sidecar 内执行；**生产 binary 为 `deepseek-runtime`**（Zagens `externalBin`）；dev/CLI 可 fallback `deepseek-tui serve --http`。~~`app-server`~~ 已于 D7 C5 **删除**（见 [D4_APPSERVER_DEPRECATED.md](./adr/D4_APPSERVER_DEPRECATED.md)）。

---

## 1. 系统总览

```mermaid
flowchart TB
    subgraph users["用户入口"]
        U1["Zagens 桌面用户"]
        U2["终端 / 开发者"]
        U3["脚本 / CI / Headless"]
    end

    subgraph desktop_pkg["crates/desktop  (Zagens v0.4.3, Tauri 2)"]
        WEB["web-ui (React + Vite)<br/>App.tsx · Composer · RightPanel<br/>api/client.ts"]
        TAURI["Tauri Shell<br/>main.rs · WindowRegistry · 托盘"]
        CMDS["commands.rs<br/>get_runtime_port · API Key · Vision<br/>系统设置 · 符号索引 · 终端 PTY"]
        PROXY["runtime_proxy.rs<br/>runtime_http · runtime_post_stream<br/>runtime_get_sse (Bearer 注入)"]
        SUP["sidecar.rs (Supervisor)<br/>/health 探测 · 崩溃退避<br/>~/.deepseek/logs/sidecar.log"]
        TERM["terminal.rs<br/>portable-pty 子终端"]
        BIN[("binaries/<br/>deepseek-runtime-&lt;triple&gt;.exe<br/>(build.rs 复制嵌入)")]
    end

    subgraph sidecar_proc["子进程: deepseek-runtime  (crates/runtime-server)"]
        MAIN["runtime-server/main.rs<br/>→ runtime_serve::run_from_args"]
        HTTP["runtime_api::run_http_server<br/>axum @ 127.0.0.1:&lt;dynamic&gt;"]
        ROUTER["runtime_api/router.rs<br/>/v1/* · /health · /internal/probe"]
        AUTH["auth.rs<br/>require_runtime_token (Bearer)"]
        STREAM["stream.rs (SSE)"]
        MGR["runtime_threads/manager.rs<br/>RuntimeThreadManager"]
        ENG_C["core/engine/runtime.rs<br/>Engine struct + op_loop<br/>EngineHandle · Op 通道"]
        ENG_SHIM["tui/core/engine.rs<br/>~130 LOC shim<br/>platform_dispatch · build_engine"]
        TURN["core/engine/turn_loop<br/>handle_deepseek_turn · TurnEnginePort"]
        LLM_C["client.rs · llm_client/<br/>HTTP/SSE → LLM"]
        MCP_POOL["mcp.rs · McpPool"]
        TOOLS_R["tools/* · shell · subagent<br/>todo · plan · lsp"]
        TASKS["task_manager.rs<br/>automation_manager.rs"]
        PERSIST["runtime_threads/persist.rs<br/>session_manager.rs<br/>thread_store_sqlite.rs"]
    end

    subgraph external["外部服务"]
        LLM[("DeepSeek API<br/>SiliconFlow / OpenAI 兼容")]
        MCP_SRV[("MCP Servers<br/>stdio / SSE")]
    end

    subgraph fs["~/.deepseek/"]
        SESS[("sessions/")]
        RT_DIR[("tasks/runtime/<br/>threads · turns · items · events + SQLite")]
        LOGS[("logs/<br/>sidecar.log · supervisor.log")]
        KEYS[("OS Keyring<br/>(deepseek-secrets)")]
    end

    subgraph cli_pkg["crates/cli — deepseek (开发/CI 入口)"]
        CLI["lib.rs · 命令分发<br/>delegate_to_tui · config · auth<br/>thread · model · sandbox · app-server"]
    end

    U1 --> WEB
    WEB -- "invoke" --> CMDS
    WEB -- "invoke runtime_*" --> PROXY
    CMDS -. "管理 token/port" .- PROXY
    TAURI --> SUP
    SUP -- "spawn + 监控" --> MAIN
    BIN -. "嵌入打包" .- SUP
    PROXY -- "Bearer Token + HTTP/SSE" --> HTTP

    U2 --> CLI
    U3 --> CLI
    CLI -- "delegate (子进程)" --> MAIN
    CLI -. "dev: deepseek-tui serve --http" .- MAIN

    MAIN --> HTTP
    HTTP --> ROUTER
    ROUTER --> AUTH
    ROUTER --> STREAM
    ROUTER --> MGR
    MGR --> ENG_C
    ENG_C --> ENG_SHIM
    ENG_SHIM --> TURN
    TURN --> LLM_C
    ENG_SHIM --> MCP_POOL
    ENG_SHIM --> TOOLS_R
    MGR --> TASKS
    MGR --> PERSIST

    LLM_C --> LLM
    MCP_POOL --> MCP_SRV
    PERSIST --> SESS
    PERSIST --> RT_DIR
    SUP --> LOGS
    CMDS --> KEYS

    classDef product fill:#1e3a5f,stroke:#60a5fa,color:#fff
    classDef sidecar fill:#3f2f1a,stroke:#fbbf24,color:#fff
    classDef ext fill:#3a1e3a,stroke:#f472b6,color:#fff
    classDef store fill:#1e3a2f,stroke:#34d399,color:#fff
    class WEB,TAURI,CMDS,PROXY,SUP,TERM,BIN product
    class MAIN,HTTP,ROUTER,AUTH,STREAM,MGR,ENG_C,ENG_SHIM,TURN,LLM_C,MCP_POOL,TOOLS_R,TASKS,PERSIST sidecar
    class LLM,MCP_SRV ext
    class SESS,RT_DIR,LOGS,KEYS store
```

**图例：** 蓝 = Zagens 产品壳；橙 = 嵌入式 sidecar（独立进程）；紫 = 外部服务；绿 = 本地持久化。实线 = 数据/控制流；虚线 = 弱关联（token 管理、二进制嵌入、握手）。

**关键代码出处（按节点）：**

| 节点 | 文件 | 说明 |
|------|------|------|
| 端口动态化 + Bearer Token UUID | [`crates/desktop/src/main.rs`](../../crates/desktop/src/main.rs) | `tokio::sync::watch::channel::<u16>` 初始 0；supervisor 解析 `DS_PICK_READY.port` 后发布；初始建议端口 7878；`uuid::Uuid::new_v4()` 每次启动新 token，仅注入 sidecar 子进程环境 |
| Sidecar 二进制嵌入 | [`crates/desktop/build.rs`](../../crates/desktop/build.rs) | 构建期复制为 `binaries/deepseek-runtime-<triple>(.exe)`，作为 Tauri `externalBin` |
| Sidecar 入口 | [`crates/runtime-server/src/main.rs`](../../crates/runtime-server/src/main.rs) → [`runtime_serve.rs`](../../crates/tui/src/runtime_serve.rs) | `deepseek-runtime --host … --port …`（无 `serve` 子命令）；dev fallback：`deepseek-tui serve --http` |
| Tauri IPC handlers | [`crates/desktop/src/commands.rs`](../../crates/desktop/src/commands.rs) | 密钥/设置/平台/符号索引/导出 等 30+ 命令 |
| HTTP 代理（Bearer 注入） | [`crates/desktop/src/runtime_proxy.rs`](../../crates/desktop/src/runtime_proxy.rs) | `runtime_http` / `runtime_post_stream` / `runtime_get_sse` + path 白名单 |
| Sidecar 监督 | [`crates/desktop/src/sidecar.rs`](../../crates/desktop/src/sidecar.rs) | spawn/重启/退避/`DS_PICK_READY` 行协议；优先 `deepseek-runtime`，legacy `deepseek-tui` argv 兼容 |
| HTTP 入口（库） | [`crates/tui/src/runtime_api/mod.rs`](../../crates/tui/src/runtime_api/mod.rs) | `run_http_server` — 由 `runtime-server` 与 `deepseek-tui serve --http` 共用 |
| 路由表 | [`crates/tui/src/runtime_api/router.rs`](../../crates/tui/src/runtime_api/router.rs) | 全部 `/v1/*` 路由集中注册；`/health` `/internal/probe` 不走鉴权 |
| 线程管理 | [`crates/tui/src/runtime_threads/manager.rs`](../../crates/tui/src/runtime_threads/manager.rs) | `RuntimeThreadManager` + LRU 活跃线程 + 事件 broadcast |
| Engine struct + op loop | [`crates/core/src/engine/runtime.rs`](../../crates/core/src/engine/runtime.rs) + [`op_loop.rs`](../../crates/core/src/engine/op_loop.rs) | M7/M8：`Engine::with_hosts` / `Engine::run()` 在 core |
| tui Engine shim | [`crates/tui/src/core/engine.rs`](../../crates/tui/src/core/engine.rs) | ~130 LOC newtype + `build_engine` + `platform_dispatch` |
| turn_loop | [`crates/core/src/engine/turn_loop/mod.rs`](../../crates/core/src/engine/turn_loop/mod.rs) | `handle_deepseek_turn` / `TurnEnginePort` / `Session` |
| Web 客户端 | [`crates/desktop/web-ui/src/api/client.ts`](../../crates/desktop/web-ui/src/api/client.ts) | `useTauriRuntimeProxy`；D10 `filterThreadStreamEvents`；D9 `turnControl.ts` |

**版本线：** runtime/CLI workspace **0.8.15**（Rust 1.88+）；Zagens desktop **0.4.3**（独立 SemVer）。  
**CLI 不内嵌 L1：** `deepseek` 通过 `delegate_to_tui` 子进程转调 `deepseek-tui`（TUI/headless 命令）；HTTP sidecar 生产路径为 **`deepseek-runtime`**（[`crates/cli/src/lib.rs`](../../crates/cli/src/lib.rs) · [`crates/runtime-server`](../../crates/runtime-server/)）。  
**~~实验路径~~ 已删除（D7 C5）：** ~~`deepseek app-server`~~ / ~~`crates/app-server`~~ — 见 [`adr/D4_APPSERVER_DEPRECATED.md`](./adr/D4_APPSERVER_DEPRECATED.md)。

---

## 2. Sidecar 内部数据流（deepseek-runtime / deepseek-tui lib）

```mermaid
flowchart LR
    HTTP["HTTP /v1/* + SSE<br/>(axum, tower-http CORS)"]
    ROUTER["runtime_api/router.rs"]
    AUTH["auth::require_runtime_token<br/>middleware"]
    STREAM_H["stream.rs<br/>stream_turn · stream_thread_events"]
    THREADS["threads.rs / sessions.rs / tasks.rs<br/>automations.rs / mcp.rs / skills.rs<br/>blackboards.rs / topic_memory.rs / usage.rs"]

    MGR["RuntimeThreadManager<br/>(runtime_threads/manager.rs)"]
    ACTIVE["active.rs<br/>ActiveThreads (LRU)"]
    LIFE["turn_lifecycle.rs<br/>turn_control.rs · turn_wait.rs"]
    MONITOR["monitor.rs (事件落盘)"]
    PERSIST_M["persist.rs<br/>RuntimeThreadStore + SQLite"]
    BCAST(("broadcast::Sender<br/>RuntimeEventRecord"))

    PORT["deepseek_core::engine<br/>TurnEnginePort + StartTurnParams"]
    ENG["core/engine/runtime.rs<br/>Engine + op_loop · EngineHandle"]
    ENG_SHIM["tui/core/engine.rs<br/>platform_dispatch · build_engine"]
    OP_EXT["EnginePlatformExt<br/>(tui platform_dispatch)"]
    TURN["core/engine/turn_loop<br/>handle_deepseek_turn"]
    LLM_C["client.rs (DeepSeekClient)<br/>llm_client/ SSE"]
    TOOL_REG["tools/* + tool_registry<br/>shell · plan · todo · subagent<br/>lsp · sandbox · skills"]
    MCP_POOL["mcp.rs::McpPool"]

    HTTP --> ROUTER
    ROUTER --> AUTH
    ROUTER --> STREAM_H
    ROUTER --> THREADS

    STREAM_H --> MGR
    THREADS --> MGR

    MGR --> ACTIVE
    MGR --> LIFE
    MGR --> MONITOR
    MGR --> PERSIST_M
    MGR --> BCAST

    LIFE --> PORT
    PORT -- "validate + delegate" --> ENG
    ENG --> ENG_SHIM
    ENG_SHIM --> OP_EXT
    OP_EXT --> TURN
    TURN --> LLM_C
    TURN --> TOOL_REG
    TOOL_REG --> MCP_POOL

    MONITOR --> BCAST
    BCAST -- "SSE tail" --> STREAM_H
    PERSIST_M -. "thread_store_sqlite.rs<br/>~/.deepseek/tasks/runtime/" .- MGR
```

**路径要点：** HTTP 请求 → `RuntimeThreadManager::start_turn` 经 `TurnEnginePort`（core）校验 → 向 `EngineHandle` 发 `Op::SendMessage`（同进程 mpsc）→ `Engine::run()`（core `op_loop`）经 `EnginePlatformExt` 分发平台 op → tui `platform_dispatch` 接线 → `handle_deepseek_turn`（core）→ 事件经 `broadcast` 既回流 SSE 又由 `monitor.rs` 持久化。**ratatui** 路径同样持 `EngineHandle`（`spawn_engine`），不走 HTTP；现处 freeze（用户产品已切到 Zagens）。

**定型后拆分现状（2026-05-26）：**

| 组件 | 落点 | 说明 |
|------|------|------|
| `Engine` struct、`Engine::run()` op loop、`EngineHandle`、`Op` 通道 | [`crates/core/src/engine/`](../../crates/core/src/engine/) | M-series M7/M8 ✅ |
| `handle_deepseek_turn`、Session 类型、`TurnEnginePort` | [`crates/core/src/engine/turn_loop/`](../../crates/core/src/engine/turn_loop/) | core 库层 |
| tui newtype shim + `build_engine` + `platform_dispatch` | [`crates/tui/src/core/engine.rs`](../../crates/tui/src/core/engine.rs) + 子模块 | ~130 LOC 入口 + engine-flow 编排（`message_handlers`、`capacity_flow` 等） |
| 生产 sidecar binary | [`crates/runtime-server/`](../../crates/runtime-server/) | `deepseek-runtime` — **不**链 ratatui（D6 ✅） |
| HTTP 线程/回合生命周期 | [`crates/tui/src/runtime_threads/`](../../crates/tui/src/runtime_threads/) | 源码仍在 tui lib；D6 phase B 可选迁入 runtime-server |
| HTTP 路由 | [`crates/tui/src/runtime_api/`](../../crates/tui/src/runtime_api/) | `router.rs` 注册全部 `/v1/*`；OpenAPI 导出见 D8 |

---

## 3. Workspace crate 依赖

```mermaid
flowchart BT
    DESK["deepseek-desktop<br/>(crates/desktop)"]
    RT_SRV["deepseek-runtime-server<br/>(crates/runtime-server)"]
    TUI["deepseek-tui<br/>(crates/tui, bin + lib)"]
    CLI["deepseek-tui-cli<br/>bin: deepseek<br/>(crates/cli)"]
    CORE["deepseek-core<br/>(crates/core)"]
    TM["deepseek-topic-memory"]
    PROTO["deepseek-protocol"]
    CFG["deepseek-config"]
    SEC["deepseek-secrets"]
    TOOLS["deepseek-tools"]
    AGENT["deepseek-agent"]
    EXEC["deepseek-execpolicy"]
    HOOKS["deepseek-hooks"]
    MCP["deepseek-mcp"]
    STATE["deepseek-state (CLI legacy)"]

    DESK --> CFG
    DESK --> SEC

    RT_SRV --> TUI
    TUI --> CORE
    TUI --> CFG
    TUI --> SEC
    TUI --> PROTO
    TUI --> TOOLS
    TUI --> TM

    CORE --> AGENT
    CORE --> CFG
    CORE --> EXEC
    CORE --> HOOKS
    CORE --> MCP
    CORE --> PROTO
    CORE --> STATE
    CORE --> TOOLS

    CLI --> AGENT
    CLI --> CFG
    CLI --> EXEC
    CLI --> MCP
    CLI --> SEC
    CLI --> STATE

    classDef product fill:#1e3a5f,stroke:#60a5fa,color:#fff
    class DESK,RT_SRV,TUI,CLI product
```

| Crate | 路径 | 角色 |
|-------|------|------|
| **deepseek-desktop** | `crates/desktop/` | Zagens Tauri 壳；**只**依赖 `config` + `secrets` + Tauri/reqwest/portable-pty |
| **deepseek-runtime-server** | `crates/runtime-server/` | 生产 sidecar binary **`deepseek-runtime`**；依赖 `deepseek-tui` lib（`default-features = false`，**无** ratatui） |
| **deepseek-tui** | `crates/tui/` | HTTP/SSE 实现（`runtime_api` / `runtime_threads`）+ ratatui TUI（freeze，`tui-ui` feature）；依赖 `core` |
| **deepseek-tui-cli** | `crates/cli/` | `deepseek` 二进制；dev/headless 入口；`delegate_to_tui` 子进程转调 |
| **deepseek-core** | `crates/core/` | `Engine` + `op_loop` + `turn_loop` / Session / `TurnEnginePort` / 工具目录 |
| ~~**deepseek-app-server**~~ | — | **已删除**（D7 C5）；见 [D4_APPSERVER_DEPRECATED.md](./adr/D4_APPSERVER_DEPRECATED.md) |
| **deepseek-state** | `crates/state/` | CLI legacy 线程元数据 SQLite（**非** sidecar SSOT；`thread list --source state`） |
| **deepseek-topic-memory** | `crates/topic-memory/` | B2 话题记忆 + `/v1/topic-memory` |

**关键事实（与各 `Cargo.toml` 一一核对）：**

- `deepseek-desktop` **只**依赖 `deepseek-config` + `deepseek-secrets`（加上 Tauri/reqwest/portable-pty/sha2/dirs），**不**直接依赖 `core` 或 `tui` —— 所有 Agent 能力通过嵌入式 **`deepseek-runtime`** 子进程 + HTTP/IPC 获得。
- `deepseek-runtime-server` 链接 `deepseek-tui` **无** `tui-ui` —— sidecar 二进制不含 ratatui/crossterm（D6 ✅；`cargo tree -p deepseek-runtime-server -i ratatui` → 无匹配）。
- `deepseek-tui` 依赖 `deepseek-core`；`Engine` struct + `Engine::run()` 在 core，tui 保留 ~130 LOC shim + 工具/MCP/LSP 宿主实现。
- ~~`deepseek-tui-core` legacy crate~~ **已于 2026-05-25 删除**。
- ~~`deepseek-app-server`~~ **已于 2026-05-26 删除**（D7 C5）。

---

## 4. 持久化（D7 ✅）

生产 sidecar 内 **Sessions** 与 **Runtime threads** 双 SQLite，由 `runtime_thread_id` 链接；完整路径、环境变量与 HTTP 读写表见 **[PERSISTENCE.md](./PERSISTENCE.md)**（D7 SSOT，2026-05-26 落地）。

| 轨 | 语义 | 代码 | 默认目录 |
|----|------|------|----------|
| **Sessions** | 桌面侧栏会话、消息快照、`runtime_thread_id` | [`session_manager.rs`](../../crates/tui/src/session_manager.rs) + [`session_store_sqlite.rs`](../../crates/tui/src/session_store_sqlite.rs) | `~/.deepseek/sessions/` |
| **Runtime threads** | HTTP `/v1/threads/*`、回合、事件、SSE | [`runtime_threads/persist.rs`](../../crates/tui/src/runtime_threads/persist.rs) + [`thread_store_sqlite.rs`](../../crates/tui/src/thread_store_sqlite.rs) | `~/.deepseek/tasks/runtime/`（`DEEPSEEK_RUNTIME_DIR` / `DEEPSEEK_TASKS_DIR`） |
| **CLI legacy** | `deepseek thread list --source state` 等 | [`deepseek-state`](../../crates/state/) | `~/.deepseek/state.db`（**非** sidecar SSOT；生产 list 默认 `runtime.db`） |

**Runtime 数据布局**（schema v2）：`threads/`、`turns/`、`items/`、`events/` + `runtime.db` SQLite；路由规则 `routing_rules.json`。

**TUI 异步落盘：** [`tui/persistence_actor.rs`](../../crates/tui/src/tui/persistence_actor.rs)；阻塞 I/O 策略见 [`A1_PERSIST_BLOCKING_AUDIT.md`](./adr/A1_PERSIST_BLOCKING_AUDIT.md)。

---

## 5. L2 契约：双通道

Zagens WebView 通过 **两条通道** 访问运行时（详见 [API_DESIGN.md](./API_DESIGN.md)）：

```mermaid
flowchart LR
    subgraph webview["WebView (Vite/React)"]
        UI["Composer · ChatView · SettingsPanel<br/>McpPanel · RightPanel · 等"]
        CLIENT["api/client.ts<br/>(useTauriRuntimeProxy 切换)"]
    end

    subgraph rust["Tauri Rust 进程 (crates/desktop)"]
        IPC["invoke handlers (commands.rs)"]
        RP["runtime_proxy.rs"]
        SC["sidecar.rs (Supervisor)"]
        KR["deepseek-secrets<br/>OS Keyring"]
    end

    subgraph child["子进程 deepseek-runtime (legacy: deepseek-tui serve --http)"]
        HEALTH["GET /health"]
        V1["/v1/* (Bearer 必需)"]
        SSE["SSE 端点<br/>POST /v1/stream<br/>GET /v1/threads/{id}/events"]
    end

    UI --> CLIENT

    CLIENT -- "通道 A: Tauri invoke" --> IPC
    IPC -- "API Key / 设置 / 平台<br/>符号索引 / 终端 PTY<br/>窗口 / 文件二进制读取" --> IPC
    IPC --> KR
    IPC -- "save_*_settings → notify_one()" --> SC

    CLIENT -- "通道 B: runtime_http<br/>runtime_post_stream<br/>runtime_get_sse" --> RP
    RP -- "Authorization: Bearer {token}<br/>+ path 白名单 (/health · /v1/*)" --> V1
    RP --> SSE
    SC -- "DS_PICK_READY 行协议<br/>stdin ping/drain" --> HEALTH

    classDef a fill:#1e3a5f,stroke:#60a5fa,color:#fff
    classDef b fill:#3f2f1a,stroke:#fbbf24,color:#fff
    class IPC,KR a
    class RP,SC b
```

| 通道 | 机制 | 典型用途 | 代码出处 |
|------|------|----------|----------|
| **A — Tauri IPC** | `invoke()` → `#[tauri::command]` | 端口/token、密钥、设置、符号索引、平台/系统/终端 PTY、文件二进制读取、sidecar 重启、多窗口 | [`crates/desktop/src/commands.rs`](../../crates/desktop/src/commands.rs)、[`window_registry.rs`](../../crates/desktop/src/window_registry.rs)、[`terminal.rs`](../../crates/desktop/src/terminal.rs) |
| **B — Runtime HTTP/SSE** | `runtime_proxy.rs` 转发 + Bearer 注入 | 对话、线程、SSE、MCP、任务、用量、自动化、技能 | [`crates/desktop/src/runtime_proxy.rs`](../../crates/desktop/src/runtime_proxy.rs)；客户端 [`web-ui/src/api/client.ts`](../../crates/desktop/web-ui/src/api/client.ts) · [`turnControl.ts`](../../crates/desktop/web-ui/src/api/turnControl.ts) |

**安全约束（H06）+ 桌面 UX（D9/D10）：**

- `runtime_http` / `runtime_get_sse` / `runtime_post_stream` 都在 Rust 侧注入 `Authorization: Bearer {token}`，**token 不进** WebView storage / DevTools。
- 路径白名单（[`runtime_proxy::validate_runtime_path`](../../crates/desktop/src/runtime_proxy.rs)）：仅允许 `/health` 与 `/v1/*`，拒绝 `..` 与非 `/v1` 前缀。
- **取消/打断两层（D9）：** 上层 `runtime_cancel_sse`（`SSE_CANCEL_FLAGS` 按 `window.label()` 维度；断开 WebView ↔ sidecar SSE）；下层 `POST /v1/threads/{id}/turns/{turn_id}/interrupt`（真正取消 turn，发 `Op::Interrupt`）。UI 统一经 `turnControl.ts`。
- **跨窗口 SSE 过滤（D10）：** `filterThreadStreamEvents` 按 thread owner 过滤，避免多窗口「幽灵渲染」。
- 配置改动 ⇒ 重启 sidecar：`AppContext::sidecar_restart: Arc<Notify>`，写入密钥/设置后 `notify_one()`，由 `sidecar::start_and_monitor` 监听并按 `RESTART_DEBOUNCE_MS` 去抖。
- 握手协议：sidecar 启动后向 stdout 输出 `DS_PICK_READY {...}`（含 `port`、`pid`、`token_fp`、`version`），supervisor 据此判定就绪；运行期通过 stdin `op: ping | drain` 做心跳/优雅退出。

**TUI 同进程路径（非 HTTP）：** `spawn_engine` → `EngineHandle` → `Op`，直接走 mpsc，不经 HTTP（除非 `--http` sidecar 模式）。

---

## 6. Zagens sidecar 监督

`crates/desktop/src/sidecar.rs` 启动嵌入二进制：

```text
deepseek-runtime --host 127.0.0.1 --port {port}
  --cors-origin http(s)://tauri.localhost
  (legacy dev fallback: deepseek-tui serve --http --host … --port …)
环境变量:
  DEEPSEEK_RUNTIME_TOKEN=<UUID from main.rs>
  DEEPSEEK_CLIENT_SURFACE=zagens
  DEEPSEEK_API_KEY=<OS keyring>
  DEEPSEEK_BUNDLED_PYTHON=<Office 工具，可选>
```

监督能力：`/health` 探测、崩溃退避、rapid restart 限制、日志 `~/.deepseek/logs/sidecar.log` + `supervisor.log`。

---

## 7. CLI 子命令路由

| 类型 | 示例命令 | 路径 |
|------|----------|------|
| 转调 tui | `run`, `exec`, `serve`, `resume`, `sessions`, … | `delegate_to_tui` 子进程 → **deepseek-tui**（TUI/headless） |
| HTTP sidecar | `deepseek-runtime`（Zagens 嵌入）或 `deepseek serve --http` | `runtime-server` binary 或 tui `serve --http` → 同一 `runtime_api` |
| CLI 内建 | `config`, `thread list`, `model`, `sandbox`, `auth` | workspace 库（含 `deepseek-state` legacy） |
| ~~实验~~ | ~~`app-server`~~ | **已删除**（D7 C5） |

---

## 8. 典型 Desktop 发消息时序

```mermaid
sequenceDiagram
    autonumber
    participant User as 用户
    participant UI as web-ui Composer
    participant Cli as api/client.ts
    participant Proxy as runtime_proxy.rs
    participant Http as axum /v1/stream
    participant Mgr as RuntimeThreadManager
    participant Port as TurnEnginePort (core)
    participant Eng as Engine (core)
    participant Plat as platform_dispatch (tui)
    participant Turn as handle_deepseek_turn (core)
    participant LLM as DeepSeek API
    participant Pers as persist.rs + SQLite

    User->>UI: 输入 + 发送
    UI->>Cli: streamTurn(req)
    Cli->>Proxy: invoke runtime_post_stream(body)
    Proxy->>Http: POST /v1/stream  Bearer {token}
    Http->>Mgr: create_thread + start_turn(StartTurnRequest)
    Mgr->>Port: validate StartTurnParams
    Port->>Eng: Op::SendMessage (mpsc)
    Eng->>Plat: EnginePlatformExt::dispatch_op
    Plat->>Turn: handle_deepseek_turn(ctx)
    loop SSE chunks
        Turn->>LLM: chat (SSE)
        LLM-->>Turn: delta / tool_use / finish
        Turn-->>Eng: Event (thinking/message/tool)
        Eng-->>Mgr: broadcast RuntimeEventRecord
        Mgr->>Pers: append event / turn / item
        Mgr-->>Http: tail broadcast
        Http-->>Proxy: SSE frame
        Proxy-->>Cli: emit runtime://stream-chunk
        Cli-->>UI: 渲染增量
    end
    Turn-->>Eng: turn.completed (usage)
    Eng-->>Mgr: 终态事件
    Mgr->>Pers: 落盘 turn 结束态
    Proxy-->>Cli: runtime://stream-done
```

**关键点（容易混淆的两处）：**

- **`Op::SendMessage` 是同进程 mpsc**，不是 HTTP；Engine 后台 task 长期持有 `rx_op`，并向 `tx_event` 发事件。`RuntimeThreadManager` 用 `tokio::sync::broadcast` 把事件**同时**喂给 SSE handler 与持久化 monitor。
- **取消/打断有两层**：上层 `runtime_cancel_sse`（仅断开 WebView ↔ sidecar 的 SSE）；下层 `POST /v1/threads/{id}/turns/{turn_id}/interrupt`（真正取消 turn loop，发 `Op::Interrupt`，触发 `CancellationToken`）。前者断流后 turn 仍在跑；后者才停止 LLM/工具调用。

---

## 9. 关键模块索引

| 关注点 | 路径 |
|--------|------|
| Tauri 主入口 / token UUID / 7878 端口 | [`crates/desktop/src/main.rs`](../../crates/desktop/src/main.rs) |
| Sidecar spawn / supervisor / 退避 | [`crates/desktop/src/sidecar.rs`](../../crates/desktop/src/sidecar.rs) |
| Tauri IPC handlers（通道 A） | [`crates/desktop/src/commands.rs`](../../crates/desktop/src/commands.rs) |
| Runtime HTTP/SSE 代理（通道 B） | [`crates/desktop/src/runtime_proxy.rs`](../../crates/desktop/src/runtime_proxy.rs) |
| 多窗口注册 | [`crates/desktop/src/window_registry.rs`](../../crates/desktop/src/window_registry.rs) |
| 终端 PTY | [`crates/desktop/src/terminal.rs`](../../crates/desktop/src/terminal.rs) |
| Sidecar 二进制嵌入 | [`crates/desktop/build.rs`](../../crates/desktop/build.rs) |
| Sidecar 入口 / `deepseek-runtime` | [`crates/runtime-server/src/main.rs`](../../crates/runtime-server/src/main.rs) · [`runtime_serve.rs`](../../crates/tui/src/runtime_serve.rs) |
| Sidecar dev fallback | [`crates/tui/src/main.rs`](../../crates/tui/src/main.rs) · `Commands::Serve {--http}` |
| HTTP 路由表 + Bearer 中间件 | [`crates/tui/src/runtime_api/router.rs`](../../crates/tui/src/runtime_api/router.rs) + [`auth.rs`](../../crates/tui/src/runtime_api/auth.rs) |
| SSE handlers | [`crates/tui/src/runtime_api/stream.rs`](../../crates/tui/src/runtime_api/stream.rs) |
| HTTP server 装配 | [`crates/tui/src/runtime_api/mod.rs`](../../crates/tui/src/runtime_api/mod.rs)（`run_http_server`） |
| 线程管理 / LRU / broadcast | [`crates/tui/src/runtime_threads/manager.rs`](../../crates/tui/src/runtime_threads/manager.rs) |
| 持久化（事件/线程） | [`crates/tui/src/runtime_threads/persist.rs`](../../crates/tui/src/runtime_threads/persist.rs) + [`monitor.rs`](../../crates/tui/src/runtime_threads/monitor.rs) |
| Engine struct + op loop (core) | [`crates/core/src/engine/runtime.rs`](../../crates/core/src/engine/runtime.rs) · [`op_loop.rs`](../../crates/core/src/engine/op_loop.rs) |
| tui Engine shim + platform dispatch | [`crates/tui/src/core/engine.rs`](../../crates/tui/src/core/engine.rs) · [`platform_dispatch.rs`](../../crates/tui/src/core/engine/platform_dispatch.rs) |
| Turn loop / Port / Session (core) | [`crates/core/src/engine/`](../../crates/core/src/engine/) |
| Web 客户端 | [`crates/desktop/web-ui/src/api/client.ts`](../../crates/desktop/web-ui/src/api/client.ts) · [`turnControl.ts`](../../crates/desktop/web-ui/src/api/turnControl.ts) |
| CLI 转调 | [`crates/cli/src/lib.rs`](../../crates/cli/src/lib.rs)（`delegate_to_tui`） |
| OpenAPI / TS 类型 | [`docs/tech/openapi/zagens-runtime-v1.openapi.json`](./openapi/zagens-runtime-v1.openapi.json) · `export-runtime-openapi` bin |
| Sidecar 契约测（lib / in-proc） | [`crates/tui/src/runtime_api/tests.rs`](../../crates/tui/src/runtime_api/tests.rs) · `sidecar_contract_full_lifecycle` |
| Sidecar 契约测（binary / D6 A+） | [`crates/runtime-server/tests/sidecar_binary_contract.rs`](../../crates/runtime-server/tests/sidecar_binary_contract.rs) · CI ubuntu |

---

## 10. 产品定位与架构稳定性

自 **2026-05-24** 战略签收（[DEV_NOTES.md](../desktop/DEV_NOTES.md)）：

- **D12 Desktop-only：** Zagens 为唯一用户产品壳；ratatui TUI 进入 **freeze**（维护/回归，不做 parity 新特性）。
- **Sidecar 执行面：** 生产 binary 为 **`deepseek-runtime`**（`runtime_api` + `runtime_threads` 源码仍在 `deepseek-tui` lib）；dev fallback 保留 `deepseek-tui serve --http`。
- **D6 闭合（2026-05-26）：** Phase A（不链 ratatui）+ Phase A+（真实 binary 契约测 + CI）— [D6_IMPLEMENTATION_PLAN §3](./adr/D6_IMPLEMENTATION_PLAN.md)。
- **架构定型（2026-05-26）：** [ARCHITECTURE_ASSESSMENT §1 = 10/10](./adr/ARCHITECTURE_ASSESSMENT_2026-05-25.md) — M-series（Engine 入核）、D6（sidecar 去 ratatui）、D7（持久化叙事）、D8（OpenAPI）、D1（巨型文件策略）均已闭合；**可正常推进产品功能**，仍须遵守 Assessment §7.1 红线（`/v1/*` 须 OpenAPI、`desktop` 不链 `core`/`tui` 等）。
- **D10 已解除（2026-05-24）：** P2 后桌面 GAP 解冻；见 [P2_D10_UNFREEZE_RECORD.md](./adr/P2_D10_UNFREEZE_RECORD.md)。

CLI 保留为 **dev / headless / CI** 入口，与 Zagens 共享同一 HTTP `/v1/*` 契约。

**剩余非阻塞债（定型后）：** `runtime_api` / `runtime_threads` 源码物理迁入 `runtime-server` lib（**D6 Phase B**，需先 `agent-host` 中间 crate — [D6_PHASE_B_SPIKE.md](./adr/D6_PHASE_B_SPIKE.md)）；`deepseek-state` CLI legacy；P2（D11–D14）。
