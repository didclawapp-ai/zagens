# DS Pick API 设计文档

> 版本: 0.2.1 | 最后更新: 2026-05-18 | 基于仓库实际代码

---

## 1. 架构概览

DS Pick 采用 **双通道 API** 架构：WebView 前端与 Rust 后端之间通过两条路径通信。

```
┌────────────────────────────────────────────────────────────┐
│                    DS Pick (Tauri App)                      │
│                                                            │
│  ┌─────────────────────┐    ┌────────────────────────┐     │
│  │   WebView (React)   │    │    Rust Shell          │     │
│  │                     │    │                        │     │
│  │  Channel A:         │    │  commands.rs           │     │
│  │  Tauri invoke() ────┼───►│  sidecar.rs            │     │
│  │  (原生桥接, 26 命令) │    │  main.rs               │     │
│  │                     │    │                        │     │
│  │  Channel B:         │    │  ┌──────────────────┐  │     │
│  │  HTTP fetch() ──────┼────┼─►│  Sidecar Process │  │     │
│  │  (localhost:7878)   │    │  │  deepseek-tui    │  │     │
│  │  ~45 REST 端点      │    │  │  (HTTP + SSE)    │  │     │
│  └─────────────────────┘    │  └──────────────────┘  │     │
│                             └────────────────────────┘     │
└────────────────────────────────────────────────────────────┘
```

| 通道 | 载体 | 用途 |
|------|------|------|
| **A — Tauri IPC** | `invoke()` → Rust `#[tauri::command]` | 系统级操作：密钥管理、文件对话框、二进制文件 I/O、工作区导出、符号索引管理 |
| **B — Runtime HTTP** | `fetch()` → `http://127.0.0.1:7878` | Agent 运行时：对话线程、会话、流式响应、MCP 管理、任务自动化、用量统计 |

---

## 2. Tauri IPC 命令 (Channel A)

**协议**: JSON-RPC (Tauri 内置)，通过 `@tauri-apps/api/core` 的 `invoke()` 调用。
**认证**: 无需额外认证 — 进程内方法调用，安全由 Tauri 沙箱保证。

### 2.1 运行时连接

| 命令 | 参数 | 返回 | 说明 |
|------|------|------|------|
| `get_runtime_port` | — | `u16` | 获取 sidecar 监听端口（默认 7878） |
| `get_runtime_token` | — | `String` | 获取 Bearer token，WebView 用此 token 访问 Runtime HTTP API |
| `get_platform_info` | — | `PlatformInfo` | 返回 `{ os, arch, version }` |
| `get_os_theme` | — | `String` | 固定返回 `"dark"` |
| `get_locale` | — | `String` | 固定返回 `"zh-CN"` |
| `restart_sidecar` | — | `()` | 触发 sidecar 进程重启（重载 config.toml） |

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
| `read_thread_workspace_binary` | `thread_id, relative_path` | `BinaryFileResponse { data, mime }` | 读取线程工作区中的二进制文件（base64 + MIME） |
| `read_workspace_binary_at_root` | `workspace_root, relative_path` | `BinaryFileResponse` | 同上，无线程上下文 |
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
| `rebuild_symbol_index` | `workspace: String` | `()` | 调用 Runtime API `POST /v1/symbol-index/rebuild` |
| `get_symbol_index_info` | `workspace: String` | `SymbolIndexInfo` | 获取索引状态、大小、符号数等 |
| `delete_symbol_index` | `workspace: String` | `()` | 删除工作区的符号索引 |

---

## 3. Runtime HTTP API (Channel B)

**服务端**: `deepseek-tui serve --http` (sidecar 进程)
**监听地址**: `http://127.0.0.1:7878`
**序列化**: JSON (请求/响应体)
**流式协议**: SSE (Server-Sent Events) — 用于 `/v1/stream` 和 `/v1/threads/{id}/events`

### 3.1 认证

桌面模式下，所有 `/v1/*` 路由经过 Bearer Token 中间件校验：

```
Authorization: Bearer <runtime_token>
```

`runtime_token` 由 Tauri 主进程通过 `get_runtime_token` IPC 命令交付给 WebView。Token 仅存在于 JS 闭包作用域中，不暴露到 `window`。

公开端点（无需认证）：
- `GET /health`
- `GET /internal/probe`

### 3.2 通用约定

- **所有请求体**: `Content-Type: application/json`
- **重试策略**: WebView 端对瞬态网络错误（`TypeError: Failed to fetch`、`NetworkError`）执行指数退避重试（5 次，基础延迟 350ms）
- **运行时就绪探测**: 启动时轮询 `GET /health`（然后 `GET /v1/sessions`），最长 90s

---

### 3.3 端点全景

#### 3.3.1 健康检查

| 方法 | 路径 | 认证 | 说明 |
|------|------|------|------|
| GET | `/health` | 否 | 运行时存活检查，返回 `{ "ok": true }` |
| GET | `/internal/probe` | 否 | 内部就绪探测 |

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

**SSE 事件流**: 每个事件为 `event:` + `data:` 块，以 `\n\n` 分隔。

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
| PUT | `/v1/apps/routing/rules` | 设置路由规则 |
| `{ "rules": [...] }` |

#### 3.3.10 用量统计 (Usage)

| 方法 | 路径 | 说明 |
|------|------|------|
| GET | `/v1/usage` | 获取用量统计 |
| `?since=&until=&group_by=` | | 支持按模型/日期聚合 |

#### 3.3.11 符号索引 (Symbol Index)

| 方法 | 路径 | 认证 | 说明 |
|------|------|------|------|
| POST | `/v1/symbol-index/rebuild` | Bearer | 重建指定工作区的符号索引 |

---

## 4. 认证流程

```
┌──────────┐          ┌──────────┐          ┌──────────────┐
│  Tauri   │          │  WebView │          │   Sidecar    │
│  main.rs │          │   JS     │          │  (deepseek)  │
└────┬─────┘          └────┬─────┘          └──────┬───────┘
     │                     │                       │
     │ ① uuid v4 token     │                       │
     │─────────────────────│                       │
     │ ② spawn sidecar     │                       │
     │  with DEEPSEEK_     │                       │
     │  RUNTIME_TOKEN       │                       │
     │────────────────────────────────────────────►│
     │                     │                       │
     │                     │ ③ invoke get_token    │
     │                     │──────────────────────►│
     │                     │        ◄──────────────│
     │                     │                       │
     │                     │ ④ fetch /v1/sessions  │
     │                     │   Authorization:       │
     │                     │   Bearer <token>      │
     │                     │──────────────────────►│
     │                     │      200 OK ◄─────────│
```

1. Tauri `main.rs` 生成 `uuid v4` token
2. Sidecar 通过环境变量 `DEEPSEEK_RUNTIME_TOKEN` 启动
3. WebView 通过 `get_runtime_token` IPC 命令获取 token
4. 所有 Runtime API 请求携带 `Authorization: Bearer <token>`

**Token 安全**: 仅存储在 JS 闭包作用域中（`client.ts` 模块级变量），不写入 `localStorage`、`window` 全局或任何持久化存储。

---

## 5. 错误处理

### 5.1 Runtime HTTP 错误格式

```json
HTTP 4xx/5xx → Response body: plain text error message
```

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
  │◄── SSE: event=thinking ───────│                          │
  │◄── SSE: event=tool_call ──────│                          │
  │◄── SSE: event=text ───────────│                          │
  │◄── SSE: event=done ───────────│                          │
```

### 7.2 工具审批

```
WebView                          Sidecar
  │                                │
  │◄── SSE: event=approval ───────│  (需要用户审批)
  │                                │
  │ POST /resolve-approval         │
  │ { decision: "approve" }        │
  │───────────────────────────────►│
  │                                │  继续执行工具调用
  │◄── SSE: event=tool_result ────│
```

---

## 8. 端点一览表

### Channel A: Tauri IPC

| # | 命令名 | 功能分类 |
|---|--------|----------|
| 1 | `get_runtime_token` | 运行时连接 |
| 2 | `get_runtime_port` | 运行时连接 |
| 3 | `get_platform_info` | 运行时连接 |
| 4 | `get_os_theme` | 运行时连接 |
| 5 | `get_locale` | 运行时连接 |
| 6 | `restart_sidecar` | 运行时连接 |
| 7 | `get_api_key_status` | 密钥管理 |
| 8 | `save_deepseek_api_key` | 密钥管理 |
| 9 | `get_vision_bridge_status` | 密钥管理 |
| 10 | `save_vision_bridge` | 密钥管理 |
| 11 | `clear_vision_bridge` | 密钥管理 |
| 12 | `vision_transcribe_image` | 密钥管理 |
| 13 | `read_thread_workspace_binary` | 文件操作 |
| 14 | `read_workspace_binary_at_root` | 文件操作 |
| 15 | `open_in_shell` | 文件操作 |
| 16 | `open_with_system_app` | 文件操作 |
| 17 | `export_thread_json` | 文件操作 |
| 18 | `export_session_json` | 文件操作 |
| 19 | `get_system_settings` | 系统设置 |
| 20 | `save_system_settings` | 系统设置 |
| 21 | `read_pick_rules` | 项目规则 |
| 22 | `save_pick_rules` | 项目规则 |
| 23 | `rebuild_symbol_index` | 符号索引 |
| 24 | `get_symbol_index_info` | 符号索引 |
| 25 | `delete_symbol_index` | 符号索引 |

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

| 组件 | 文件 | 行数 |
|------|------|------|
| Tauri 命令定义 | `crates/desktop/src/commands.rs` | 1381 |
| Sidecar 进程管理 | `crates/desktop/src/sidecar.rs` | 741 |
| Tauri 主入口 | `crates/desktop/src/main.rs` | 140 |
| WebView API Client | `crates/desktop/web-ui/src/api/client.ts` | 838 |
| Runtime HTTP 路由 | `crates/tui/src/runtime_api.rs` (build_router) | L586–682 |
| App-Server 路由 | `crates/app-server/src/lib.rs` | 783 |
| Protocol 定义 | `crates/protocol/src/` | — |
