# Zagens API 设计文档

> **Zagens 壳版本:** 0.4.3（`crates/desktop/Cargo.toml`）| **文档修订:** 2026-05-26 | **权威实现:** `commands.rs`、`runtime_proxy.rs`、`runtime_api/router.rs` `build_router`、`web-ui/src/api/client.ts`

本文档描述 **Zagens 桌面壳** 的双通道集成 API。协议类型见 `crates/protocol/`；HTTP 路由 SSOT 为 `crates/runtime-server/src/runtime_api/router.rs`（sidecar：**`deepseek-runtime`**）。

**OpenAPI 3.1（D8）：** 检入契约 [`openapi/zagens-runtime-v1.openapi.json`](./openapi/zagens-runtime-v1.openapi.json)；由 `export-runtime-openapi` 从 Rust `schemars` + 路径表导出；`web-ui` TS 经 `openapi-typescript` 生成（见 [`adr/D8_OPENAPI_TS_GENERATION.md`](./adr/D8_OPENAPI_TS_GENERATION.md)）。历史 `RUNTIME_API.md` 已移除。运行时架构附图见 [RUNTIME_ARCHITECTURE.md](./RUNTIME_ARCHITECTURE.md)。

**与 Agent 行为：** 同一 sidecar 载入的 runtime prompt 含 [幻觉防控子规则](prompt-hallucination-patch.md)（Capability / Architecture Claims）。回归显示 Zagens 在「能力/架构类裸问」上幻觉率较未打 patch 的发行 TUI 明显下降；**本文档的 SSE 示意图与能力结论无关**，集成时以代码与 `streamNormalize.ts` 为准。

> **唯一生产 HTTP 运行时：** Zagens 与 headless 脚本使用 `crates/runtime-server/src/runtime_api/`（`/v1/*`），binary 为 **`deepseek-runtime`**。~~`crates/app-server`~~ **已于 D7 删除**；~~`deepseek-tui` / `deepseek serve`~~ **已于 D6 Phase B 删除**（见 [`adr/D4_APPSERVER_DEPRECATED.md`](./adr/D4_APPSERVER_DEPRECATED.md) · [`adr/D6_PHASE_B_CLI_SUNSET.md`](./adr/D6_PHASE_B_CLI_SUNSET.md)）。

---

## 1. 架构概览

Zagens 采用 **双通道 API** 架构：WebView 前端与 Rust 后端之间通过两条路径通信。

```
┌────────────────────────────────────────────────────────────┐
│                    Zagens (Tauri App)                      │
│                                                            │
│  ┌─────────────────────┐    ┌────────────────────────┐     │
│  │   WebView (React)   │    │    Rust Shell          │     │
│  │                     │    │                        │     │
│  │  Channel A:         │    │  commands.rs           │     │
│  │  Tauri invoke() ────┼───►│  runtime_proxy.rs      │     │
│  │  (原生桥接, ~41 命令)│    │  sidecar.rs            │     │
│  │                     │    │  main.rs               │     │
│  │  Channel B:         │    │  ┌──────────────────┐  │     │
│  │  经 proxy 或 dev    │    │  │  Sidecar Process │  │     │
│  │  fetch (127.0.0.1)  │───►│  │  deepseek-tui    │  │     │
│  │  56 条 HTTP 路由*   │    │  │  (HTTP + SSE)    │  │     │
│  └─────────────────────┘    │  └──────────────────┘  │     │
│                             └────────────────────────┘     │
└────────────────────────────────────────────────────────────┘
```

| 通道 | 载体 | 用途 |
|------|------|------|
| **A — Tauri IPC** | `invoke()` → Rust `#[tauri::command]` | 系统级操作：密钥、设置、二进制 I/O、**runtime HTTP/SSE 代理（H06）**、终端、多窗口 |
| **B — Runtime HTTP** | Tauri 下经 `runtime_proxy` 转发；Vite dev 直连 `http://127.0.0.1:{port}` | Agent 运行时：对话线程、会话、流式响应、MCP、任务、用量 |

\* §8「Channel B」表按 **方法 + 路径** 逐条计数共 56 行；与「资源组」数量不同。

**与 CLI / `app-server`：** Zagens **不** 启动 `crates/app-server`；桌面仅 spawn 捆绑的 `deepseek-tui` sidecar，HTTP 面即 `runtime_api/`。`app-server` 供其他入口（若有）参考，非 Zagens 数据路径。

---

## 2. Tauri IPC 命令 (Channel A)

**协议**: JSON-RPC (Tauri 内置)，通过 `@tauri-apps/api/core` 的 `invoke()` 调用。
**认证**: 无需额外认证 — 进程内方法调用，安全由 Tauri 沙箱保证。

### 2.1 运行时连接

| 命令 | 参数 | 返回 | 说明 |
|------|------|------|------|
| `get_runtime_port` | — | `u16` | 获取 sidecar 监听端口（默认 7878） |
| `runtime_http` | `request: { method, path, body? }` | `{ status, body }` | **H06 代理：** REST `/v1/*`；Rust 侧注入 Bearer |
| `runtime_post_stream` | `body: String` | `()` + SSE 事件 | **H06 代理：** `POST /v1/stream`；事件经 Tauri 推回 WebView |
| `runtime_get_sse` | `path: String` | `()` + SSE 事件 | **H06 代理：** `GET …/events` 等 SSE |
| `runtime_cancel_sse` | — | `()` | 取消进行中的 `runtime_get_sse` |
| `get_platform_info` | — | `PlatformInfo` | `{ os, arch, version }` — 其中 `version` 为 **Zagens 壳** SemVer（`CARGO_PKG_VERSION`），非操作系统版本 |
| `get_os_theme` | — | `String` | **占位：** 当前固定 `"dark"`，未读系统主题 API |
| `get_locale` | — | `String` | **占位：** 当前固定 `"zh-CN"`，未读系统 locale |
| `restart_sidecar` | — | `()` | 触发 sidecar 进程重启（重载 config.toml） |

#### 2.1.1 取消 / 打断两层契约（D9）

桌面 **Stop** / Escape 必须同时执行两层；只断流不会停止 runtime turn loop。

| 层 | 机制 | 作用 | 不做的后果 |
|----|------|------|------------|
| **Layer 1** | `AbortSignal` + IPC `runtime_cancel_sse` | 断开 WebView ↔ sidecar 的 SSE / poll 管道 | turn 仍在跑；LLM / 工具继续 |
| **Layer 2** | HTTP `POST /v1/threads/{id}/turns/{turn_id}/interrupt` | runtime `Op::Interrupt` → `CancellationToken` | 本地 UI 仍显示 streaming |

**Web UI SSOT：** `crates/desktop/web-ui/src/api/turnControl.ts`

- `disconnectThreadEventStream(signal?)` — Layer 1 only（导航离开、sidecar 重启等）
- `stopThreadTurn({ threadId, turnId, streamControl? })` — **用户 Stop**：Layer 2 再 Layer 1（409 已结束则忽略）

运行时叙事与 broadcast 细节见 [RUNTIME_ARCHITECTURE.md](./RUNTIME_ARCHITECTURE.md) §8。

#### 2.1.2 多窗口 thread 归属（D10）

| IPC | 说明 |
|-----|------|
| `register_window_thread { threadId }` | 当前 WebView 声明对该 thread 的 UI 所有权 |
| `thread_owned_by_window { threadId }` | 查询本窗口是否仍为 owner |

**SSE 过滤：** 非 owner 窗口须忽略 live `GET …/events` / `runtime_get_sse` 增量（`filterThreadStreamEvents` + `windowOwnsThreadForStream` TTL 缓存）。`approval.required` 等同理。历史 replay（加载会话）不受此限。

> **`get_runtime_token` 已移除（2026-05-20 安全跟进 H06）：** Bearer 不再进入 WebView；Tauri 正式路径一律经 `runtime_proxy`。完整 IPC 列表以 `crates/desktop/src/main.rs` `generate_handler!` 为准（2026-05-25 约 **41** 条，含终端与多窗口命令）。

### 2.2 API 密钥管理

| 命令 | 参数 | 返回 | 说明 |
|------|------|------|------|
| `get_api_key_status` | — | `ApiKeyStatus { configured }` | 检查 OS 密钥链中是否已存储 DeepSeek API Key |
| `save_deepseek_api_key` | `key: String` | `()` | 写入 OS 密钥链 → 清除 config.toml 中的明文 → 重启 sidecar |
| `get_vision_bridge_status` | — | `VisionBridgeStatus { configured, base_url, model }` | 视觉转录 bridge 状态 |
| `save_vision_bridge` | `api_key, base_url, model` | `()` | 保存视觉 bridge 配置 → 重启 sidecar |
| `clear_vision_bridge` | — | `()` | 清除视觉 bridge 配置 → 重启 sidecar |
| `vision_transcribe_image` | `data_url: String` | `String` | 调用视觉模型将图片转写为文字描述 |

### 2.3 文件 / 工作区操作

| 命令 | 参数 | 返回 | 说明 |
|------|------|------|------|
| `read_thread_workspace_binary` | `thread_id, relative_path` | `BinaryFileResponse` | 读取线程工作区二进制（见下方结构）；Runtime `workspace/file` **仅 UTF-8 文本** |
| `read_workspace_binary_at_root` | `workspace_root, relative_path` | `BinaryFileResponse` | 同上，无线程上下文 |

**`BinaryFileResponse`**（`commands.rs`）:

```json
{
  "mime_type": "image/png",
  "base64": "...",
  "size": 12345,
  "truncated": false
}
```
| `open_in_shell` | `path: String` | `()` | 在系统终端中打开指定路径 |
| `open_with_system_app` | `path: String` | `()` | 用系统默认应用打开文件 |
| `export_thread_json` | `thread_id: String` | 保存文件对话框 | 导出线程对话记录为 JSON |
| `export_session_json` | `session_id: String` | 保存文件对话框 | 导出会话记录为 JSON |

### 2.4 系统设置

| 命令 | 参数 | 返回 | 说明 |
|------|------|------|------|
| `get_system_settings` | — | `SystemSettings` | 读取完整系统设置 |
| `save_system_settings` | `settings: SystemSettings` | `()` | 写入设置 → 重启 sidecar |

**`SystemSettings` 结构**:

```
default_model: string      reasoning_effort: string      cost_currency: string
allow_shell: bool           approval_policy: string        sandbox_mode: string
max_subagents: number       web_search: bool               subagents_enabled: bool
exec_policy: bool           memory_enabled: bool            lsp_enabled: bool
snapshots_enabled: bool     notify_method: string           session_file_mb: number
```

### 2.5 项目规则 & 符号索引

| 命令 | 参数 | 返回 | 说明 |
|------|------|------|------|
| `read_pick_rules` | `workspace_root: String` | `String` | 读取 `.deepseek/pick-rules.md`（不存在则返回空） |
| `save_pick_rules` | `workspace_root, content` | `()` | 写入项目规则文件 |
| `rebuild_symbol_index` | `workspace: String` | `()` | HTTP `POST /v1/symbol-index/rebuild?workspace=…`（**query**，无 JSON body） |
| `get_symbol_index_info` | `workspace: String` | `SymbolIndexInfo` | **仅 IPC**：读 `.deepseek/symbols.json`（status/size/count，含 stale 检测） |
| `delete_symbol_index` | `workspace: String` | `()` | 删除工作区 `.deepseek/symbols.json` 等索引文件 |

---

## 3. Runtime HTTP API (Channel B)

**服务端**: sidecar 执行 `deepseek serve --http --host 127.0.0.1 --port {port}`（见 `sidecar.rs`）
**监听地址**: `http://127.0.0.1:{port}`（默认 **7878**，以 `get_runtime_port` 为准）
**Sidecar 环境**: `DEEPSEEK_RUNTIME_TOKEN`、`DEEPSEEK_CLIENT_SURFACE=zagens`、可选 `DEEPSEEK_API_KEY`（密钥链）
**CORS**: sidecar 允许 `http(s)://tauri.localhost`（供 WebView 跨域访问）
**序列化**: JSON (请求/响应体)
**流式协议**: SSE (Server-Sent Events) — 用于 `/v1/stream` 和 `/v1/threads/{id}/events`

### 3.1 认证

当 `RuntimeApiState.runtime_token` 已配置时，所有 `/v1/*` 路由经中间件校验，任选其一：

```http
Authorization: Bearer <runtime_token>
```

或

```http
x-deepseek-runtime-token: <runtime_token>
```

`runtime_token` 由 Tauri `main.rs` 生成 UUID v4，经环境变量 `DEEPSEEK_RUNTIME_TOKEN` 传给 sidecar。**WebView 不持有 token** — Tauri 正式路径经 `runtime_proxy.rs` 在 Rust 侧注入 `Authorization: Bearer …`（H06）。`client.ts` 在 `initRuntimeConfig()` 后设 `useTauriRuntimeProxy = true`，REST 走 `runtime_http`，流式走 `runtime_post_stream` / `runtime_get_sse`。

Vite 纯前端 dev（非 Tauri）可直连 sidecar，此时无 Bearer（仅当 sidecar 未配置 token 时可用）。

若 **未** 配置 `runtime_token`（部分测试/开发路径），中间件放行全部 `/v1/*` — 桌面正式路径始终带 token。

公开端点（无需认证）：
- `GET /health`
- `GET /internal/probe`

### 3.2 通用约定

- **所有请求体**: `Content-Type: application/json`
- **重试策略**: WebView 端对瞬态网络错误（`TypeError: Failed to fetch`、`NetworkError`）执行指数退避重试（5 次，基础延迟 350ms）
- **运行时就绪探测**: 启动时轮询 `GET /health`（token 就绪后再测 `GET /v1/sessions`），最长 90s
- **有 body 的 POST/PATCH**: `Content-Type: application/json`；**例外：** `symbol-index/rebuild` 仅用 query（见 §3.3.11）

### 3.2.1 SSE 稳定事件子集 v1（`event:` 字段）

> **状态：正式发布 · `event_schema_version` = 2**
>
> `POST /v1/stream` 与 `GET /v1/threads/{id}/events` 均输出 SSE。
> 每个 `RuntimeEventRecord` 携带 `schema_version: 2`；客户端 SHOULD 忽略
> `schema_version` 大于已知最大值的记录。下表为 **穷举** 的稳定事件子集；
> 实现见 `map_compat_stream_event()`（`crates/tui/src/runtime_api/stream.rs`）。

| SSE `event:` | 来源 | 含义 |
|-------------|------|------|
| `turn.started` | `stream_turn`（首帧） | 新轮次开始，含 `thread_id` / `turn_id` / `model` / `mode` / `workspace` |
| `thinking.delta` | `item.delta`（thinking） | 思考/推理增量，`data: { content }` |
| `message.delta` | `item.delta`（agent） | 助手文本增量，`data: { content }` |
| `tool.progress` | `item.delta`（tool） | 工具输出流，`data: { output }` |
| `tool.started` | `item.started`（tool） | 工具调用开始，`data: { id, name, input }` |
| `tool.completed` | `item.completed` / `item.failed` | 工具调用结束，`data: { id, success, output }` |
| `status` | `item.completed`（status） | 状态/通知，`data: { message }` |
| `error` | `item.completed`（error） | 错误消息，`data: { message }` |
| `approval.required` | `approval.required` | 需用户审批工具执行，`data: { id, tool_name, description, … }` |
| `sandbox.denied` | `sandbox.denied` | 沙箱策略拒绝 |
| `turn.completed` | `turn.completed` | 轮次结束，`data: { usage, turn_summary? }` |
| `agent.spawned` | `agent.spawned` | 子代理启动，`data: { agent_id, prompt? }` |
| `agent.progress` | `agent.progress` | 子代理状态更新，`data: { agent_id, status }` |
| `agent.completed` | `agent.completed` | 子代理完成，`data: { agent_id, result }` |
| `agent.list` | `agent.list` | 子代理列表，`data: { agents }` |
| `craft.verdict` | `craft.verdict` | CRAFT 结构化裁决，`data: { agent_id, agent_type, task_id?, verdict, summary?, items }` |
| `craft.board_updated` | `craft.board_updated` | CRAFT 黑板分区更新，`data: { task_id, partition, agent_id }` |
| `panel.checklist` | `panel.checklist` | Checklist 面板数据 |
| `panel.scratchpad` | `panel.scratchpad` | Scratchpad 面板数据 |
| `panel.context` | `panel.context` | Context 面板数据 |
| `done` | `stream_turn`（末帧） | 流结束，无 data |

**`turn_summary` 子对象**（仅 `turn.completed`，可选）：
```json
{
  "step_count": 3,
  "tool_names": ["read_file", "grep_files"],
  "end_reason": null
}
```

**客户端指引：**
- 未知事件名 SHOULD 被忽略（向前兼容）。
- `schema_version` 递增时，新字段为增量添加；客户端 MUST NOT 因为未知字段而失败。
- WebView 在 `streamNormalize.ts` 归一化为 UI 事件。

---

### 3.3 端点全景

#### 3.3.1 健康检查

| 方法 | 路径 | 认证 | 说明 |
|------|------|------|------|
| GET | `/health` | 否 | 运行时存活检查 |

**`GET /health` 响应**:

```json
{
  "status": "ok",
  "service": "deepseek-runtime-api",
  "mode": "local",
  "event_schema_version": 2
}
```

| GET | `/internal/probe` | 否 | 内部就绪 / 排障（PID、启动时间、token 指纹、sidecar 版本） |

**`GET /internal/probe` 响应**（摘要）:

```json
{
  "status": "ok",
  "pid": 12345,
  "started_at_ms": 1710000000000,
  "token_fingerprint": "...",
  "version": "0.8.15"
}
```

（`version` 为 **deepseek-tui / runtime crate** 版本，非 Zagens 壳版本。）

#### 3.3.2 流式对话

| 方法 | 路径 | 说明 |
|------|------|------|
| POST | `/v1/stream` | **核心流式端点** — 创建匿名线程并启动一次对话轮次，SSE 流式返回 |

**请求体** (`StreamTurnRequest`):

```json
{
  "prompt": "用户的输入文本",
  "workspace": "/path/to/project",
  "mode": "agent",
  "model": "deepseek-v4-pro",
  "auto_approve": false,
  "trust_mode": false,
  "allow_shell": false,
  "route_intent": "coding"
}
```

**SSE 事件流**: 每个事件为 `event:` + `data:`（多为 JSON）块，以 `\n\n` 分隔。事件名见 §3.2.1。

**对话双路径:**

| 路径 | 适用 |
|------|------|
| `POST /v1/stream` | 快速单轮（匿名线程 + 一轮 turn） |
| `POST /v1/threads` + `…/turns` + `GET …/events` | 持久线程、多轮、审批/中断 |

#### 3.3.3 会话 (Sessions)

| 方法 | 路径 | 说明 |
|------|------|------|
| GET | `/v1/sessions` | 列出所有会话 |
| GET | `/v1/sessions/{id}` | 获取会话详情（含消息历史、系统提示） |
| DELETE | `/v1/sessions/{id}` | 删除会话（返回 200 或 204） |
| POST | `/v1/sessions/{id}/resume-thread` | 从存档会话恢复为运行时线程 |

**`GET /v1/sessions/{id}` 响应** (`SessionDetail`):

```json
{
  "metadata": { "id": "...", "name": "...", "title": "...", "created_at": 0, "updated_at": 0 },
  "messages": [
    { "role": "user", "content": [{ "type": "text", "text": "..." }] }
  ],
  "system_prompt": "..."
}
```

#### 3.3.4 线程 (Threads) — 核心对话单元

| 方法 | 路径 | 说明 |
|------|------|------|
| GET | `/v1/threads` | 列出所有线程 |
| POST | `/v1/threads` | 创建新线程 |
| GET | `/v1/threads/summary` | 线程摘要列表 |
| GET | `/v1/threads/{id}` | 获取线程详情 + `latest_seq` |
| PATCH | `/v1/threads/{id}` | 更新线程属性 |
| POST | `/v1/threads/{id}/resume` | 恢复线程继续对话 |
| POST | `/v1/threads/{id}/fork` | 分叉线程 |
| POST | `/v1/threads/{id}/compact` | 压缩线程上下文 |
| POST | `/v1/threads/{id}/turns` | 在此线程中启动一次对话轮次 |
| POST | `/v1/threads/{id}/turns/{turn_id}/steer` | 引导对话方向 |
| POST | `/v1/threads/{id}/turns/{turn_id}/resolve-approval` | 处理工具审批（批准/拒绝） |
| POST | `/v1/threads/{id}/turns/{turn_id}/interrupt` | 中断正在执行的轮次 |
| GET | `/v1/threads/{id}/checklist` | 获取线程关联的 checklist |
| GET | `/v1/threads/{id}/events` | 订阅线程事件流 (SSE) |
| POST | `/v1/threads/{id}/persist-session` | 将线程持久化为可存档会话 |
| GET | `/v1/threads/{id}/snapshots` | 列出线程的 git 快照（`?limit=N`） |
| POST | `/v1/threads/{id}/snapshots/restore` | 恢复到指定快照 (`{ "n": N }`) |

**`PATCH /v1/threads/{id}` 请求体** (`PatchThreadBody`):

```json
{
  "archived": false,
  "allow_shell": true,
  "trust_mode": false,
  "auto_approve": true,
  "model": "deepseek-v4-flash",
  "mode": "agent",
  "title": "重构任务",
  "system_prompt": "你是 Rust 专家",
  "workspace": "/path/to/workspace"
}
```

**`POST /v1/threads/{id}/turns` 请求体**:

```json
{
  "prompt": "请修复 clippy 警告",
  "model": "deepseek-v4-pro",
  "mode": "agent",
  "allow_shell": true,
  "trust_mode": false,
  "auto_approve": false,
  "route_intent": "coding"
}
```

**`POST /v1/threads/{id}/turns/{turn_id}/resolve-approval` 请求体**:

```json
{
  "tool_call_id": "...",
  "decision": "approve"
}
```

##### 线程工作区浏览

| 方法 | 路径 | 说明 |
|------|------|------|
| GET | `/v1/threads/{id}/workspace/browse` | 列出线程工作区目录 (`?path=`) |
| GET | `/v1/threads/{id}/workspace/file` | 读取线程工作区文件 (`?path=`) |

##### Composer 工作区浏览（无线程上下文）

| 方法 | 路径 | 说明 |
|------|------|------|
| GET | `/v1/workspace/browse` | 列出任意工作区目录 (`?workspace=&path=`) |
| GET | `/v1/workspace/file` | 读取任意工作区文件 (`?workspace=&path=`) |
| GET | `/v1/workspace/status` | 获取工作区状态 |

**`GET ../workspace/browse` 响应**:

```json
{
  "workspace": "/path/to/root",
  "path": "src",
  "entries": [
    { "name": "main.rs", "kind": "file", "size": 12400 },
    { "name": "components", "kind": "dir" }
  ]
}
```

**`GET ../workspace/file` 响应**:

```json
{
  "path": "src/main.rs",
  "content": "fn main() { ... }",
  "truncated": false,
  "language_hint": "rust"
}
```

#### 3.3.5 任务 (Tasks) — 持久化后台工作

| 方法 | 路径 | 说明 |
|------|------|------|
| GET | `/v1/tasks` | 列出所有任务 |
| POST | `/v1/tasks` | 创建任务 |
| GET | `/v1/tasks/{id}` | 获取任务详情 |
| POST | `/v1/tasks/{id}/cancel` | 取消任务 |
| GET | `/v1/resume-tasks/{thread_id}` | 获取线程关联的恢复任务 |

#### 3.3.6 Skills（技能）

| 方法 | 路径 | 说明 |
|------|------|------|
| GET | `/v1/skills` | 列出所有技能 |
| POST | `/v1/skills` | 创建新技能 |

#### 3.3.7 自动化 (Automations)

| 方法 | 路径 | 说明 |
|------|------|------|
| GET | `/v1/automations` | 列出所有自动化规则 |
| POST | `/v1/automations` | 创建自动化规则 |
| GET | `/v1/automations/{id}` | 获取单个自动化 |
| PATCH | `/v1/automations/{id}` | 更新自动化 |
| DELETE | `/v1/automations/{id}` | 删除自动化 |
| POST | `/v1/automations/{id}/run` | 运行自动化 |
| POST | `/v1/automations/{id}/pause` | 暂停自动化 |
| POST | `/v1/automations/{id}/resume` | 恢复自动化 |
| GET | `/v1/automations/{id}/runs` | 列出自动化执行历史 |

#### 3.3.8 MCP (Model Context Protocol) 服务器

| 方法 | 路径 | 说明 |
|------|------|------|
| GET | `/v1/apps/mcp/servers` | 列出所有 MCP 服务器 |
| POST | `/v1/apps/mcp/servers` | 添加 MCP 服务器 |
| GET | `/v1/apps/mcp/servers/{name}` | 获取单个 MCP 服务器配置 |
| PUT | `/v1/apps/mcp/servers/{name}` | 更新 MCP 服务器配置 |
| DELETE | `/v1/apps/mcp/servers/{name}` | 删除 MCP 服务器 |
| GET | `/v1/apps/mcp/tools` | 列出 MCP 工具 (`?server=`) |
| POST | `/v1/apps/mcp/config/merge` | 合并 MCP 配置 JSON 到 `mcp.json` |

**`POST /v1/apps/mcp/servers` 请求体**:

```json
{
  "name": "my-server",
  "command": "npx",
  "args": ["-y", "@modelcontextprotocol/server-filesystem", "/tmp"],
  "url": null
}
```

#### 3.3.9 路由规则 (Routing)

| 方法 | 路径 | 说明 |
|------|------|------|
| GET | `/v1/apps/routing/rules` | 获取模型路由规则 |
| PUT | `/v1/apps/routing/rules` | 设置路由规则；请求体 `{ "rules": [ ... ] }` |

#### 3.3.10 用量统计 (Usage)

| 方法 | 路径 | 说明 |
|------|------|------|
| GET | `/v1/usage` | 获取用量统计 |
| `?since=&until=&group_by=` | | 支持按模型/日期聚合 |

#### 3.3.11 符号索引 (Symbol Index)

| 方法 | 路径 | 认证 | 说明 |
|------|------|------|------|
| POST | `/v1/symbol-index/rebuild?workspace=/abs/path` | Bearer | 重建索引；**仅 query 参数** `workspace`，无请求体 |

成功响应示例（JSON）:

```json
{
  "status": "ok",
  "path": "/path/to/.deepseek/symbols.json",
  "symbol_count": 42
}
```

索引 **状态/大小/是否 stale** 走 IPC `get_symbol_index_info`，无对应 GET HTTP 路由。

---

## 4. 认证流程（H06）

```
┌──────────┐          ┌──────────┐          ┌──────────────┐
│  Tauri   │          │  WebView │          │   Sidecar    │
│  main.rs │          │   JS     │          │ deepseek-tui │
└────┬─────┘          └────┬─────┘          └──────┬───────┘
     │                     │                       │
     │ ① uuid v4 token     │                       │
     │  (AppState)         │                       │
     │ ② spawn sidecar     │                       │
     │  DEEPSEEK_RUNTIME_  │                       │
     │  TOKEN=zagens       │                       │
     │────────────────────────────────────────────►│
     │                     │                       │
     │                     │ ③ invoke runtime_http │
     │                     │   / runtime_post_     │
     │                     │   stream / runtime_   │
     │                     │   get_sse             │
     │                     │──────────────────────►│
     │  Rust 注入 Bearer   │                       │
     │────────────────────────────────────────────►│
     │                     │      200 / SSE ◄──────│
```

1. Tauri `main.rs` 生成 `uuid v4` token，存入 `AppState`
2. Sidecar 通过 `DEEPSEEK_RUNTIME_TOKEN` 环境变量启动
3. WebView 调用 `runtime_http` / `runtime_post_stream` / `runtime_get_sse`（**不**获取 token）
4. `runtime_proxy.rs` 在 Rust 侧为每个请求注入 `Authorization: Bearer <token>`

**Token 安全:** Bearer **不**进入 WebView、**不**写入 `localStorage` / `window`；见 CHANGELOG 2026-05-20 H06 跟进。

---

## 5. 错误处理

### 5.1 Runtime HTTP 错误格式

失败时响应体多为 **纯文本** 或简短 JSON（视 handler 而定）。WebView **不** 假定固定 schema，统一 `await res.text()` 后构造 `Error`。

WebView 端统一封装为 `Error` 对象，附带 `status` 属性：

```typescript
// client.ts 中的统一模式
if (!res.ok) {
  const text = await res.text();
  const err = new Error(`HTTP ${res.status}: ${text}`);
  (err as Error & { status?: number }).status = res.status;
  throw err;
}
```

### 5.2 瞬态错误重试

`isTransientRuntimeFetchError()` 检测：
- `TypeError` (fetch 失败，sidecar 未就绪)
- `Error` 消息中包含 `Failed to fetch` / `NetworkError` / `Load failed`

检测到瞬态错误时自动重试 5 次（指数退避，基础延迟 350ms）。

### 5.3 Tauri IPC 错误

返回 `Result<T, String>`，Rust 端错误以字符串形式传递，前端捕获为 `Error`。

---

## 6. Sidecar 生命周期

```
┌───────────────┐
│  Tauri 启动    │
└───────┬───────┘
        ▼
┌───────────────────────────────────────┐
│  sidecar.rs: start_and_monitor()       │
│                                        │
│  spawn deepseek serve --http            │
│  + 健康检查轮询                        │
│  + 崩溃自动重启 (退避策略)             │
│  + API Key 变更 → restart notify       │
│  + 日志输出到 sidecar.log /            │
│    supervisor.log                      │
└───────────────────────────────────────┘
```

关键参数：
- **启动探测**: 60 次重试，首次 60ms，之后 200ms，共 ~12s
- **健康检查心跳**: 5s 间隔
- **崩溃重启**: 60s 内 ≥3 次快速崩溃 → 暂停自动重启，通知用户
- **连接拒绝阈值**: 2 次 (立即判定端口死亡)
- **繁忙超时阈值**: 12 次 (进程存活但过载，宽限等待)
- **API Key 变更**: `restart_sidecar` 命令触发重启，450ms 防抖

---

## 7. 数据流示意

### 7.1 发起对话

```
WebView                          Sidecar                    LLM API
  │                                │                          │
  │ POST /v1/stream { prompt, ... }│                          │
  │───────────────────────────────►│                          │
  │                                │ POST /chat/completions   │
  │                                │─────────────────────────►│
  │                                │      SSE streaming ◄────│
  │◄── SSE: thinking.delta ───────│                          │
  │◄── SSE: tool.started ─────────│                          │
  │◄── SSE: message.delta ────────│                          │
  │◄── SSE: done ─────────────────│                          │
```

### 7.2 工具审批

```
WebView                          Sidecar
  │                                │
  │◄── SSE: approval.required ────│  (需要用户审批)
  │                                │
  │ POST …/resolve-approval        │
  │ { tool_call_id, decision }     │
  │───────────────────────────────►│
  │                                │  继续执行工具调用
  │◄── SSE: tool.completed ───────│
```

---

## 8. 端点一览表

### Channel A: Tauri IPC

> 完整列表以 `crates/desktop/src/main.rs` `generate_handler!` 为准（2026-05-25 约 **41** 条）。下表为常用子集；**不含**已移除的 `get_runtime_token`。

| # | 命令名 | 功能分类 |
|---|--------|----------|
| 1 | `get_runtime_port` | 运行时连接 |
| 2 | `runtime_http` | Runtime 代理（REST） |
| 3 | `runtime_post_stream` | Runtime 代理（POST SSE） |
| 4 | `runtime_get_sse` | Runtime 代理（GET SSE） |
| 5 | `runtime_cancel_sse` | Runtime 代理 |
| 6 | `get_platform_info` | 运行时连接 |
| 7 | `get_os_theme` | 运行时连接 |
| 8 | `get_locale` | 运行时连接 |
| 9 | `restart_sidecar` | 运行时连接 |
| 10 | `get_api_key_status` | 密钥管理 |
| 11 | `save_deepseek_api_key` | 密钥管理 |
| 12 | `get_vision_bridge_status` | 密钥管理 |
| 13 | `save_vision_bridge` | 密钥管理 |
| 14 | `clear_vision_bridge` | 密钥管理 |
| 15 | `vision_transcribe_image` | 密钥管理 |
| 16 | `read_thread_workspace_binary` | 文件操作 |
| 17 | `read_workspace_binary_at_root` | 文件操作 |
| 18 | `open_in_shell` | 文件操作 |
| 19 | `open_with_system_app` | 文件操作 |
| 20 | `export_thread_json` | 文件操作 |
| 21 | `export_session_json` | 文件操作 |
| 22 | `get_system_settings` | 系统设置 |
| 23 | `save_system_settings` | 系统设置 |
| 24 | `default_composer_workspace` | 系统设置 |
| 25 | `read_pick_rules` | 项目规则 |
| 26 | `save_pick_rules` | 项目规则 |
| 27 | `rebuild_symbol_index` | 符号索引 |
| 28 | `get_symbol_index_info` | 符号索引 |
| 29 | `delete_symbol_index` | 符号索引 |
| 30–33 | `spawn_terminal` / `write_terminal` / `resize_terminal` / `kill_terminal` | 嵌入式终端 |
| 34–41 | `get_window_label` … `close_current_window` | 多窗口 / Agent 窗 |

### Channel B: Runtime HTTP

| # | 方法 | 路径 | 功能分类 |
|---|------|------|----------|
| 1 | GET | `/health` | 健康检查 |
| 2 | GET | `/internal/probe` | 健康检查 |
| 3 | POST | `/v1/stream` | 流式对话 |
| 4 | GET | `/v1/sessions` | 会话 |
| 5 | GET | `/v1/sessions/{id}` | 会话 |
| 6 | DELETE | `/v1/sessions/{id}` | 会话 |
| 7 | POST | `/v1/sessions/{id}/resume-thread` | 会话 |
| 8 | GET | `/v1/threads` | 线程 |
| 9 | POST | `/v1/threads` | 线程 |
| 10 | GET | `/v1/threads/summary` | 线程 |
| 11 | GET | `/v1/threads/{id}` | 线程 |
| 12 | PATCH | `/v1/threads/{id}` | 线程 |
| 13 | POST | `/v1/threads/{id}/resume` | 线程 |
| 14 | POST | `/v1/threads/{id}/fork` | 线程 |
| 15 | POST | `/v1/threads/{id}/compact` | 线程 |
| 16 | POST | `/v1/threads/{id}/turns` | 线程 |
| 17 | POST | `/v1/threads/{id}/turns/{turn_id}/steer` | 线程 |
| 18 | POST | `/v1/threads/{id}/turns/{turn_id}/resolve-approval` | 线程 |
| 19 | POST | `/v1/threads/{id}/turns/{turn_id}/interrupt` | 线程 |
| 20 | GET | `/v1/threads/{id}/checklist` | 线程 |
| 21 | GET | `/v1/threads/{id}/events` | 线程 (SSE) |
| 22 | POST | `/v1/threads/{id}/persist-session` | 线程 |
| 23 | GET | `/v1/threads/{id}/snapshots` | 线程 |
| 24 | POST | `/v1/threads/{id}/snapshots/restore` | 线程 |
| 25 | GET | `/v1/threads/{id}/workspace/browse` | 工作区浏览 |
| 26 | GET | `/v1/threads/{id}/workspace/file` | 工作区浏览 |
| 27 | GET | `/v1/workspace/browse` | 工作区浏览 |
| 28 | GET | `/v1/workspace/file` | 工作区浏览 |
| 29 | GET | `/v1/workspace/status` | 工作区浏览 |
| 30 | GET | `/v1/tasks` | 任务 |
| 31 | POST | `/v1/tasks` | 任务 |
| 32 | GET | `/v1/tasks/{id}` | 任务 |
| 33 | POST | `/v1/tasks/{id}/cancel` | 任务 |
| 34 | GET | `/v1/resume-tasks/{thread_id}` | 任务 |
| 35 | GET | `/v1/skills` | 技能 |
| 36 | POST | `/v1/skills` | 技能 |
| 37 | GET | `/v1/automations` | 自动化 |
| 38 | POST | `/v1/automations` | 自动化 |
| 39 | GET | `/v1/automations/{id}` | 自动化 |
| 40 | PATCH | `/v1/automations/{id}` | 自动化 |
| 41 | DELETE | `/v1/automations/{id}` | 自动化 |
| 42 | POST | `/v1/automations/{id}/run` | 自动化 |
| 43 | POST | `/v1/automations/{id}/pause` | 自动化 |
| 44 | POST | `/v1/automations/{id}/resume` | 自动化 |
| 45 | GET | `/v1/automations/{id}/runs` | 自动化 |
| 46 | GET | `/v1/apps/mcp/servers` | MCP |
| 47 | POST | `/v1/apps/mcp/servers` | MCP |
| 48 | GET | `/v1/apps/mcp/servers/{name}` | MCP |
| 49 | PUT | `/v1/apps/mcp/servers/{name}` | MCP |
| 50 | DELETE | `/v1/apps/mcp/servers/{name}` | MCP |
| 51 | GET | `/v1/apps/mcp/tools` | MCP |
| 52 | POST | `/v1/apps/mcp/config/merge` | MCP |
| 53 | GET | `/v1/apps/routing/rules` | 路由 |
| 54 | PUT | `/v1/apps/routing/rules` | 路由 |
| 55 | GET | `/v1/usage` | 用量统计 |
| 56 | POST | `/v1/symbol-index/rebuild` | 符号索引 |

---

## 9. 源文件索引

| 组件 | 文件 | 说明 |
|------|------|------|
| Tauri 命令注册 | `crates/desktop/src/main.rs` | `generate_handler!` — 约 **41** 个 IPC 命令 |
| Tauri 命令实现 | `crates/desktop/src/commands.rs` | 密钥、设置、二进制预览、符号索引 IPC |
| Runtime HTTP 代理 | `crates/desktop/src/runtime_proxy.rs` | H06 — Bearer 注入、SSE 转发 |
| Sidecar 进程管理 | `crates/desktop/src/sidecar.rs` | spawn、`deepseek-runtime`、健康检查、重启 |
| WebView HTTP 客户端 | `crates/desktop/web-ui/src/api/client.ts` | `initRuntimeConfig`、proxy 分支、就绪探测 |
| SSE 归一化 | `crates/desktop/web-ui/src/api/streamNormalize.ts` | wire `event` → UI 事件 |
| Runtime HTTP 路由 | `crates/runtime-server/src/runtime_api/router.rs` | `build_router` |
| 协议类型 | `crates/protocol/src/` | 共享 DTO（若有） |
| Agent prompt（sidecar） | `crates/runtime-server/src/prompts/base.md` 等 | 含幻觉防控子规则；见 [prompt-hallucination-patch.md](prompt-hallucination-patch.md) |

> `crates/app-server/` 为 monorepo 内其他入口所用，**Zagens 桌面路径不经过此 crate**。行数为撰写时约数，以仓库为准。

---

## 10. 相关文档

| 文档 | 内容 |
|------|------|
| [RUNTIME_ARCHITECTURE.md](./RUNTIME_ARCHITECTURE.md) | 三层模型、crate 依赖、双持久化附图 |
| [RUNTIME_EVOLUTION_ROADMAP.md](./RUNTIME_EVOLUTION_ROADMAP.md) | 演进路线图 SSOT §3 现状快照 |
| [prompt-hallucination-patch.md](prompt-hallucination-patch.md) | V4 能力/架构断言防控、Zagens vs 官方 TUI 0.8.39 回归 |
| [tui/回归测试.md](tui/回归测试.md) | 幻觉防控与并行调度回归用例 |
| [agent-reliability-craft-plan.md](agent-reliability-craft-plan.md) | CRAFT、子代理写路径 §3.2 |

---

## 修订记录

| 日期 | 说明 |
|------|------|
| 2026-05-26 | D6 Phase B：sidecar 路径改为 `crates/runtime-server`；移除 `deepseek-tui` / CLI 生产叙事 |
| 2026-05-25 | 对齐代码：Zagens 0.4.3、`runtime_api/router.rs`、H06 代理认证（移除 `get_runtime_token`）、IPC ~41 条、`DEEPSEEK_CLIENT_SURFACE=zagens` |
| 2026-05-18 | 对照代码修正：health/probe、BinaryFileResponse、SSE 事件名、symbol-index query、认证 header、25 IPC / 56 HTTP、app-server 边界 |
| 2026-05-18 | 初版（Zagens API 双通道） |
