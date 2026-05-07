# DeepSeek-TUI 桌面端实现方案

> **方案代号**：方案 A — Tauri + Web 前端  
> **版本**：v3（文档链接 + Phase 2a 里程碑）  
> **基础版本**：DeepSeek-TUI v0.8.15  
> **日期**：2026-05-07  

---

## 一、方案概述

在现有 DeepSeek-TUI 终端代理的基础上，新增一个**原生桌面应用程序**。用户双击桌面图标即可打开独立窗口，通过图形界面使用全部功能——对话、工具调用、审批、配置管理等。

### 1.1 核心原则

- **双 UI 共存**：TUI（终端）和 Desktop（桌面）共享同一引擎，用户可以自由选择
- **复用已有 API**：TUI 的 `runtime_api.rs` 已经提供了完整的 HTTP/SSE 流式 turn API（`/v1/stream`、`/v1/threads/*` 等），桌面端直接对接，不自造轮子
- **Sidecar 模式**：Tauri 桌面壳启动一个 `deepseek-tui` 子进程，使用 **`serve --http`** 启动运行时 HTTP/SSE API（与仓库中 `deepseek serve --http` / `deepseek-tui serve --http` 一致），Web 前端通过 HTTP/SSE 与之通信
- **引擎不改**：`deepseek-core` 等基础 crate 零改动；仅 `runtime_api` 侧可能需要少量适配

### 1.2 关键现状（修正原 v1 错误）

| v1 错误假设 | 实际代码现状 |
|------------|------------|
| `app-server` 的 `POST /thread` 能跑完整 turn | `core::Runtime::handle_thread(Message)` 只做入队 + 返回 `"accepted"`/`"queued"` 占位事件 |
| 桌面端需要全新 SSE 端点 | `crates/tui/src/runtime_api.rs` **已有** `POST /v1/stream` SSE 端点，可创建 thread、启动 turn、实时流式推送事件 |
| 需要自己造事件流 | [`crates/tui/src/runtime_threads.rs`](../crates/tui/src/runtime_threads.rs) 已有完整的 Engine 管理 + broadcast 事件通道 |
| `app-server` 是唯一 HTTP 层 | `runtime_api` 已有 20+ 个 REST 端点（threads、sessions、tasks、automations、mcp 等）|
| SSE 事件名可自定义 | 已有 `RuntimeEventRecord` + `map_compat_stream_event()`（如 `message.delta`、`tool.started`）；若要对齐 `EventFrame`，需在服务端做一次映射/收口 |

### 1.3 最终产物

| 平台 | 产物 |
|------|------|
| Windows | `DeepSeek-Setup.exe` 安装包 |
| macOS | `DeepSeek.dmg` 磁盘映像 |
| Linux | `DeepSeek.AppImage` / `.deb` / `.rpm` |

### 1.4 文档与链接约定

- 文内引用源码与 crate 时，使用**相对本文件的路径**（`../crates/…`），不使用 `file:///` 或本机盘符，便于在任意 clone 路径与 Git 托管页面上跳转。
- **Phase 2a**（下文第十节）为刻意插入的里程碑：在 Phase 3 做完整工具与 **ApprovalDialog** 之前，先落地「HTTP 审批」运行时，避免出现仅有前端弹窗、后端仍同步 deny 的死局。

---

## 二、架构设计

### 2.1 引擎选型（新增——核心架构决策）

桌面端需要一个"完整的 Agent Turn 管道"：LLM 流式请求 → 工具调用 → 审批 → 工具执行 → 下一轮请求。当前仓库中存在**两条不同的 turn 管道**，需要选定对接哪一条。

#### 现有两条管道对照

| 维度 | A: `app-server` → `core::Runtime` | B: `runtime_api` → `RuntimeThreadManager` → `Engine` |
|------|----------------------------------|------------------------------------------------------|
| **代码位置** | [`crates/core/src/lib.rs`](../crates/core/src/lib.rs) | [`crates/tui/src/runtime_threads.rs`](../crates/tui/src/runtime_threads.rs) |
| **`handle_thread(Message)` 行为** | 入队 + 返回 `"accepted"` 占位事件 ← **不跑 turn** | 通过 `start_turn()` → `Engine` **真跑完整 turn** |
| **`handle_prompt` 行为** | 只解析模型 + 记录 hook 事件 ← **不调 LLM** | 不通过该路径，走 `stream_turn` |
| **LLM 流式** | ❌ 无 | ✅ `Engine::handle_deepseek_turn()`（[`turn_loop.rs`](../crates/tui/src/core/engine/turn_loop.rs)） |
| **工具执行** | `invoke_tool`（单次调用，需外部触发） | `Engine` 自动编排（turn loop 内嵌） |
| **审批流** | 策略引擎存在但不自动触发 | `Engine` 内审批；**注意**：`RuntimeThreadManager` 在收到 `ApprovalRequired` 时会按 `auto_approve`/`trust_mode` **同步**调用 `approve`/`deny`，**无**「等待 HTTP 再批准」——桌面若要交互式审批需新增运行时能力（见 §4.2） |
| **SSE 端点** | 无（一次性 JSON） | **已有** `POST /v1/stream` |
| **事件模型** | `EventFrame` (protocol) | `EngineEvent` (core/events) → `RuntimeEventRecord` |
| **已有鉴权** | 无（CORS 全开） | **已有** `require_runtime_token` 中间件；CLI 对应 `--auth-token` / `DEEPSEEK_RUNTIME_TOKEN` |
| **已有并发** | `Arc<Mutex<Runtime>>`（锁整条） | 每 thread 独立 Engine（不共享锁） |
| **持久化** | `StateStore` (SQLite) | `RuntimeThreadManager` (JSONL + Engine session) |

#### 推荐路线：三级火箭

```
Phase 1（本方案）: 直接对接 B — runtime_api 已有 SSE 端点
  ├─ 桌面端 web-ui → POST /v1/stream (SSE)
  ├─ Sidecar 启动: deepseek-tui serve --http --auth-token <token>（等价 `deepseek serve --http`，见分发包中 binary 布局）
  ├─ 引擎改动: ≈0 行（runtime_api 已就绪）
  └─ 代价: 依赖 tui crate 的 Engine（与 TUI 共享 runtime）

Phase 2（中期架构升级）: 将 Engine turn 管道上移至 deepseek-core
  ├─ 提取 Engine::handle_deepseek_turn() 到 core::Runtime
  ├─ core::Runtime::handle_thread(Message) → 真正跑 turn
  ├─ app-server 自然获得完整 SSE 能力
  ├─ 代价: 打破 "core 零改动"（但长期最干净）
  └─ 时机: 桌面端 MVP 验证后 2-3 周

Phase 3（远期可选）: 桌面端切到 app-server
  ├─ 此时 app-server 已有完整 turn 管道
  ├─ 桌面 sidecar 改为 app-server binary
  └─ TUI 和 Desktop 真正共享同一 Runtime
```

**本方案（Phase 1）采用路线 B（runtime_api）**——原因：

1. **零引擎改动**：`/v1/stream` SSE 端点已经在跑完整的 Agent turn，不需要新写一行 Rust engine 代码
2. **已有鉴权**：`runtime_token` 机制解决本地 API 安全问题
3. **已有并发隔离**：每 thread 独立 Engine 实例，不共享锁
4. **已有实例检测**：`RuntimeThreadManager` 管理活跃 Engine 的 lifecycle

### 2.2 整体架构（修正后）

```
┌──────────────────────────────────────────────────────────────┐
│                    Tauri Desktop Shell                         │
│  ┌────────────────────────────────────────────────────────┐  │
│  │              Web 前端 (React)                            │  │
│  │  ┌──────────┐ ┌──────────┐ ┌──────────┐ ┌──────────┐  │  │
│  │  │ ChatView │ │ ToolPanel│ │ ConfigPg │ │ Sidebar  │  │  │
│  │  └──────────┘ └──────────┘ └──────────┘ └──────────┘  │  │
│  │         │            │           │           │          │  │
│  │         └────────────┴───────────┴───────────┘          │  │
│  │                        │ HTTP + SSE                      │  │
│  └────────────────────────┼────────────────────────────────┘  │
│  ┌────────────────────────┼────────────────────────────────┐  │
│  │              Tauri Rust Backend (薄层)                    │  │
│  │  - 启动 sidecar (`deepseek-tui serve --http` / `--auth-token`) │  │
│  │  - 注入 token 到 WebView window.__DEEPSEEK__             │  │
│  │  - 剪贴板/文件对话框/系统托盘 IPC                         │  │
│  └────────────────────────┼────────────────────────────────┘  │
└───────────────────────────┼───────────────────────────────────┘
                            │ HTTP (localhost:7878)
                            ▼
┌──────────────────────────────────────────────────────────────┐
│       deepseek-tui serve --http (sidecar 子进程)              │
│  ┌──────────────────────────────────────────────────────┐   │
│  │  runtime_api.rs                                       │   │
│  │  ├─ POST /v1/stream        ← SSE 流式 turn ★          │   │
│  │  ├─ GET  /v1/threads       ← 会话列表                 │   │
│  │  ├─ POST /v1/threads       ← 创建会话                 │   │
│  │  ├─ GET  /v1/threads/{id}  ← 会话详情                 │   │
│  │  ├─ POST /v1/threads/{id}/turns        ← 启动 turn    │   │
│  │  ├─ POST /v1/threads/{id}/turns/{id}/steer   ← steer  │   │
│  │  ├─ POST /v1/threads/{id}/compact      ← compact      │   │
│  │  ├─ GET  /v1/sessions      ← 已保存会话               │   │
│  │  ├─ GET  /v1/tasks         ← 任务列表                 │   │
│  │  ├─ GET  /v1/skills        ← 可用技能                 │   │
│  │  └─ ... (20+ 端点)                                    │   │
│  ├──────────────────────────────────────────────────────┤   │
│  │  RuntimeThreadManager                                 │   │
│  │  ├─ create_thread / start_turn / ...                  │   │
│  │  ├─ ensure_engine_loaded → spawn_engine               │   │
│  │  └─ broadcast 事件通道 (EVENT_CHANNEL_CAPACITY=1024)   │   │
│  └──────────────────────────────────────────────────────┘   │
│                            │                                  │
│                            ▼                                  │
│  ┌──────────────────────────────────────────────────────┐   │
│  │  Engine (每 thread 独立实例)                           │   │
│  │  ├─ handle_deepseek_turn() → LLM 流式请求             │   │
│  │  ├─ 工具调用编排 + 审批 + 执行                         │   │
│  │  ├─ EngineEvent → RuntimeThreadManager → SSE          │   │
│  │  └─ tx_event (mpsc) → 上一层消费                      │   │
│  └──────────────────────────────────────────────────────┘   │
└──────────────────────────────────────────────────────────────┘
```

### 2.3 已有模块复用清单

| 模块 | 路径 | 角色 | 改动 |
|------|------|------|------|
| `deepseek-core` | `crates/core/` | 会话/作业/工具执行原语 | **0 改动** |
| `deepseek-protocol` | `crates/protocol/` | 请求/响应/事件类型 | **0 改动** |
| `deepseek-tools` | `crates/tools/` | 工具注册与调度 | **0 改动** |
| `deepseek-agent` | `crates/agent/` | 模型注册表 | **0 改动** |
| `deepseek-mcp` | `crates/mcp/` | MCP 客户端 | **0 改动** |
| `deepseek-config` | `crates/config/` | 配置管理 | **0 改动** |
| `deepseek-execpolicy` | `crates/execpolicy/` | 执行策略引擎 | **0 改动** |
| `deepseek-hooks` | `crates/hooks/` | 钩子系统 | **0 改动** |
| `deepseek-state` | `crates/state/` | SQLite 持久化 | **0 改动** |
| `deepseek-secrets` | `crates/secrets/` | 密钥管理 | **0 改动** |
| `runtime_api.rs` | `crates/tui/src/runtime_api.rs` | HTTP + SSE API（**已有，核心复用**） | **~100 行新增**（Web 前端专用端点 / CORS 适配） |
| `runtime_threads.rs` | `crates/tui/src/runtime_threads.rs` | Engine 管理 + 事件广播（**已有，核心复用**） | **~50 行新增**（事件映射 / Last-Event-ID 重放） |

### 2.4 新增模块

| 模块 | 路径 | 描述 |
|------|------|------|
| `deepseek-desktop` | `crates/desktop/` | Tauri 应用入口 + Rust 后端薄层 |
| `web-ui` | `crates/desktop/web-ui/` | Web 前端项目（React） |

---

## 三、技术选型

### 3.1 Rust 后端

| 选项 | 选型 | 理由 |
|------|------|------|
| 桌面框架 | **Tauri v2** | 原生窗口 + WebView2，跨平台，体积小 |
| 引擎对接 | **runtime_api SSE** | 已有 `/v1/stream`，无需新造 |
| 流式推送 | **SSE**（Server-Sent Events），**POST 流** + **GET 订阅** | `POST /v1/stream` 无法用浏览器原生 `EventSource`（仅支持 GET）；前者用 **`fetch()` + ReadableStream** 解析 SSE；`GET /v1/threads/{id}/events` 可用 `EventSource` 或同样 fetch |
| 进程管理 | Tauri **sidecar** | `deepseek-tui serve --http …` 作为 sidecar 子进程（官方 CLI 为 `deepseek serve --http`） |
| 安全鉴权 | 运行时 token（已有） | `--auth-token` / `DEEPSEEK_RUNTIME_TOKEN` + Bearer；中间件仍为 `require_runtime_token` |
| 系统集成 | Tauri Plugin API | 系统托盘、通知、快捷键、自动启动 |

### 3.2 Web 前端

| 选项 | 候选 | 推荐 |
|------|------|------|
| 框架 | React / Svelte / SolidJS | **React**（生态最丰富）或 **Svelte**（体积最小） |
| 构建工具 | Vite | 必选，快速 HMR |
| 样式方案 | Tailwind CSS + shadcn/ui | 组件化，主题切换方便 |
| 流式渲染 | `fetch` + ReadableStream（POST SSE）或 `EventSource`（仅限 GET `/v1/threads/.../events`） | POST 流不能用原生 EventSource |
| Markdown | markdown-it + highlight.js | 对话内容渲染 |
| Diff 展示 | diff2html | 文件变更可视化 |
| 终端模拟 | xterm.js | Shell 命令输出展示 |
| 图表 | recharts | Token 用量/成本统计 |

### 3.3 推荐组合

```
Tauri 2 + React 18 + TypeScript + Vite + Tailwind CSS
```

---

## 四、关键数据流（基于 runtime_api 修正）

### 4.1 用户发送消息（SSE 流式 turn）

```
用户输入文字 → 点击发送
       │
       ▼
Web UI: **fetch(`POST /v1/stream`)** + **ReadableStream 解析 SSE**（带 `Authorization: Bearer`；**不能**用原生 `EventSource`，其对 POST 不可用）
       │
       ▼
runtime_api::stream_turn():
  1. create_thread(model, workspace, mode)
  2. start_turn(thread_id, prompt)
     └─ RuntimeThreadManager::ensure_engine_loaded()
        └─ spawn_engine(EngineConfig) → 启动独立 Engine 实例
  3. 回放 backlog events (events_since)
  4. 订阅 live events (subscribe_events → broadcast channel)
  5. SSE stream: 逐帧 yield SseEvent（`map_compat_stream_event`）
       │
       ▼
SSE 事件流 → Web UI:
  event: turn.started（stream_turn 内手写首帧）
  event: message.delta / tool.progress / tool.started / tool.completed / …
       （与 `map_compat_stream_event()` 一致；**非**稳定的 `thinking.delta`/`response.delta` 字面名）
  event: turn.completed
  event: done             → SSE 流关闭
```

### 4.2 工具调用审批流（与当前实现对齐）

**现状（重要）**：`RuntimeThreadManager` 在处理 `EngineEvent::ApprovalRequired` 时，会在同一协程内根据该 turn 的 `auto_approve` / `trust_mode` 调用 `engine.approve_tool_call` 或 `deny_tool_call`，并发出 `approval.required` SSE 仅供**观测**。**不存在**文档旧版所述的 `POST /v1/threads/{id}/turns/{turn_id}/approve` 路由；未 `auto_approve` 时策略为**立即 deny**，无法像 TUI 那样暂停等待用户按键。

**桌面端要做「Agent + 交互式审批」必须二选一或组合**：

1. **MVP / 对齐当前 HTTP 语义**：桌面仅支持 **YOLO / `auto_approve: true`** 路径跑通工具（与现有 runtime 一致），或仅 Plan 模式等不落审批的路径。  
2. **产品级 Agent 审批**：在 `runtime_threads` / `Engine` 侧增加**待定审批队列**（pending approval id），并新增 HTTP 批准/拒绝路由。**实施顺序见第十节 Phase 2a**，须先于 Phase 3 `ApprovalDialog` 完整验收。

以下为**目标交互**（待上述 API 落地后）：

```
LLM 请求工具调用 (如 shell 命令)
       │
       ▼
Engine: ExecPolicyEngine::pre_check() → 需要审批
       │
       ▼
（待定实现）Runtime 暂停工具执行 → SSE 推送 approval.required
       │
       ▼
Web UI: 弹出审批对话框
       │
       ▼
用户点击 "批准"
  → POST …/approve { tool_call_id, decision }  （**待实现**）
  → Engine: approve → 工具执行 → SSE 推送 tool.*
       │
       ▼
Web UI: 终端面板实时显示命令输出
```

### 4.3 会话恢复

```
用户启动桌面应用
       │
       ▼
Web UI: GET /v1/sessions  → 获取已保存会话列表
       │
       ▼
Web UI: 侧边栏显示会话列表
       │
       ▼
用户点击某个会话
  → GET /v1/sessions/{id}/resume-thread
  → 解析 history → 渲染完整对话
```

---

## 五、SSE 事件协议（与 EventFrame 对齐）

### 5.1 事件类型映射

`runtime_api` 当前使用 `RuntimeEventRecord`（含 `event: String` 字段，如 `"turn.started"`），`stream_turn` 通过 `map_compat_stream_event()` 做了一层映射。对于桌面端，SSE 事件应该**直接序列化 `EventFrame`**（见 [`crates/protocol/src/lib.rs`](../crates/protocol/src/lib.rs) 中 `EventFrame` 定义），避免维护两套平行的事件名称。

```rust
// EventFrame 枚举（protocol crate，已存在，不改）
#[serde(tag = "event", rename_all = "snake_case")]
pub enum EventFrame {
    ResponseStart { response_id: String },
    ResponseDelta { response_id: String, delta: String },
    ResponseEnd   { response_id: String },
    ToolCallStart { response_id: String, tool_name: String, arguments: Value },
    ToolCallResult { response_id: String, tool_name: String, output: Value },
    ExecApprovalRequest { request: ExecApprovalRequestEvent },
    ExecCommandBegin { command: String, cwd: String },
    ExecCommandOutputDelta { command: String, delta: String },
    ExecCommandEnd { command: String, exit_code: i32 },
    PatchApplyBegin { path: String },
    PatchApplyEnd { path: String, ok: bool },
    TurnStarted { turn_id: String },
    TurnComplete { turn_id: String },
    TurnAborted { turn_id: String, reason: String },
    Error { response_id: String, message: String },
    // ... 还有 McpStartupUpdate, McpToolCallBegin/End 等
}
```

### 5.2 SSE 端点适配

runtime_api 已有 `map_compat_stream_event()` 将内部事件映射为 SSE 格式。桌面端适配只需：

1. `GET /v1/threads/{id}/events` SSE 端点（**已存在**：`stream_thread_events`）
2. （可选）在服务端增加 `EventFrame` 兼容层，统一对外 JSON；或沿用 `map_compat_stream_event` 的 event 名 + 类型化 TS 前端
3. **Thinking 与最终回答**：当前 `map_compat_stream_event()` 对 `item.delta` / `agent_message` 统一打出 `message.delta`，**未**单独分出 `thinking` 事件名。若产品上要区分 💭 与正文，需要在映射层按 payload（如 `kind`、是否 reasoning 流）拆事件，或在前端根据 thread 元数据约定解析——与「Web 端通过 event 名 thinking/response 区分」的表述**尚未一致**，实现时需单列任务。

### 5.3 不需要改 protocol（可选扩展）

`EventFrame` 已覆盖大量语义。完全对齐 `EventFrame` 可在**服务端序列化层**完成，**不一定**要改 `protocol` crate。若 thinking 需独立 variant，再评估是否扩展协议。

---

## 六、可靠性设计

### 6.1 SSE 断线重连

| 策略 | 说明 |
|------|------|
| **游标 `since_seq`** | `GET /v1/threads/{id}/events?since_seq=<seq>` 已实现 backlog +  live（见 `ThreadEventsQuery`）。重连时客户端携带**上次收到的最大 `seq`**（在 payload 的 JSON 里，见 `runtime_event_payload`），无需假设服务端已设置 SSE `id:` 字段 |
| **标准 Last-Event-ID** | 可选增强：在 `sse_json` 输出中为每条事件设置 `id: <seq>`，即可与浏览器 `EventSource` 的 `Last-Event-ID` 对齐。**当前实现以查询参数为主** |
| **POST `/v1/stream` 重连** | 单连接 POST 流一般需**整段重试**（重新发起 POST）或改用「先 `POST …/turns` 再只订 `GET …/events`」的组合，不能依赖 `EventSource` 自动重连 POST |

### 6.2 broadcast 丢事件防护

当前 `runtime_threads.rs` 使用 `EVENT_CHANNEL_CAPACITY = 1024` 的 broadcast channel。慢消费者可能丢事件。

| 措施 | 说明 |
|------|------|
| **容量加大** | 从 1024 → 4096（够 4 个长 turn 的事件量） |
| **Lag 监控** | 订阅端若 `recv()` 返回 `Err(Lagged(n))`，说明已丢 n 条，应立刻用 `since_seq` **全量补读** persisted events，而不是依赖 broadcast |
| **快照重放** | 后端 `events_since(thread_id, Some(last_seq))` 从 JSONL 持久化补发；前端重连时带最新 `since_seq` |
| **KeepAlive** | 已有：`KeepAlive::new().interval(Duration::from_secs(15))` — 15 秒心跳防超时断开 |

### 6.3 并发与锁粒度

当前 `runtime_api` 的并发模型优于 `app-server`：

| 维度 | app-server | runtime_api |
|------|-----------|-------------|
| **锁模型** | `Arc<Mutex<Runtime>>` — 所有操作共享一把锁 | 每 thread 独立 `Engine` 实例（不共享锁） |
| **turn 阻塞** | 长 turn 阻塞所有其他 HTTP 请求 | 不同 thread 的 turn 互相独立 |
| **列表/配置操作** | 被 turn 卡住 | 可并发进行 |

**桌面端直接受益于此模型**——Sidecar 进程的 runtime_api 不会因为一个对话 turn 而阻塞配置读取或会话列表查询。

### 6.4 安全设计

```
┌────────────────────────────────────────────┐
│         安全层次                              │
├────────────────────────────────────────────┤
│ 1. 绑定地址: 127.0.0.1 (不对外暴露)          │
│ 2. 随机 token: 启动时生成 UUID token          │
│    ├─ deepseek-tui serve --http \          │
│    │   --auth-token <random-uuid>          │
│    │   （或环境变量 DEEPSEEK_RUNTIME_TOKEN）   │
│    └─ 写入 Tauri 启动 sidecar 的环境变量       │
│ 3. Tauri 注入: WebView 加载前注入 token      │
│    window.__DEEPSEEK_TOKEN__ = "<token>"    │
│ 4. 请求鉴权: 所有 /v1/* 路由需要             │
│    Authorization: Bearer <token>            │
│    (已有 require_runtime_token 中间件)       │
│ 5. Sidecar 实例检测:                        │
│    启动前检查端口是否被自身 token 持有        │
│    (避免误连其他 DeepSeek 实例)              │
└────────────────────────────────────────────┘
```

**注意**：现有 `app-server` 使用 `CorsLayer::permissive()` 且无鉴权——桌面端不应使用 `app-server` 作为 HTTP 层，而应使用已有鉴权的 `runtime_api`。

---

## 七、Tauri 桌面壳设计

### 7.1 项目结构

```
crates/desktop/
├── Cargo.toml              # Tauri 应用配置
├── tauri.conf.json         # Tauri 窗口/打包配置
├── icons/                  # 应用图标 (多尺寸)
├── src/
│   ├── main.rs             # Tauri 入口
│   ├── commands.rs         # Tauri commands (IPC bridge)
│   ├── sidecar.rs          # deepseek-tui sidecar 管理
│   └── lib.rs
└── web-ui/                 # Web 前端项目
    ├── package.json
    ├── vite.config.ts
    ├── index.html
    ├── src/
    │   ├── main.tsx
    │   ├── App.tsx
    │   ├── api/
    │   │   ├── client.ts        # HTTP + SSE 客户端（含 Bearer token）
    │   │   └── types.ts         # EventFrame 对应 TypeScript 类型
    │   ├── components/
    │   │   ├── ChatView.tsx      # 对话视图
    │   │   ├── MessageBubble.tsx # 消息气泡
    │   │   ├── ThinkingBlock.tsx # 推理块渲染
    │   │   ├── Composer.tsx      # 输入框
    │   │   ├── ToolPanel.tsx     # 工具调用面板
    │   │   ├── DiffViewer.tsx    # diff 查看器
    │   │   ├── Terminal.tsx      # 终端输出展示 (xterm.js)
    │   │   ├── ApprovalDialog.tsx# 审批弹窗
    │   │   ├── Sidebar.tsx       # 会话侧边栏
    │   │   ├── ConfigPanel.tsx   # 配置面板
    │   │   └── StatusBar.tsx     # 状态栏
    │   ├── hooks/
    │   │   ├── useSSE.ts         # SSE：GET 用 EventSource/fetch；POST 流用 fetch+ReadableStream；重连用 since_seq
    │   │   ├── useSession.ts     # 会话管理 hook
    │   │   └── useTheme.ts       # 主题管理 hook
    │   ├── store/
    │   │   └── chatStore.ts      # 客户端状态管理
    │   └── styles/
    │       └── globals.css       # 全局样式 + tailwind
    └── tsconfig.json
```

### 7.2 Sidecar 生命周期

```
Tauri 应用启动
  │
  ├─ 生成随机 token: uuid v4
  │
  ├─ 端口探测: 检查默认/选定端口 + token 是否属于本实例
  │    ├─ 匹配 → 复用已有进程
  │    └─ 不匹配或未运行 → 启动 sidecar（示例）:
  │         deepseek-tui serve --http --port 7878 --auth-token <token>
  │         （分发包若以 `deepseek` 为入口则为 `deepseek serve --http …`）
  │
  ├─ Web UI 加载前注入 token:
  │    <script>window.__DEEPSEEK_TOKEN__ = "<token>"</script>
  │
  ├─ Web UI: GET /health 确认就绪
  │
  ├─ 运行期间:
  │    ├─ 定期 GET /health 心跳检测（每 5 秒）
  │    ├─ 3 次失败 → 自动重启 sidecar
  │    └─ 重启时复用同一 token（保留已有 token）
  │
  └─ 应用关闭: sidecar 优雅退出 (SIGTERM → 保存 sessions)
```

### 7.3 Tauri Commands（Rust → Web IPC）

```rust
// src/commands.rs
#[tauri::command]
async fn get_runtime_token() -> String;

#[tauri::command]
async fn get_runtime_port() -> u16;

#[tauri::command]
async fn get_platform_info() -> PlatformInfo;

#[tauri::command]
async fn open_file_dialog() -> Option<String>;

#[tauri::command]
async fn write_clipboard(text: String) -> Result<(), String>;

#[tauri::command]
async fn read_clipboard() -> Result<String, String>;

#[tauri::command]
async fn get_os_theme() -> String;   // "light" | "dark"

#[tauri::command]
async fn get_locale() -> String;      // "zh-CN" | "en" | ...
```

---

## 八、Web 前端核心组件设计

### 8.1 ChatView（对话视图）

```
┌─────────────────────────────────────────────┐
│  [系统消息] 欢迎使用 DeepSeek               │
│                                              │
│  ┌─ User ─────────────────────────────────┐ │
│  │ 帮我写个 Rust 的 HTTP server            │ │
│  └────────────────────────────────────────┘ │
│                                              │
│  ┌─ Assistant ────────────────────────────┐ │
│  │ [💭 Thinking (2.3s)]                    │ │
│  │   用户想创建一个 HTTP server...          │ │
│  │   需要用到 axum 框架...                  │ │
│  │                                         │ │
│  │ [🔧 Tool: shell] cargo init             │ │
│  │   ✓ Completed (0.3s)                    │ │
│  │                                         │ │
│  │ [🔧 Tool: write_file] src/main.rs       │ │
│  │   +45 -0 lines                          │ │
│  │   ✓ Completed                           │ │
│  │                                         │ │
│  │ 好的，我已经帮你创建了一个 HTTP server   │ │
│  └────────────────────────────────────────┘ │
│                                              │
│  ▼ 滚动到最新消息                            │
├─────────────────────────────────────────────┤
│ [Composer] 输入你的问题...   [YOLO|Agent] [发送] │
└─────────────────────────────────────────────┘
```

### 8.2 ThinkingBlock（推理块）

```tsx
// 折叠式推理过程展示
<ThinkingBlock duration={2.3} reasoningEffort="high" collapsed={false}>
  <Markdown content={reasoningBuffer} />
</ThinkingBlock>
```

### 8.3 ToolPanel（工具调用面板）

每次工具调用渲染为可展开卡片：

```tsx
<ToolCard
  tool="shell"
  params={{ command: "cargo build" }}
  status="running"
>
  <Terminal output={streamingOutput} />
  <StatusBadge status="completed" duration={1.2} />
</ToolCard>
```

### 8.4 ApprovalDialog（审批弹窗）

```
┌─────────────────────────────────────────────┐
│  ⚠️ 需要你的审批                             │
│                                              │
│  Shell 命令:                                 │
│  ┌─────────────────────────────────────────┐│
│  │ $ cargo build --release                  ││
│  └─────────────────────────────────────────┘│
│  ⚡ 风险评估: medium                          │
│                                              │
│  [拒绝]  [单次批准]  [始终批准此类操作]        │
└─────────────────────────────────────────────┘
```

### 8.5 Sidebar（侧边栏）

```
┌──────────────┐
│  📁 Sessions  │
│  ├─ ❯ 新的会话 │
│  ├─ HTTP server│
│  ├─ 重构代码   │
│  └─ ...        │
│               │
│  🔧 Settings  │
│  ├─ API Key   │
│  ├─ Model     │
│  │  ├─ V4 Pro │
│  │  └─ V4 Flash│
│  ├─ Theme     │
│  │  ├─ Light  │
│  │  └─ Dark   │
│  └─ About     │
└──────────────┘
```

---

## 九、Cargo Workspace 变更

### 9.1 新增 workspace member

```toml
# 根 Cargo.toml，在 [workspace.members] 中新增：
"crates/desktop"
```

### 9.2 `crates/desktop/Cargo.toml`

```toml
[package]
name = "deepseek-desktop"
version.workspace = true
edition.workspace = true

[build-dependencies]
tauri-build = { version = "2", features = [] }

[dependencies]
tauri = { version = "2", features = [
    "tray-icon",
    "image-png"
] }
tauri-plugin-shell = "2"
tauri-plugin-dialog = "2"
tauri-plugin-clipboard-manager = "2"
tauri-plugin-notification = "2"
tauri-plugin-os = "2"
tauri-plugin-process = "2"
serde.workspace = true
serde_json.workspace = true
tokio.workspace = true
uuid.workspace = true

[features]
default = ["custom-protocol"]
custom-protocol = ["tauri/custom-protocol"]
```

### 9.3 runtime_api 需要的最小适配

```rust
// crates/tui/src/runtime_api.rs — 无需新依赖

// 1. 增加事件映射函数，将 EngineEvent 序列化为 EventFrame JSON
// 2. 确保 GET /v1/threads/{id}/events SSE 端点支持 Last-Event-ID
// 3. 确保 stream_turn SSE 端点支持 Last-Event-ID（当前已有基础）
// 4. broadcast channel 容量 1024 → 4096
```

### 9.4 Tauri sidecar 配置

```json
// tauri.conf.json (sidecar 配置)
{
  "bundle": {
    "externalBin": [
      "binaries/deepseek-tui"
    ]
  }
}
```

运行时 `deepseek-tui` 二进制需打包进应用 bundle。可通过 Tauri sidecar 或 bundle resources 方式嵌入。

---

## 十、实施路线图（修正版）

阶段顺序：**Phase 1**（runtime_api / SSE）→ **Phase 2**（Tauri + 基础对话）→ **Phase 2a**（HTTP 交互式审批，硬性插在 Phase 3 之前）→ **Phase 3–5**（工具可视化、配置、打包）。

**按步骤执行（勾选、验收、依赖）**见 [DESKTOP_IMPLEMENTATION_STEPS.md](./DESKTOP_IMPLEMENTATION_STEPS.md)。

| 任务 | 说明 | 文件 |
|------|------|------|
| broadcast 容量加大 | `EVENT_CHANNEL_CAPACITY: 1024 → 4096` | `runtime_threads.rs` |
| SSE `since_seq` / 可选 `id:` 字段 | `GET …/events` 重放；**可选**为每条 SSE 加 `id:` 以对标 Last-Event-ID | `runtime_api.rs` |
| 事件格式对齐 EventFrame | 在序列化层输出 `EventFrame` 或保持 compat 事件名 + 生成 TS 类型 | `runtime_api.rs` |
| 验证端点可用性 | `curl -N localhost:7878/v1/stream` 能收到完整 turn 事件流 | 集成测试 |

**验收标准**：

```bash
curl -N -X POST http://127.0.0.1:7878/v1/stream \
  -H "Authorization: Bearer <token>" \
  -H "Content-Type: application/json" \
  -d '{"prompt":"say hello","workspace":"/tmp/test","mode":"agent"}' \
  | head -20
# → 输出逐条 SSE 事件: turn.started → message.delta / tool.* → turn.completed → done（事件名以 map_compat_stream_event 为准）
```

### Phase 2：Tauri 脚手架 + 基础对话（预计 2 周）

| 任务 | 说明 |
|------|------|
| Tauri 项目初始化 | `crates/desktop/` 脚手架 + `npm create tauri-app` |
| Vite + React + TypeScript 初始化 | `web-ui/` 前端脚手架 |
| Sidecar 生命周期管理 | 启动/心跳检测/自动重启/优雅退出 |
| Token 注入 | WebView 加载前注入 `window.__DEEPSEEK_TOKEN__` |
| API 客户端 | `fetch` 解析 POST `/v1/stream` SSE；`GET …/events` 可用 EventSource 或 fetch；Bearer + `since_seq` 重连 |
| 基础 ChatView + Composer | 发送消息 → POST /v1/stream → 流式显示回复 |
| 对话历史渲染 | GET /v1/sessions → 恢复历史 |

**验收标准**：能在窗口里和 DeepSeek 对话，看到流式正文（`message.delta`）。若尚未实现 reasoning 单独映射，则 Thinking 区可为空或与正文合并显示——与 §5.2 一致。

### Phase 2a：运行时交互式审批（HTTP）（预计 1～1.5 周）

> **定位**：填补 §4.2 所述缺口——当前 `RuntimeThreadManager` 在 `ApprovalRequired` 上同步 `approve`/`deny`，桌面无法在 Agent 模式下等待用户。**本阶段必须在 Phase 3 的 ApprovalDialog 与 Shell 工具链验收之前完成**（可与 Phase 2 末尾并行，但不得跳过）。

| 任务 | 说明 | 主要文件 |
|------|------|----------|
| 待定审批状态机 | `ApprovalRequired` 时登记 pending（`thread_id`、`turn_id`、`tool_call_id` 等），**不再**在事件回调里立即 `deny`（当需要用户选择时） | `runtime_threads.rs` |
| 与 Engine 对齐的阻塞/唤醒 | `Engine` / turn 侧在工具执行前等待外部 `approve`/`deny` 信号（如 oneshot、`tokio::sync` channel），与现有 `approve_tool_call` / `deny_tool_call` 衔接 | `crates/tui/src/core/engine/`、`runtime_threads.rs` |
| HTTP API | 新增例如 `POST /v1/threads/{id}/turns/{turn_id}/approve`、`…/deny`，或使用单一路由 + JSON body `{ "tool_call_id", "decision" }`；鉴权沿用 `require_runtime_token` | `runtime_api.rs` |
| SSE 一致性 | 继续通过现有通道发出 `approval.required`；用户决策后工具恢复执行，后续事件流不变 | `runtime_api.rs`、`runtime_threads.rs` |
| 超时 / 默认策略 | 与 TUI 行为对齐：**超时视为 deny** 或配置项（文档化） | 同上 |
| 测试 | 集成测试：触发需审批工具 → 未批则挂起 → HTTP 批 → 流式继续 | `runtime_api.rs` 内嵌测试或 `crates/tui/tests` |

**验收标准**：在 `auto_approve: false` 的 Agent turn 中，危险工具调用会停住；桌面通过 HTTP 批准后工具执行并完成 turn；拒绝路径同理。

### Phase 3：完整工具可视化（预计 3 周）

| 任务 | 工具类型 | 说明 |
|------|---------|------|
| ThinkingBlock 渲染 | — | 折叠式，实时流式 |
| Terminal (xterm.js) | Shell | 命令实时输出 |
| File 工具卡片 | ReadFile, WriteFile, EditFile | 文件名 + 行数 + diff |
| DiffViewer | EditFile / ApplyPatch | diff2html 组件 |
| Git 工具卡片 | GitStatus, GitDiff | 分支 + 变更摘要 |
| Web 工具卡片 | FetchUrl, WebSearch | URL + 结果摘要 |
| ToolCard 通用框架 | 所有类型 | 可展开/折叠，状态颜色 |
| ApprovalDialog | Shell / File | **依赖 Phase 2a**（HTTP 审批）；否则仅 UI 壳或 YOLO-only；见 §4.2 |
| SubAgent 状态 | agent_spawn, agent_wait | 卡片 + 进度 + 结果 |
| Task 列表 | TodoWrite | 勾选框 + 进度 |

**当前 tool registry 已有工具清单**（作为验收项列全）：

| 工具族 | 具体工具 |
|--------|---------|
| File | read_file, write_file, edit_file, list_dir |
| Search | grep_files, file_search |
| Shell | exec_shell, shell_wait, shell_interact, shell_cancel |
| Git | git_status, git_diff, git_log, git_branch, git_stash, ... |
| Web | fetch_url, web_search |
| SubAgent | agent_spawn, agent_wait, agent_result, agent_cancel, agent_list |
| Todo | todo_write |
| Plan | read_plan, update_plan |
| Skill | load_skill |
| RLM | rlm |
| Review | review |
| Task | task |
| GitHub | github_issue, github_pr, ... |

### Phase 4：配置 & 会话管理（预计 1 周）

| 任务 | 说明 |
|------|------|
| Sidebar 会话列表 | GET /v1/sessions → 创建/切换/删除 |
| ConfigPanel | API Key 配置（写入 config.toml） |
| Model 选择器 | V4 Pro / V4 Flash 切换 |
| Onboarding 引导 | 首次使用引导流程 |
| 亮/暗主题切换 | 跟随系统 + 手动切换（Tauri 获取 OS theme） |
| 本地化 | zh-CN / en 切换 |

### Phase 5：打包 & 发布（预计 1 周）

| 任务 | 说明 |
|------|------|
| Tauri 打包配置 | NSIS (.exe) + DMG (.dmg) + AppImage |
| deepseek-tui 嵌入 | Sidecar 二进制 bundle |
| 代码签名 | Windows 签名 + macOS 公证 |
| 图标设计 | 各尺寸应用图标 |
| CI/CD | GitHub Actions 跨平台构建 |
| 自动更新 | Tauri updater plugin |

---

## 十一、风险与缓解（修正版）

| 风险 | 概率 | 影响 | 缓解措施 |
|------|------|------|----------|
| **SSE 断线** | 中 | 中 | `since_seq` 补读 +（可选）SSE `id` 字段；POST 流重试策略 |
| **broadcast 丢事件** | 中 | 中 | 容量 4096 + `Lagged` 时用 `events_since` 全量补读 |
| **WebView2 缺失** | 低 | 高 | 检测系统 WebView2，缺失时自动下载安装 |
| **端口冲突** | 低 | 低 | 随机端口分配 + runtime_token 实例验证 |
| **本地 API 安全** | 中 | 高 | 127.0.0.1 绑定 + Bearer token 鉴权（已有 `require_runtime_token`） |
| **Sidecar 实例误连** | 低 | 中 | token 匹配验证——新实例生成新 token，不连旧实例 |
| **大对话性能** | 中 | 中 | 虚拟列表 + 增量渲染 |
| **双 UI 维护负担** | 高 | 中 | Desktop 不增加新功能，保持与 TUI 功能对等 |
| **turn 循环阻塞配置请求** | 低 | 低 | runtime_api 每 thread 独立 Engine，不共享锁 |
| **交互式审批缺失** | 高 | 高 | §4.2：实现 pending approval + HTTP approve/deny；MVP 可用 auto_approve |
| **engine 迁移需求** | 中 | 高 | Phase 1 已有可行路径（B），若后期需迁移到 core 则额外排期 |
| **TUI 二进制体积** | 低 | 低 | Tauri ~5MB + WebView2 系统自带 + `deepseek-tui` ~20MB |

---

## 十二、与现有 TUI 的关系

```
deepseek (CLI)
  ├─ 无参数 → deepseek-tui (终端 TUI)         ← 保持不变
  ├─ -p "..." → one-shot prompt                ← 保持不变
  ├─ serve --http → runtime_api 侧车 / 独立终端 ← 已有，桌面 sidecar 复用同子命令
  ├─ --desktop → deepseek-desktop (新)          ← 可选：新增子命令，等价于启动 Tauri
  └─ doctor / setup / sessions / ...            ← 保持不变
```

桌面端运行方式：

```
方式 1: 桌面快捷方式 → deepseek-desktop.exe（Tauri bundle，自动启动 sidecar）
方式 2: CLI 命令 → deepseek --desktop（启动 Tauri 窗口）
```

---

## 十三、文档处理 Skills 扩展（Phase 6+）

> ⚠️ **前置条件**：此章节在桌面端 Phase 1–5（**含 Phase 2a** HTTP 审批）全部完成后才启动。

桌面端完成后，DeepSeek Desktop 已经是一个功能完整的 AI 编码助手。本章节规划**第二阶段的能力扩展**——让 DeepSeek 不仅能写代码，还能处理本地文档：生成 PPT、操作 Excel、提取 PDF、绘制图表等。

### 13.1 扩展思路

现有 Skills 系统采用**渐进式三层加载**架构——技能指令按需注入模型上下文，不占用基础 prompt 预算。文档处理能力天然适合用 Skill 扩展。

```
Phase 5 完成后的 DeepSeek Desktop
         ↓
┌─────────────────────────────────────┐
│     桌面壳 (Tauri) + Web 前端        │
├─────────────────────────────────────┤
│     runtime_api (HTTP + SSE)         │
├─────────────────────────────────────┤
│     RuntimeThreadManager → Engine    │
│     ├─ ToolRegistry                 │
│     │   ├─ Shell / File / Git ...   │
│     │   ├─ load_skill  ★            │  ← 加载 SKILL.md 指令
│     │   └─ rlm (Python REPL)  ★     │  ← 执行 python-pptx 等库
│     └─ SkillRegistry                │
│         ├─ skill-creator (内置)      │
│         ├─ pptx-generator (新增)    │
│         ├─ pdf-extractor (新增)     │
│         └─ ...                      │
└─────────────────────────────────────┘
```

### 13.2 三种实现深度

#### 深度一：纯 Skill（零 Rust 代码，最快见效）

只写 `SKILL.md` 指令文件。模型通过已有工具（Shell + RLM Python REPL）按照指令执行。

```
~/.deepseek/skills/pptx-generator/
├── SKILL.md              ← 指令：如何用 python-pptx 生成 PPT
├── references/
│   ├── layouts.md        ← 预定义布局模板
│   └── color_schemes.md  ← 配色方案
├── scripts/
│   └── create_slide.py   ← 可复用的幻灯片创建脚本
└── requirements.txt      ← python-pptx, Pillow
```

**优势**：零 Rust 代码，半天搞定  
**局限**：依赖模型对 python-pptx API 的熟悉度

#### 深度二：Skill + 专用 Rust Tool（质量可控）

当纯 Skill 的生成质量不够稳定时，新增 Rust 工具。注意：新工具在 `crates/tui/src/tools/` 下注册，**不改动 `deepseek-core` 和 `deepseek-tools`**。

```rust
// crates/tui/src/tools/pptx.rs (新增)
#[async_trait]
impl ToolSpec for GeneratePptxTool {
    fn name(&self) -> &'static str { "generate_pptx" }
    fn description(&self) -> &'static str {
        "Generate a PowerPoint file from a structured slide specification."
    }
    fn input_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "output_path": { "type": "string" },
                "theme": { "enum": ["business", "academic", "minimal", "creative"] },
                "slides": { "type": "array", "items": { ... } }
            },
            "required": ["output_path", "slides"]
        })
    }
}
```

#### 深度三：完整文档处理 Skills 矩阵

```
~/.deepseek/skills/
├── pptx-generator/           ← PPT 生成
├── pdf-extractor/            ← PDF 提取
├── xlsx-processor/           ← Excel 处理
├── markdown-converter/       ← Markdown ↔ Word ↔ PDF
├── diagram-generator/        ← Mermaid / PlantUML 图表
└── document-qa/              ← 本地文档问答（RAG）
```

### 13.3 实施计划

| 阶段 | 内容 | 时间 | 前置 |
|------|------|------|------|
| **Phase 6a** | `pptx-generator` 纯 Skill | 0.5 周 | Phase 5 |
| **Phase 6b** | `pdf-extractor` + `xlsx-processor` Skill | 0.5 周 | Phase 6a |
| **Phase 6c** | `diagram-generator` + `markdown-converter` Skill | 0.5 周 | Phase 6b |
| **Phase 6d** | 评估质量 → 决定升级到深度二 | 0.5 周 | Phase 6c |
| **Phase 6e** | 对质量不足的类型新增 Rust Tool（tui crate 内） | 1 周 | Phase 6d |
| **Phase 6f** | `document-qa` 本地 RAG | 2 周 | Phase 6e |

> **注**：Phase 6e 的 Rust Tool 代码在 `crates/tui/src/tools/` 下，不涉及 `deepseek-core`。RAG 涉及 chromadb 嵌入和 embedding API，有额外的许可证和依赖体积考量，建议作为独立 RFC。

### 13.4 整体能力线

```
Phase 5 完成时:
  ┌─────────────────────────────────────┐
  │ DeepSeek Desktop = AI 编码助手      │
  │ ✓ 对话  ✓ 写代码  ✓ 调试  ✓ Git    │
  │ ✓ Shell ✓ LSP    ✓ 审批  ✓ 会话    │
  └─────────────────────────────────────┘

Phase 6 完成后:
  ┌─────────────────────────────────────┐
  │ DeepSeek Desktop = AI 编码 + 知识助手│
  │ ✓ 以上全部                          │
  │ ✓ 生成 PPT    ✓ 提取 PDF           │
  │ ✓ 处理 Excel  ✓ 绘制图表           │
  │ ✓ 文档互转    ✓ 本地文档问答(RAG)   │
  └─────────────────────────────────────┘
```

---

## 十四、总结

下文「新增代码行数」为**量级估计**，待 Phase 2a（审批状态机 + HTTP）落地后，`runtime_threads` / `engine` 改动会显著高于仅做 SSE 适配时的数量；以里程碑结束时的 `git diff --stat` 为准。

| 维度 | 桌面端 (Phase 1–5，含 **Phase 2a** 审批) | 文档 Skills (Phase 6+) |
|------|-------------------|----------------------|
| **引擎对接方式** | runtime_api `/v1/stream` SSE | 复用桌面端引擎 |
| **新增 Rust 代码** | **约 400–800 行量级**（Tauri 壳 + runtime_api；含 Phase 2a 时 **+200～500 行** 仅审批相关，依实现方式浮动） | 约 500–2000 行（`crates/tui/src/tools/`） |
| **core crate 改动** | **0 行**（若以 §2.1 Phase 2 迁核方案则另计） | **0 行** |
| **tui crate 改动** | **约 150–400 行**（SSE/事件；不含 Phase 2a 时取下限）；**含 Phase 2a 再加约 150–350 行** | 约 500–2000 行（新增 Tool） |
| **新增依赖** | Tauri v2 + React + 少量 Web 库 | chromadb, python-pptx, openpyxl 等（RLM 侧） |
| **二进制体积增加** | ~5MB（Tauri 壳）+ deepseek-tui（~20MB sidecar） | ~2–5MB（chromadb 等，若启用） |
| **开发周期** | **约 8–10 周**（单人全职；含 Phase 2a 约 **+1～2 周** 相对原 7–8 周估算） | 约 4–6 周（单人全职） |
| **跨平台** | Windows / macOS / Linux 全支持 | 全部复用桌面端平台支持 |