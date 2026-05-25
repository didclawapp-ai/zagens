# Zagens 真多窗口方案

> **状态：** **已结案**（2026-05-21）— M1–M4 + M6 已交付；核心手测 T1–T2、T4 通过。**M5 整阶段延后**，有需求再开迭代。  
> **产品基准：** **Cursor / VS Code 多窗口模型**（市场验证的 IDE 习惯；实现与验收以 §1.4 对照表为准）  
> **范围：** Zagens 桌面壳（`crates/desktop/` + `web-ui/`）— **同一 OS 进程内** 多个原生窗口  
> **非目标：** 多进程多实例抢 `127.0.0.1:7878`（明确不做为默认路径）  
> **相关：** [DESKTOP_IMPLEMENTATION_PLAN.md](DESKTOP_IMPLEMENTATION_PLAN.md)、[DEV_NOTES.md](DEV_NOTES.md)、[TUI_DS_PICK_GAP.md](TUI_DS_PICK_GAP.md)、[SIDECAR_SUPERVISOR_HARDENING_PLAN.md](SIDECAR_SUPERVISOR_HARDENING_PLAN.md)、[docs/tech/API_DESIGN.md](../tech/API_DESIGN.md)

**图例：** ✅ 已有 · 🔶 需改 · ⬜ 未做 · ❌ 本方案不做

---

## 0. 实施总览（维护者更新此表）

| 阶段 | 主题 | 状态 | 目标版本 / 备注 |
|------|------|------|-----------------|
| **M0** | 方案评审、依赖与风险签字 | ✅ | 本文档 |
| **M1** | 窗口生命周期 + 单实例 + 菜单/托盘 | ✅ | `window_registry.rs`、`tauri-plugin-single-instance` |
| **M2** | 事件域隔离（终端、runtime SSE、通知） | ✅ | `terminal.rs` + `runtime_proxy.rs` `emit_to`；Web UI `getCurrentWebviewWindow().listen`（Tauri2 #11379）；每窗 PTY 上限 4 |
| **M3** | 前端：按窗口独立 UI 状态 + 按 thread 的 SSE | ✅ | `streamingThreadIds`、切 session 不 abort |
| **M4** | 审批 / 后台 turn 路由到正确窗口 | ✅ | `register_window_thread` / `thread_owned_by_window` |
| **M5** | 体验打磨（可选） | ⏸ 延后 | 标题/快捷键/每窗 workspace 已随 M1–M3 交付；几何记忆、资源管理器打开见 §7.5 |
| **M6** | 门禁、文档、CHANGELOG | ✅ | 构建 + 文档；§9 核心项 T1–T2、T4 ✅ |

---

## 1. 问题陈述

### 1.1 用户故事

- 用户在**两个物理显示器**或**左右分屏**上同时查看两个仓库：各自对话、文件树、Diff、终端。
- 项目 A 的 Agent **长时间 turn** 在跑时，用户可在项目 B 的窗口里继续编写、审批、看目录，无需「切会话 = 掐断 A」。
- 从资源管理器或 CLI 拖入第二个路径时，应 **新开窗口** 绑定该工作区，而不是覆盖当前窗。
- 习惯 Cursor / VS Code 的用户预期：**File → New Window**、任务栏多个窗口图标、关闭一个窗口不退出整个应用（仍有其它窗或托盘）。

### 1.2 现状（2026-05-21）

| 项 | 状态 | 说明 |
|----|------|------|
| Tauri 窗口声明 | 单窗 | `tauri.conf.json` 仅 `label: "main"` |
| 托盘 / 显示 | 硬编码 `main` | `main.rs` `get_webview_window("main")` |
| Capabilities | 单窗 | `capabilities/default.json` → `"windows": ["main"]` |
| Sidecar | 进程级 1 个 | 固定 port **7878**、`AppContext` 全局一份 |
| Web UI | 单 React 树 | `main.tsx` → 单 `App`；全局 `streaming`、`eventAbortRef` |
| 切 session | 中断 SSE | `handleSelectSession` 内 `eventAbortRef.abort()` |
| 终端 | 进程级 `TerminalManager` | `app.emit("terminal-data")` **广播到所有 WebView** |
| 再开一个 `.exe` | 未防护 | 可能端口冲突；sidecar 监督会杀占用 7878 的进程 |

**结论：** Runtime（`runtime_api` + 多 thread）**可以**承载多项目并行；瓶颈在 **Tauri 壳 + Web UI 单焦点模型**。

### 1.4 Cursor / VS Code 对齐清单（验收尺子）

以下行为是用户从 **Cursor（VS Code 系）** 带来的默认预期；Zagens 按表实现，**M6 手测逐项打勾**。

| # | 用户预期（市场习惯） | Zagens 落点 | 阶段 |
|---|----------------------|--------------|------|
| C1 | **File → New Window** 再开一个完整界面 | 菜单 / TitleBar → `create_agent_window` | M1 |
| C2 | 快捷键 **Ctrl+Shift+N**（Win/Linux）/ **Cmd+Shift+N**（macOS） | 全局快捷键注册 | M5 |
| C3 | **每个窗口绑定一个项目文件夹**（不同 repo 并排） | `primary_workspace` + 侧栏默认过滤 | M3–M5 |
| C4 | 任务栏 / Dock **多个应用窗口图标** | 多个 `WebviewWindow` | M1 |
| C5 | **关一个窗口不退出应用**（其它窗仍在） | 仅最后一窗走 hide-to-tray；多窗时 `destroy` 该窗 | M1 |
| C6 | 再点桌面图标 / 从资源管理器「打开」**不启动第二个进程** | `tauri-plugin-single-instance` → 新窗或聚焦 | M1 |
| C7 | A 项目 Agent 在跑时，B 窗口可正常聊天 / 看文件 | 按 `thread_id` 并行 SSE，切 session 不 abort 他 thread | M3 |
| C8 | 终端、通知、审批 **不串到别的窗口** | `emit_to` + `thread_owner` | M2、M4 |
| C9 | 窗口标题能区分项目（文件夹名） | `set_title("{folder} — Zagens")` | M5 |
| C10 | 共享用户级配置（API Key、主题） | 单进程 `~/.deepseek/config.toml` + keyring | 已有 |

**有意与 Cursor 当前差异（可选，评审可改）：**

| 点 | Cursor 现状（社区反馈） | Zagens 建议 |
|----|-------------------------|--------------|
| 同一文件夹开第二窗 | 部分版本会 **聚焦回已有窗** | **允许**同 workspace 两窗（对标 VS Code）；侧栏用 session 区分即可 |
| Agent 专用浮窗 | Cursor 曾试验 Agent Window，未作为长期主路径 | 不做独立 Agent 浮窗；**每窗 = 完整 Zagens**（聊天 + 工作台） |
| 单屏多 Agent Tab | Cursor 强调同窗 **Ctrl+T** 多 Agent 标签 | 保留；与多窗口 **并存**（窗 = 项目边界，Tab = 同项目多会话） |

**原则：** 不发明新的「多项目」交互；用户 muscle memory 从 Cursor/VS Code 直接迁移。

### 1.5 非目标

- ❌ **多进程多实例**（每个实例一个 sidecar、一个 7878）作为默认产品形态。
- ❌ **单窗口内分栏两个完整 App**（那是 L2「单窗并行」，见对话记录；本方案只做真多窗口）。
- ❌ 多窗口间 **拖拽合并会话**、**跨窗拖放文件**（远期可选）。
- ❌ 每窗口独立 sidecar / 独立 `~/.deepseek` 配置目录。

---

## 2. 设计原则

1. **单进程、单 sidecar、多 WebView** — 与 VS Code / Cursor 一致：一个应用进程，一个 agent runtime，多个窗口共享 HTTP API。
2. **窗口 = 独立 UI 壳层状态** — 每个 `WebviewWindow` 加载同一 `web-ui/dist`，但 React 状态、SSE 订阅、审批焦点、终端会话 **按窗口隔离**。
3. **Thread 是并行单位** — Runtime 已按 `thread_id` 隔离 Engine；桌面层禁止「全 app 一个 `streaming` 布尔」阻塞他窗。
4. **事件必达正确窗口** — Tauri 事件用 `emit_to(window_label, …)`；禁止终端/审批数据广播到所有 WebView。
5. **单实例启动** — 第二次点图标：激活已有进程并 **可选** `new_window(workspace)`，不启动第二个 sidecar。
6. **安全不变** — 继续走 `runtime_proxy` + Bearer；browse/读盘仍经 runtime path guard；不扩大 WebView 直读磁盘范围。

---

## 3. 目标架构

### 3.1 进程与运行时（推荐拓扑）

```
┌─────────────────────────────────────────────────────────────────┐
│  OS 进程：deepseek-desktop（单实例）                               │
│  ┌──────────────┐  ┌──────────────┐  ┌──────────────┐           │
│  │ WebView      │  │ WebView      │  │ WebView      │  …        │
│  │ label:main   │  │ label:win-2  │  │ label:win-3  │           │
│  │ React App    │  │ React App    │  │ React App    │           │
│  └──────┬───────┘  └──────┬───────┘  └──────┬───────┘           │
│         │                 │                 │                    │
│         └─────────────────┼─────────────────┘                    │
│                           │ invoke / emit_to                     │
│  ┌────────────────────────┴──────────────────────────────────┐  │
│  │ Tauri Rust：AppContext (port, token) · WindowRegistry      │  │
│  │            TerminalManager (按 window 分桶)                 │  │
│  │            系统托盘 · 单 sidecar 监督                       │  │
│  └────────────────────────┬──────────────────────────────────┘  │
└───────────────────────────┼─────────────────────────────────────┘
                            │ http://127.0.0.1:7878
                            ▼
              deepseek-tui serve --http (一个子进程)
              RuntimeThreadManager — 多 thread 并行 turn
```

### 3.2 与 Cursor 的对齐点（摘要）

详见 **§1.4** 对照表。架构上：多窗共享 **同一** sidecar + SQLite（等同 VS Code 多窗共享扩展宿主），每窗独立 UI 与 `primary_workspace`。

### 3.3 窗口标识（label 策略）

| 规则 | 说明 |
|------|------|
| 首窗 | 保留 `main`（兼容现有 capabilities、文档、测试） |
| 后续窗 | `pick-{uuid}` 或 `pick-{n}`（实现时二选一；**禁止**用户可见 label 作为业务键以外的持久化主键） |
| 人类可读标题 | `set_title`：`{workspace 末段} — Zagens` 或 session 名 |

**上限：** 建议 **8** 个 Agent 窗口（与 `TerminalManager` 每窗 PTY 上限联动，见 §6.2）。

---

## 4. Rust 壳层改动

### 4.1 新增 `WindowRegistry`（`crates/desktop/src/window_registry.rs`）

进程级 `Mutex<WindowRegistry>`，`app.manage`：

```rust
struct WindowRecord {
    label: String,
    primary_workspace: PathBuf,
    created_at: u64,
    /// 本窗当前关注的 thread_id（用于审批/通知路由）
    focused_thread_id: Option<String>,
}

struct WindowRegistry {
    windows: HashMap<String, WindowRecord>,
    /// thread_id -> window_label（谁发起了活跃 turn / 谁应弹审批）
    thread_owner: HashMap<String, String>,
}
```

**命令（示例名）：**

| 命令 | 行为 |
|------|------|
| `create_agent_window` | `WebviewWindowBuilder::new(app, label).url(...).title(...).build()`；登记 registry；可选 `workspace` 初始路径 |
| `close_agent_window` | 关闭指定 label；清理 registry、`thread_owner`、该窗 PTY |
| `list_agent_windows` | 返回 label + workspace + title，供托盘菜单 |
| `focus_agent_window` | `show` + `set_focus` |
| `register_window_thread` | 前端在 `start_turn` / resume 后上报 `{ window_label, thread_id }` |
| `get_window_label` | 当前 WebView 的 label（前端 bootstrap 用） |

### 4.2 `tauri.conf.json` / capabilities

- 静态窗口表仍保留 `main` 作为模板；**动态窗**在 Tauri 2 下通过 `WebviewWindowBuilder` 创建（不必在 json 里枚举每一个）。
- 新增 capability 或扩展 `default`：
  - `core:window:allow-create`
  - `core:window:allow-close`（已有）
  - `core:webview:allow-create-webview-window`（按 Tauri 2 权限表核对）
  - 动态 label 的 window permission 模式（`pick-*` glob，若 schema 支持）
- `gen/schemas` 在改 permissions 后重新生成（CI / 本地 `cargo build`）。

### 4.3 单实例：`tauri-plugin-single-instance`

- 依赖：`tauri-plugin-single-instance`（版本与 Tauri 2 对齐）。
- 第二次启动：解析 argv 中的 workspace 路径（可选约定 `--workspace` 或 positional）。
- 行为：
  1. 若已有进程：发送 IPC / 直接调用 `create_agent_window` + `focus`；
  2. **不**再 spawn sidecar。
- 与安装包「固定协议 URL」可后续再接（本阶段 ⬜）。

### 4.4 托盘与菜单

| 项 | 现状 | 目标 |
|----|------|------|
| 左键托盘 | 显示 `main` | 显示 **最近聚焦** 的窗口 |
| 菜单「显示 Zagens」 | 同上 | 同上；无窗时创建 `main` |
| 新增 | — | **「新建窗口」**、**「窗口列表」** 子菜单（≤8 项） |
| 退出 | 杀 sidecar + `exit(0)` | 不变；关所有窗后仍可从托盘退出 |

### 4.5 `TerminalManager` 分窗

| 项 | 现状 | 目标 |
|----|------|------|
| 会话 Map | 全局 `HashMap<id, LiveSession>` | 增加 `window_label` 字段；或 `HashMap<WindowLabel, HashMap<TermId, …>>` |
| `spawn_terminal` | `app.emit(...)` | `app.emit_to(window_label, "terminal-data", …)` |
| 上限 | 全局 `MAX_SESSIONS = 6` | **每窗** 6（或全局 6、每窗 2 — 产品定；推荐 **每窗 4，全局 ≤16**） |
| `kill_terminal` | 全局 | 仅杀本窗 PTY；关窗时批量清理 |

实现要点：`spawn_terminal` 增加参数 `window_label: String`（或由 command 从 `WebviewWindow` 上下文解析当前 label）。

### 4.6 Sidecar / `AppContext`

- **不**为每窗口起 sidecar；`runtime_port` / `runtime_token` 仍进程级单例。
- `restart_sidecar` / `quit` 影响**所有**窗口（托盘退出、设置里重启 runtime）— UI 需全窗 toast「Runtime 正在重启」。
- 健康检查失败：所有 WebView 的 `waitForRuntimeReady` 各自重试（已有逻辑可复用）。

### 4.7 关闭语义

| 场景 | 行为 |
|------|------|
| 用户点某窗关闭 | 若 registry 内窗数 > 1：**销毁该 WebView**；若仅剩 1 窗：保持现有 **hide 到托盘**（与 `on_window_event` 一致） |
| 用户托盘退出 | `shutdown.notify` → 关 sidecar → `exit(0)` |
| 某窗有 `streaming` turn | 不阻止关窗；turn 在 runtime **继续**；通知其它窗或系统通知（§5.4） |

---

## 5. Web UI 改动

### 5.1 Bootstrap

`main.tsx` / `initRuntimeConfig` 扩展：

1. `invoke('get_window_label')` → 存入 `windowSessionStorage` 或模块级 `currentWindowLabel`。
2. 仍 `get_runtime_port()` — 各窗相同 port。
3. 可选：`invoke('get_window_workspace')` — 新建窗时 Rust 注入的初始工作区。

每个 WebView **独立** `ReactDOM.createRoot` → 独立 `App` 实例（天然隔离 `useState`）。

### 5.2 并行 streaming 模型（核心）

**删除**「全 app 单 `streaming`」假设：

| 现状 | 目标 |
|------|------|
| `const [streaming, setStreaming] = useState(false)` | `streamingThreadIds: Set<string>` 或 `Map<threadId, AbortController>` |
| `handleSend`: `if (streaming) return` | 仅当 **本窗** `resumedThreadId` 已在 streaming 集合中时禁用发送（或允许 steer，按产品） |
| `handleSelectSession`: `eventAbortRef.abort()` | **仅 abort 本窗** 对该 thread 的 UI 订阅；**不**调用 runtime cancel，除非用户点 Stop |
| Stop | 针对本窗当前 thread 的 controller |

**SSE 订阅：**

- 每个窗维护 `Map<threadId, { abort, lastSeq }>`。
- 切走 session 时：保留后台 thread 的 SSE（或改由 `GET /v1/threads/{id}/events?since_seq=` 轮询 + 通知），实现二选一：
  - **方案 A（推荐）：** 本窗对「非活跃但本窗发起且未完成」的 thread 保持轻量 SSE，事件只更新侧栏 badge / 系统通知，不刷当前 ChatView。
  - **方案 B：** 离窗 thread 仅靠 runtime 持久化 + 切回时 `rebuildMessagesFromThreadEvents`（实现简单，但无实时 badge）。

### 5.3 会话列表与工作区

| 策略 | 说明 |
|------|------|
| **默认（Cursor 式）** | 侧栏 session 列表过滤：`thread.workspace` 与窗口 `primary_workspace` 相同（规范化路径后比较） |
| 「显示全部会话」 | 开关；列出其它工作区 session 时显示路径徽章 |
| 新建会话 | 默认绑定本窗 `primary_workspace` |
| Composer 改工作区 | 仅影响本窗；写入本窗 `localStorage` 键：`deepseek-desktop-workspace:{label}` |

避免继续用全局单键 `deepseek-desktop-workspace` 导致多窗互相覆盖。

### 5.4 审批对话框

Runtime 已有 `POST .../resolve-approval` 且 pending 带 `thread_id` / `turn_id`。

| 步骤 | 行为 |
|------|------|
| 注册 | 本窗 `start_turn` / resume 成功后 `register_window_thread(label, thread_id)` |
| 事件 | SSE `approval.required`（或等价）到达时，Rust **或** 各窗前端根据 `thread_owner` 判断 |
| 展示 | **仅 owner 窗** 弹 `ApprovalDialog`；其它窗可显示非阻塞 banner「项目在 X 窗等待审批」 |
| 超时 | 仍走 runtime 120s 默认 deny；owner 窗关闭时不得丢 pending — 自动转移到「任一有该 thread 注册的窗」或托盘通知点击聚焦 |

若前端路由：在 `App.tsx` 处理 SSE 时 `if (threadId !== resumedThreadId && !isOwner(threadId)) return`。

### 5.5 右栏、终端、预览

- `WorkspaceFilesPanel`、`TerminalPanel`、`RightPanel` 仅服务本窗 `resumedThreadId` + `workspaceRoot`（已满足，只要状态不跨窗泄漏）。
- `panelPreview`、`ApprovalDialog` 状态保持在各 `App` 实例内，无需共享。

### 5.6 系统通知

- 本窗最小化或失焦时，其它 thread 的 `turn.completed` / 审批：用 `tauri-plugin-notification`，body 带 workspace 名；点击 → `focus_agent_window(owner)` + 切 session。

### 5.7 标题栏 / 新窗口入口

- `TitleBar` 或应用菜单：**新建窗口** → `invoke('create_agent_window', { workspace: current })`。
- macOS 可后续接系统菜单栏；Win/Linux 先做 TitleBar 汉堡菜单。

### 5.8 与 Vite 纯 Web 开发

- `npm run dev` 无 Tauri：保持单窗逻辑；`get_window_label` 失败时用 `'dev'`。
- 多窗 E2E 仅在 `tauri dev` / 打包产物验证。

---

## 6. Runtime / API 依赖

### 6.1 无需改 runtime 即可启动 M1–M3

- 多 `thread` 并行 turn：已支持。
- 会话 SQLite：全局；多窗读同一库无妨。
- `resolve-approval`：已按 thread/turn 作用域。

### 6.2 可选增强（M4+）

| API / 行为 | 用途 |
|------------|------|
| `GET /v1/threads?workspace=...` | 侧栏过滤（若无则前端过滤） |
| SSE 事件带 `thread_id` 一致 | 已有则文档化；前端路由审批 |
| `POST /v1/threads/{id}/turns/{tid}/cancel` | 用户在某窗点 Stop（若尚无则补） |

### 6.3 资源上限

| 资源 | 建议上限 | 理由 |
|------|----------|------|
| Agent 窗口 | 8 | 内存（每 WebView ~百 MB 级） |
| 每窗 PTY | 4 | `portable-pty`、Windows 句柄 |
| 并行 streaming thread | 受 runtime 约束 | 与模型配额、机器 CPU 相关；UI 仅不人为 abort |

---

## 7. 分阶段实施

### M1 — 窗口骨架（可演示「两个空窗」）✅

- [x] `WindowRegistry` + `create_agent_window` / `close` / `list` / `focus`
- [x] `tauri-plugin-single-instance`
- [x] capabilities + `WebviewWindowBuilder` 复用 `main` 尺寸/装饰
- [x] TitleBar「新建窗口」；托盘「新建窗口」
- [x] 关最后一窗仍 hide 到托盘

**验收：** 同时开 2 窗，各自输入框独立 — **T1 ✅**

### M2 — 事件隔离 ✅

- [x] `terminal-data` / `terminal-exit` → `emit_to`
- [x] `spawn_terminal` 绑定 window label
- [x] 关窗清理 PTY
- [x] `runtime_post_stream` / `runtime_get_sse` → `emit_to`（`runtime_proxy.rs`）
- [x] Web UI：`listenRuntimeSseEvent` → `getCurrentWebviewWindow().listen`（避免 Tauri2 全局 `listen` 收他窗流）
- [x] `turn.completed` 时 `AbortController.abort()` 卸监听（`getThreadEvents` 长连接）

**验收：** A 窗开终端，B 窗不收到输出 — **T4 ✅**；A 窗聊天流式，B 窗推理/正文不叠字 — **手测**

### M3 — 并行 Agent UI ✅

- [x] `streamingThreadIds`；切 session 不 abort 他 thread SSE
- [x] per-window `localStorage` 键（`deepseek-desktop-workspace:{label}`）
- [x] 侧栏按 `primary_workspace` 过滤（+「显示全部会话」）

**验收：** A 窗跑 turn 时 B 窗可正常对话 — **T2 ✅**

### M4 — 审批路由 ✅

- [x] `register_window_thread` + `thread_owner`
- [x] 仅 owner 窗弹 `ApprovalDialog`（`thread_owned_by_window`）
- [ ] owner 关闭时 fallback 通知（⏸ 非结案阻塞；见 §7.5）

**验收：** T3 未纳入结案手测；回归需要时再勾 §9。

### M5 — 体验打磨 ⏸ 延后（本方案不实施）

结案时 **Cursor 对齐所需** 的标题、快捷键、每窗工作区已在 M1–M3 交付，**不单独阻塞发布**。

| 条目 | 状态 | 说明 |
|------|------|------|
| 窗口标题（`{folder} — Zagens`） | ✅ 已交付 | `updateWindowTitle` / 新建窗 `set_title` |
| Ctrl/Cmd+Shift+N、TitleBar 新建窗 | ✅ 已交付 | `App.tsx` / `windowBridge.ts` |
| 每窗 workspace 绑定 + 侧栏过滤 | ✅ 已交付 | M3 |
| 窗口位置/尺寸持久化 | ⏸ backlog | 建议 `tauri-plugin-window-state`；双屏用户再开 issue |
| 资源管理器「用 Zagens 打开」 | ⏸ backlog | 安装包/文件关联 + `single-instance` 传路径 |
| 托盘动态「窗口列表」子菜单 | ⏸ backlog | 当前托盘仅「新建窗口」 |

**何时再做 M5：** 有明确用户反馈（重启后窗口乱、要从文件夹一键开第二项目）再开一小迭代即可。

### M6 — 门禁 ✅

- [x] `cargo check -p deepseek-desktop`、`npm run build`（web-ui）
- [x] §9 核心手测（T1–T2、T4）
- [x] [TUI_DS_PICK_GAP.md](TUI_DS_PICK_GAP.md) 多窗口行
- [x] [CHANGELOG.md](../../CHANGELOG.md) `[Unreleased]`
- [ ] `cargo clippy -p deepseek-desktop`（CI / 发版前常规跑即可）

---

## 8. 安全与权限

- 新窗口 **不** 新增磁盘访问面；仍经 `runtime_proxy` 与现有 `read_*` commands。
- `thread_owner` 仅进程内内存，不写入磁盘。
- 单实例避免第二 sidecar 与 token 泄露面扩大。
- 动态窗口权限遵循 [security-trust.mdc](../../.cursor/rules/security-trust.mdc)：不扩大 arbitrary file read。

---

## 9. 测试清单（手测）

**结案范围：** T1–T2、T4 已通过即视为本方案验收完成；T3、T5–T9 为**可选回归**（发版前或改 M5 backlog 时再勾）。

| # | 步骤 | 期望 | 状态 |
|---|------|------|------|
| T1 | 新建窗口 ×2 | 两个独立 WebView，标题可区分 | ✅ 结案 |
| T2 | A 窗开始 streaming，B 窗发送消息 | B 成功；A 流不中断 | ✅ 结案 |
| T3 | A 窗审批 pending，焦点在 B | B 无模态挡屏；通知或 badge 提示 A | ⏸ 可选 |
| T4 | A 窗开终端 | 仅 A 显示输出 | ✅ 结案 |
| T5 | 关闭 A 窗（仍剩 B） | B 正常；sidecar 仍 alive | ⏸ 可选 |
| T6 | 关闭最后一窗 | 隐藏托盘，不退出进程 | ⏸ 可选 |
| T7 | 托盘退出 | 进程结束，7878 释放 | ⏸ 可选 |
| T8 | 双击快捷方式第二次（带路径） | 不启动第二进程；新窗打开该路径 | ⏸ 可选 |
| T9 | 第九个新窗 | 友好错误提示（达上限） | ⏸ 可选 |

---

## 10. 风险与缓解

| 风险 | 缓解 |
|------|------|
| 多 WebView 内存占用高 | 窗口上限 8；文档注明推荐机器配置 |
| SSE 连接数 × 窗 × thread | 非活跃 thread 降频轮询（方案 B）或共享一条 multiplexer（远期） |
| 审批丢在已关闭窗口 | `thread_owner` fallback + 通知点击聚焦 |
| 动态窗权限配置错误 | 对照 Tauri 2 capability 文档；CI 打包冒烟 |
| 开发机 `tauri dev` 与 prod 行为差 | 手测清单含 release build |
| 全局 `restart_sidecar` | 全窗 toast + `waitForRuntimeReady` |

---

## 11. 开放问题（结案结论）

| # | 问题 | 结案决定 |
|---|------|----------|
| 1 | 侧栏默认按 workspace 过滤 | **保持**：默认过滤 +「显示全部会话」开关（已实现） |
| 2 | 关窗后 turn 完成是否系统通知 | **延后**（M5 / 可选回归） |
| 3 | 每窗 PTY 上限 | **4/窗、全局 16**（已实现，见 `terminal.rs`） |
| 4 | macOS 关闭与 hide-to-tray | **未专项测**；有问题再开 issue |
| 5 | `tauri-plugin-window-state` | **归入 M5 backlog**，非本方案交付项 |

---

## 12. 修订记录

| 日期 | 摘要 |
|------|------|
| 2026-05-21 | 初稿：真多窗口目标架构、Rust/Web 分阶段、测试清单 |
| 2026-05-21 | §1.4 Cursor/VS Code 对齐验收清单；产品基准表述 |
| 2026-05-21 | M1–M5 代码落地（`window_registry.rs`、`windowBridge.ts`、App/Sidebar/TitleBar） |
| 2026-05-21 | 手测 T1–T2、T4 通过（维护者确认） |
| 2026-05-21 | **方案结案**：M5 延后 §7.5；M1–M4、M6 关闭 |

---

## 附录 A — 代码锚点（现状）

| 模块 | 路径 |
|------|------|
| 单窗配置 | `crates/desktop/tauri.conf.json` |
| 托盘 / 关闭 | `crates/desktop/src/main.rs` |
| 全局 runtime | `crates/desktop/src/commands.rs` `AppContext` |
| HTTP 代理 | `crates/desktop/src/runtime_proxy.rs` |
| 终端广播 | `crates/desktop/src/terminal.rs` `app.emit` |
| 切 session abort | `crates/desktop/web-ui/src/App.tsx` `handleSelectSession` |
| 发送 guard | `crates/desktop/web-ui/src/App.tsx` `handleSend` |

## 附录 B — 与「单窗并行（L2）」关系

- **L2** 可在不增加 `WebviewWindow` 的情况下部分缓解（不切 abort、按 thread streaming）。
- 本产品决策为 **直接做 L3（真多窗口）**；M3 中的并行 streaming 与 L2 重叠，**一处实现、两处受益**（即使只开一窗，切 session 也不应无故 abort 后台 turn）。
