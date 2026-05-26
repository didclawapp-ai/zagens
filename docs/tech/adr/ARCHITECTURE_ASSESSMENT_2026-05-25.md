# 架构评估快照 — 2026-05-25（"先定型，再迭代功能"）

> **类型：** 架构评估 / 决策依据（非功能 ADR）
> **作者职责：** 维护者 / 架构 owner
> **M7/M8 复评：** **2026-05-26** — M-series **M1–M8 全部落地**（[`BACKLOG_ENGINE_STRUCT_IN_CORE.md`](./BACKLOG_ENGINE_STRUCT_IN_CORE.md) **Closed**）；**D6–D8、D7、D1 闭合**；进度 **10/10**（架构定型）。
> **实施顺序签收：** **2026-05-26** — 维护者签收 **§5.1 推荐实施顺序**（D6 → D9/D10 → D7 → D8 → D1 → P2）；§5 表「已记账」= backlog ADR 存在，**≠ 已落地**。
> **配套 SSOT：** [RUNTIME_ARCHITECTURE.md](../RUNTIME_ARCHITECTURE.md)（系统架构图）· [RUNTIME_EVOLUTION_ROADMAP.md](../RUNTIME_EVOLUTION_ROADMAP.md)（演进排期）· [API_DESIGN.md](../API_DESIGN.md)
> **相关 backlog ADR：** [PR_M0_ENGINE_STRUCT_TO_CORE_SPIKE.md](./PR_M0_ENGINE_STRUCT_TO_CORE_SPIKE.md) · ~~[BACKLOG_ENGINE_STRUCT_IN_CORE.md](./BACKLOG_ENGINE_STRUCT_IN_CORE.md)~~ **Closed 2026-05-26** · [D4_APPSERVER_DEPRECATED.md](./D4_APPSERVER_DEPRECATED.md) · [BACKLOG_RUNTIME_UNIFICATION.md](./BACKLOG_RUNTIME_UNIFICATION.md) · [BACKLOG_STATESTORE_JSONL.md](./BACKLOG_STATESTORE_JSONL.md) · [BACKLOG_LANDLOCK_ENFORCE.md](./BACKLOG_LANDLOCK_ENFORCE.md) · [A1_PERSIST_BLOCKING_AUDIT.md](./A1_PERSIST_BLOCKING_AUDIT.md)

---

## 0. 摘要与决策建议（最重要的一段）

**结论：§1 = 10/10，架构定型（2026-05-26）** — M-series、D6–D8、D7、D1 均已闭合；Engine 在 core，`deepseek-runtime` sidecar 已剥离 ratatui。

| 决策 | 推荐 |
|------|------|
| 新增大型功能（新面板、新工具链路、新协议） | **可推进**（遵守 §7.1：新 `/v1/*` 须 OpenAPI + owner） |
| 现有功能 bug 修 / 性能 / a11y / 文案 | **正常推进** |
| 桌面 UI/UX 小迭代（不引入新运行时概念） | **正常推进** |
| 任何「往 `crates/runtime-server` 加新顶层文件」的改动 | **需 owner 评审**（runtime lib 承载 tools / HTTP / engine shim） |
| M-series D5（`Engine` struct → core） | **✅ 完成 2026-05-26**（M1–M8） |
| D1 巨型文件 | **✅ 闭合 2026-05-26** — runtime 四模块已拆；`desktop/commands.rs` 等见 §5.1「不拆分」 |
| 下一优先 | **P2**（§5.1 阶段 F：D11–D14）或产品功能迭代 |
| 端口动态化、删 legacy crate | **D2/D3 ✅** |

**定型后仍须遵守：** §7.1 红线（`/v1/*`、Engine 字段、`desktop` 不链 `deepseek_tui` 等）；巨型文件 ~1k 行为指南，**非**硬性拆文件 KPI（§5.1 D1 表）。

---

## 1. 定型判定（满足后即可解冻功能迭代）

以下 10 条全部勾选 = 架构定型，可大胆做功能。**当前进度：10/10**（2026-05-26 **D1 闭合** — 见 §5.1；[`D8_OPENAPI_TS_GENERATION.md`](./D8_OPENAPI_TS_GENERATION.md) · [`D7_PERSISTENCE_UNIFICATION.md`](./D7_PERSISTENCE_UNIFICATION.md)）。

- [x] **L1 turn loop 在 core**（P2 PR6 / G3 已闭合，见 [P2_G3_ENGINE_L2_SIGNOFF.md](./P2_G3_ENGINE_L2_SIGNOFF.md)）
- [x] **L2 契约稳定**（`/v1/*` 路由 + `event_schema_version: 2`，[`runtime_api/router.rs`](../../../crates/runtime-server/src/runtime_api/router.rs)）
- [x] **桌面 ↔ sidecar 双通道安全模型**（Bearer 不出 WebView + path 白名单，[H06 完成](./IMPLEMENTATION_SUMMARY_2026-05-24.md)）
- [x] **Engine struct 在 core**（**✅ M-series M1–M8 闭合 2026-05-26** — M7/M8：`deepseek_core::engine` + `EnginePlatformExt`；runtime lib shim：[`crates/runtime-server/src/core/engine.rs`](../../../crates/runtime-server/src/core/engine.rs) ~130 LOC。详见 [`BACKLOG_ENGINE_STRUCT_IN_CORE.md`](./BACKLOG_ENGINE_STRUCT_IN_CORE.md) Closed 表。）
- [x] **sidecar 二进制不再链接 ratatui / CLI**（**✅ D6 Phase B 2026-05-26** — `deepseek-runtime` 单 crate；Zagens bundles `deepseek-runtime-*`；~~`deepseek-tui`~~ 已删除 — [`D6_PHASE_B_CLI_SUNSET.md`](./D6_PHASE_B_CLI_SUNSET.md)）
- [x] **持久化单库**（**✅ D7 2026-05-26** — Sessions + Runtime threads 双 SQLite + `runtime_thread_id` 链接；叙事 SSOT [`PERSISTENCE.md`](../PERSISTENCE.md)；非物理单文件）
- [x] **`app-server` 实验路径决策**（**✅ 2026-05-26 deprecated** — [`D4_APPSERVER_DEPRECATED.md`](./D4_APPSERVER_DEPRECATED.md)；`deepseek-state` crate 保留至 D7，非整体废弃）
- [x] **`crates/tui-core` legacy 删除**（**2026-05-25 完成**，`cargo check --workspace --all-targets` 全绿）
- [x] **HTTP 契约 OpenAPI 自动生成 + TS 类型自动产出**（**✅ D8 2026-05-26** — [`zagens-runtime-v1.openapi.json`](../openapi/zagens-runtime-v1.openapi.json) + `export-runtime-openapi` + `web-ui` `generate:api-types`；[`D8_OPENAPI_TS_GENERATION.md`](./D8_OPENAPI_TS_GENERATION.md)）
- [x] **D1 巨型文件策略闭合（2026-05-26）** — runtime：`config/`、`compaction/`、`mcp/`、`client/` 已模块化；**不拆：** `desktop/commands.rs`（~1.5k）；`runtime-server` 内 >1k 行单体按需再拆

---

## 2. 当前架构的优点（值得保留 / 推广）

### 2.1 进程边界划得很硬

- [`crates/desktop`](../../../crates/desktop/) **只依赖 `config + secrets`**，不依赖 `core` 或 `deepseek_runtime` lib。
  - 桌面壳 OTA 升级 sidecar 二进制时不需要重链整个 runtime lib；
  - sidecar crash 不会拖死 Tauri 主进程；
  - Zagens v0.4.x 与 workspace v0.8.15 两条独立 SemVer 已经在 [CHANGELOG.md](../../../CHANGELOG.md) 体现。
- [`crates/desktop/build.rs`](../../../crates/desktop/build.rs) 自动从 `target/` 复制 sidecar 二进制到 `binaries/`，解决跨工程构建顺序——简单但极其有效。

### 2.2 双通道 + Bearer 注入是 Tauri 桌面安全的优等解

- token 仅在 Rust 进程注入（[`runtime_proxy.rs`](../../../crates/desktop/src/runtime_proxy.rs)），WebView 拿不到，DevTools 也拿不到；
- 路径白名单 `validate_runtime_path` 拒绝 `..` / 非 `/v1` 前缀，配合 sidecar 端 [`auth::require_runtime_token`](../../../crates/runtime-server/src/runtime_api/auth.rs) 中间件——**双重防御**；
- CORS 白名单含 `http(s)://tauri.localhost`，对齐 Tauri 2 WebView2 实际 origin（注释里有写历史伤痕）。

### 2.3 `TurnEnginePort` 切口干净

- [`RuntimeThreadManager::start_turn`](../../../crates/runtime-server/src/runtime_threads/manager.rs) 通过 `StartTurnParams` 显式校验后才发 `Op::SendMessage`；
- **核心校验在 core，平台副作用在 runtime lib**——这是 M-series 重构的唯一支点，价值很高。

### 2.4 事件 broadcast 设计成熟

- `tokio::sync::broadcast::Sender<RuntimeEventRecord>` 同时喂 SSE handler 与持久化 monitor；
- `event_coalesce` 对 `item.delta` 合并；`RecvError::Lagged` 触发客户端 `since_seq` 回放（B3.3）；
- 配合 `EVENT_CHANNEL_CAPACITY` + LRU `ActiveThreads` + `spawn_blocking` 异步落盘——三个常见性能坑都关掉了。

### 2.5 Supervisor 协议是工程派

`DS_PICK_READY {port, pid, token_fp, version}` 行协议握手 + stdin `op: ping/drain` 心跳 + 退避（`MAX_RAPID_RESTARTS=3 / RAPID_RESTART_WINDOW_SECS=60`）+ Windows `EADDRINUSE` 端口让出循环（`PORT_FREE_POLL_MS`）——这些细节都来自真实事故复盘，**应当作为内部 SDK 模板沉淀**。

### 2.6 Desktop 与 Headless 共享同一 sidecar 语义

Zagens 与 headless 脚本均消费 **`deepseek-runtime`** 同一 HTTP 契约（`/v1/*` + Bearer + `DS_PICK_READY`）——**避免 Desktop/CI 行为漂移**。~~`deepseek` CLI / `delegate_to_tui`~~ 已于 D6 Phase B 删除。

---

## 3. 问题与债务（按严重度排序）

### 3.1【高】~~`crates/tui` 胖 crate~~ → **`crates/runtime-server` 单 crate** ✅ **2026-05-26（D6 Phase B）**

原 `tui` 同时承载 ratatui UI、HTTP、tools、engine shim。Phase B 后：

| 类别 | 落点 | 状态 |
|---|---|---|
| HTTP 服务端 | `runtime-server/src/runtime_api/*` | ✅ 已迁入 |
| 线程管理 | `runtime-server/src/runtime_threads/*` | ✅ 已迁入 |
| Engine shim + tools | `runtime-server/src/core/engine/`、`tools/*` | ✅ 已迁入 |
| ratatui TUI | ~~`tui/*`~~ | ✅ **已删除** |
| CLI 分发器 | ~~`crates/cli`~~ | ✅ **已删除** |

**剩余债（非 blocking）：** sidecar 冷启动 profiling；`capacity_flow` 等 engine-flow 编排仍留 runtime lib（非 core）；`deepseek-state` 仍供 `deepseek-core` 编译依赖。

### 3.2【高】持久化三套并存

| 轨 | 位置 | 用途 |
|---|---|---|
| Sessions | `~/.deepseek/sessions/` | 桌面/TUI 会话恢复 |
| Runtime threads | `~/.deepseek/tasks/runtime/` | HTTP `/v1/threads/*` |
| `deepseek-state` SQLite | `crates/state/` | CLI `thread list` / `app-server` |

边界靠注释维护、靠 ADR 维护，没物理隔离。**实际故事**：桌面看到的 thread 与 CLI `thread list` 不一致已经发生过，是双 schema 飘移的经典症状。

**已记账：** [`BACKLOG_RUNTIME_UNIFICATION.md`](./BACKLOG_RUNTIME_UNIFICATION.md) + [`BACKLOG_STATESTORE_JSONL.md`](./BACKLOG_STATESTORE_JSONL.md)。

### 3.3【中】~~端口 7878 写死~~ ✅ **2026-05-25 完成**

桌面端口管理已从 `u16` 升级为 `tokio::sync::watch::channel::<u16>`：

- **Sidecar**（[`runtime_api/mod.rs`](../../../crates/runtime-server/src/runtime_api/mod.rs)）：`DS_PICK_READY.port` 现在报告 `listener.local_addr().port()`——**实际绑定**端口而非请求端口；
- **Desktop 壳**：`AppContext::runtime_port` 改为 `watch::Receiver<u16>`，supervisor 持 `Sender`；所有 IPC handler（`runtime_proxy::{http,post_stream,get_sse}` + `commands::{export_thread_json, export_session_json, rebuild_symbol_index, read_thread_workspace_binary}`）通过 `AppContext::require_port()` 读真实端口；
- **`get_runtime_port`**：改为 `rx.changed().await` 等待第一次发布——web-ui `initRuntimeConfig` 调用时自然阻塞到 sidecar 就绪；
- **重启路径**：spawn 新 sidecar 前 `port_tx.send(0)`，IPC handler 期间 fast-fail 或 await 新发布，杜绝拿到 stale 端口；
- **`--port 0` ephemeral 绑定**（2026-05-25 follow-up）：移除了 `runtime_api/mod.rs` 中 `if options.port == 0 { bail!(...) }` 守卫——用户/上游脚本可显式传 `--port 0` 让 OS 自动选端口，bound 端口仍通过 `DS_PICK_READY` 回报；`bail!` 从 `anyhow` import 中清理。 sidecar contract regression 复跑 ✅。
- **回归测试**：`sidecar_contract_full_lifecycle` ✅；`runtime_proxy::tests` ✅；`desktop::architecture_boundary` ✅。

### 3.4【中】Engine struct 与工具实现耦合在 tui

- `mcp.rs` 2.2k 行、`tools/*` 28k、`lsp/*` 1.3k、`sandbox/*` 2k 行全部和 Engine 同 crate；
- 想做"sidecar 不带 ratatui"必须先把这些 trait 化；
- `SandboxBackend` 已经是 `dyn trait` ✅；8 个 host trait 已在 M3–M5 立起；**Engine struct + op loop 已在 core** ✅（M7/M8）。
- **2026-05-25 M3 进度：** LSP / SubAgent / Shell / Sandbox 四组边界 trait 已在 `deepseek_core::engine::hosts` 立起（call-graph driven，对应 spike §5 R1）；`SandboxBackend` trait 与 `DiagnosticBlock` 数据类型搬入 core；tui 侧通过 inline `impl` + `TuiSandboxHost` / `TuiShellHost` newtype 接线。剩余 MCP / Seam / Cycle / Workshop / TopicMemory 等子系统的 trait 化拆到 M4 / M5。
- **2026-05-25 M4 进度：** `McpHost` trait 已在 `deepseek_core::engine::hosts::mcp` 立起（4 个 default-impl 方法），`mcp.rs` 2218 LOC body 零修改。**M8 后：** MCP pool `shutdown_all` 经 `EnginePlatformExt::on_shutdown`；`ensure_pool` 仍 tui engine-flow。
- **2026-05-25 M6 进度：** `CapacityController` + coherence reducer 原子搬核（spike R10）。tui `capacity.rs` / `coherence.rs` 收缩为 re-export shim。**M7/M8 后** engine-flow 集成测 green（见 M7/M8 条目）。

- **2026-05-25 M5 进度：** 三个边界 trait + `ScratchpadStepState` 类型搬核：
  - `SeamHost`（M-series 至今最宽 trait，10 方法 — `config_enabled` / `highest_level` / `seam_level_for` / `verbatim_window_start` / `collect_seam_texts` / `produce_soft_seam` / `recompact` / `seam_count` / `produce_flash_briefing` / `reset`）覆盖完整 layered-context Flash pipeline #159；opaque `SeamError = Box<dyn std::error::Error + Send + Sync>` 让 `anyhow::Error` 通过 `.map_err(Into::into)` 安全跨越 core 表面，**不**把 tui 的错误层级（`anyhow` / `reqwest` / `LlmClientError`）泄漏到 core。inherent `new` / `should_cycle`（死代码）/ 私有 `summarize_messages` 按 R1 故意不在 trait 上；`config()` 返回的 `SeamConfig` 因为是 tui-only 类型，被替换为更窄的 `config_enabled() -> bool` 访问器（Engine 只读 `.enabled` 这一位）。
  - `WorkshopHost` 空 marker（mirrors M3 `ShellHost`）— Engine 从不在 `workshop_vars` 上调方法，`tool_context.rs:51` 唯一引用只是把 `Arc<Mutex<WorkshopVariables>>` 克隆到 `ToolContext`（所有 `WorkshopVariables` 方法都从 tool 实现内部调用，与 Engine 正交）。tui 侧 `crates/tui/src/tools/large_output_router.rs` 新增 `TuiWorkshopHost(pub Option<Arc<Mutex<WorkshopVariables>>>)` newtype + 空 `impl`。
  - `TopicMemoryHost` 2 方法 `compose_block(query_hint) -> Option<String>` / `on_turn_complete(user, assistant)`：**settings 移入实现**（`TopicMemoryRuntime` 新增 `settings: TopicMemorySettings` 字段 + `TopicMemoryRuntime::new(settings)` 构造器），trait 表面无 settings 参数，避免 R9 spike option (b) 把 `deepseek-topic-memory` 拉入 core deps，也避免在 core 重复定义 `TopicMemorySettings` 这种平行结构反模式。settings 热加载今天没有任何 slash command 入口，engine init 的一次性克隆已经足够。
  - `ScratchpadStepState`（2 `usize` 字段 + `reset(&mut self)`，~30 LOC）搬入 `core::engine::scratchpad_state` per R12；`crates/tui/src/core/engine/scratchpad_flow.rs` 484 LOC 的 UI / 审计 / 覆盖 / 提醒辅助（`record_tool_outcome` / `inject_summary_if_needed` / `build_layered_summary` / `coverage_gate` / `read_inventory` …）**保持 tui 侧**，文件顶部留 `pub use deepseek_core::engine::ScratchpadStepState;` re-export shim 让所有 `use crate::core::engine::scratchpad_flow::ScratchpadStepState` 调用方编绿。
  - tui 侧实现：… Engine call-site swap 经 UFCS 走 trait；**M7** 字段已切换为 `Box<dyn …Host>` / `Arc<dyn LspHost>` 等（见 M7/M8 条目）。
- **2026-05-26 M7/M8 进度（M-series 闭合）：**
  - **M7：** `Engine<P,R>` struct + `EngineHostBundle<P,R>` + `Engine::with_hosts` 入 `deepseek_core::engine`；35 字段 host 侧换 trait object；7 条 mpsc channel core 侧创建（R11）；tui `build_engine` 构造具体子系统并装箱；`Engine::ext` 为 `Box<dyn EnginePlatformExt<P,R>>`（承载 `EngineRuntimeExt` + 后续 op 分发）；tui 保留 `#[repr(transparent)]` newtype + `engine_from_core` 供 platform dispatch 复用 inherent impl。
  - **M8：** `deepseek_core::engine::op_loop` — `Engine::run()` 事件循环；cancel / approve / deny / truncate core 内联；其余 `Op` 经 `EnginePlatformExt::dispatch_op`（tui `platform_dispatch.rs`）；MCP `shutdown_all` 在 `on_shutdown`；删除 tui `op_loop.rs` / `op_handlers.rs` / `engine_new.rs`。
  - **测试 / 性能：** 原 2 个 pre-existing 失败（topic_memory 注入 fixture、`capacity` 集成测 trim O(n²)）已修复；`context_trim` 增加 bulk-drain 快路径。
  - **仍留 tui（非 blocking）：** `capacity_flow/*`、`turn_loop/host_impl/*`、`message_handlers` 等 engine-flow 编排；spike 未要求 M8 全量迁入 core。
  - **下一刀：** D6 `crates/runtime-server` — sidecar 二进制不再链接 ratatui（§1 第 5 项）。

### 3.5【中】`commands.rs` (desktop) ~1.5k 单文件

至少 7 类混在一起：API key / Vision 桥 / 系统设置 / 符号索引 / 终端 PTY / 窗口管理 / 文件二进制读取。理想形态可拆 `commands/{auth,vision,settings,symbol_index,terminal,workspace}.rs`。**2026-05-26 维护者决定：D1 不拆**（~1.5k 尚可维护；IPC 命令集中便于检索；避免碎片化；后续大改时再拆）。

### 3.6【中】多窗口 ↔ 单 sidecar 的事件归属

- [`window_registry::register_window_thread`](../../../crates/desktop/src/window_registry.rs) 已记录"窗口 → thread"归属；
- 但 SSE 事件回流**没按归属过滤**：所有订阅同一 thread 的窗口都会收到；
- 当前没造成 bug，**但跨窗口分叉/恢复场景下会出现"幽灵渲染"**。

### 3.7【中】`runtime_proxy` 把 SSE 当 Tauri event 中转

```
WebView → invoke runtime_get_sse → reqwest stream
       → app.emit_to(window, "runtime://events-chunk", payload)
       → WebView JS 监听 → streamNormalize.ts
```

- 每条 SSE 帧额外拷贝 3-4 次 + JSON 解析两轮；
- 代价是 token 不进 WebView——**这是有意为之的安全权衡**，不一定要改。

### 3.8【低】~~`crates/tui-core` legacy 没删~~ ✅ **2026-05-25 完成**

已从 workspace 移除并删除目录；`cargo check --workspace --all-targets` 全绿。同步删除 [`RUNTIME_ARCHITECTURE.md`](../RUNTIME_ARCHITECTURE.md) §3 依赖图 legacy 节点。

### 3.9【低】~~`crates/app-server` 实验路径长期残留~~ ✅ **2026-05-26 决策：deprecated**

[`D4_APPSERVER_DEPRECATED.md`](./D4_APPSERVER_DEPRECATED.md) — `deepseek app-server` / `crates/app-server` 标记废弃；**不删代码**（后续 PR 移除）。生产 HTTP 仅 sidecar → D6 `runtime-server`。`deepseek-state` 仍供 CLI `thread` 等，待 D7 统一。

### 3.10【低】HTTP 契约没有版本化策略 / 自动 ts 类型

- 30+ 个 `/v1/*` 端点，desktop ts 类型靠手写 + `streamNormalize.ts` 防护性兼容；
- 已有 `schemars` 但没生成 OpenAPI；
- 未来想 break change 时没有 `/v2` 路径策略。

---

## 4. 评分汇总

| 维度 | 评分 | 简评 |
|---|---|---|
| 分层与边界（L1/L2/L3） | ★★★★★ | L1 turn loop + **Engine struct + op loop 均在 core**；tui 为 shim + tools + HTTP |
| 进程模型 / 安全 | ★★★★★ | 每次 UUID Bearer + 路径白名单 + token 不出 WebView，业界水平 |
| 通信契约（HTTP+IPC 双通道） | ★★★★☆ | 切分干净；缺少版本化策略与 OpenAPI 生成 |
| 持久化 | ★★★☆☆ | 三套并存，需要长期合并 |
| 可观测性与错误处理 | ★★★★☆ | `ErrorEnvelope` + `tracing` + supervisor.log 完备；缺指标化 |
| 并发 / 取消 / 背压 | ★★★★☆ | `broadcast + coalesce + Lagged catch-up` 成熟；取消两层需文档化 |
| 测试基线 | ★★★★☆ | `runtime_api/tests.rs` 76k、`runtime_threads/tests.rs` 103k，回归网密 |
| **代码物理组织** | **★★★☆☆** | Engine 边界已清；巨型文件 + sidecar 仍链 ratatui 是剩余主债 |

---

## 5. 迭代方向（按优先级）

### P0 · 0-2 个月（结构定型期，必须做）

| ID | 内容 | 工作量 | 已记账 |
|----|------|--------|--------|
| **D1** | 巨型文件：**runtime 四模块已拆**；**其余不拆**（§5.1，含 `desktop/commands.rs`） | runtime 4 PR ✅；策略闭合 ✅ | — |
| **D2** | 端口动态化：桌面消费 `DS_PICK_READY {port}`，去掉 7878 写死 | ≤2 天 | ✅ **完成 2026-05-25**（含 `--port 0` ephemeral 守卫移除 follow-up） |
| **D3** | 删 `crates/tui-core` legacy | ≤0.5 天 | ✅ **完成 2026-05-25** |
| **D4** | 决策 `crates/app-server`：晋升或 deprecated | 决策 0.5 天 | ✅ **deprecated 2026-05-26** — [`D4_APPSERVER_DEPRECATED.md`](./D4_APPSERVER_DEPRECATED.md)；代码移除 defer |
| **D5** | **Engine struct → core（M-series M1→M8）** | 4-6 周 / 1 人 | ✅ **完成 2026-05-26** — M1–M8 全落地；[`BACKLOG_ENGINE_STRUCT_IN_CORE.md`](./BACKLOG_ENGINE_STRUCT_IN_CORE.md) Closed；§6 回归 + sidecar contract green；详见 CHANGELOG `[Unreleased]` M7/M8 条目 |

### P1 · 0-3 个月（D5 完成后立即启动）

| ID | 内容 | 工作量 | 已记账 |
|----|------|--------|--------|
| **D6** | 抽 `crates/runtime-server`（sidecar 不再链 ratatui / CLI） | 2-3 周（D5 完成后） | **可启动** — D5 ✅ |
| **D7** | 持久化整合 Sessions ⊕ Runtime threads → 单 SQLite + 视图 | 4-6 周 | ✅ **2026-05-26** — [`D7_PERSISTENCE_UNIFICATION.md`](./D7_PERSISTENCE_UNIFICATION.md) · [`PERSISTENCE.md`](../PERSISTENCE.md) |
| **D8** | OpenAPI schema 导出 + `web-ui` ts 类型自动生成 | 1-2 周 | — |
| **D9** | 取消/打断两层契约文档化 + `api/client.ts` 统一封装 | 3-5 天 | ✅ **2026-05-26** — [`D9_D10_DESKTOP_UX.md`](./D9_D10_DESKTOP_UX.md) · `turnControl.ts` · API_DESIGN §2.1.1 |
| **D10** | 跨窗口事件按 thread owner 过滤（消除"幽灵渲染"） | 1 周 | ✅ **2026-05-26** — 同上 · `filterThreadStreamEvents` · API_DESIGN §2.1.2 |

### P2 · 6-12 个月（产品形态升级）

| ID | 内容 | 工作量 | 已记账 |
|----|------|--------|--------|
| **D11** | `/v1/metrics`（Prometheus 兼容）+ 桌面运行时健康分页 | 2-3 周 | — |
| **D12** | 每 workspace 一个 sidecar（多 sidecar 隔离） | 6-8 周 | — |
| **D13** | Capability Manifest（合并 `sandbox` / `execpolicy` / `network_policy` / `command_safety`） | 4-6 周 | ✅ [`BACKLOG_LANDLOCK_ENFORCE.md`](./BACKLOG_LANDLOCK_ENFORCE.md) |
| **D14** | MCP 池稳定性增强（健康检查 + 背压 + 子进程纳入 supervisor） | 2-3 周 | — |

### 5.1 推荐实施顺序（2026-05-26 维护者签收）

> **目标：** 先把 §1 从 **6/10 → 10/10**（结构定型），再启动 P2 产品形态升级。  
> **人力假设：** 约 **1 人**；PR 保持小且可回归（对齐 §7 纪律）。  
> **读表须知：** §5 中「✅ backlog」仅表示已有 ADR/记账（如 D7/D13），**不代表代码已落地**。

#### 排期原则

1. **先改进程 / crate 边界，再改数据模型** — D6 不完成，D7 会在 `crates/tui` 巨壳内越拆越乱。
2. **先稳定 HTTP 契约，再自动生成类型** — D8 落在 D6 之后（可与 D7 尾段并行），避免对着即将搬家的模块生成 OpenAPI。
3. **D1 按「碰到的文件就拆」** — 不单独占阶段；挂在大 PR 后做 follow-up（优先 D6 动到的 `runtime_api` / `client.rs`，而非先啃 4800 行 `config.rs`）。
4. **D9/D10 体量小、风险低** — 可插空，不挡主线；改善多窗口 / 取消体验，**不增加 §1 勾选数**。
5. **P2 全部延后** — 等 §1 ≥ 8/10 再动 D11–D14；**D12（每 workspace 一 sidecar）** 最大，放 P2 末位（勿与 DEV_NOTES「D12 Desktop-only」产品战略混淆）。
6. **D4 代码物理删除** — defer 至 **D7 完成后**（`deepseek-state` 退场时一并删 `app-server`），不必单独占阶段。

#### 阶段一览

| 阶段 | 时段（约） | 焦点 | §1 里程碑 |
|------|------------|------|-----------|
| **A** | 2–3 周 | **D6** `runtime-server`（spike → 落地 → sidecar contract 回归） | **#5 勾选 → 7/10** |
| **B** | 1–2 周（插空） | **D9** 取消两层契约 + **D10** SSE 按 thread owner 过滤 | 体验债；进度仍 7/10 |
| **C** | 4–6 周 | **D7** 持久化统一（设计定稿 → 单 SQLite + 视图 → 迁移 PR 链） | **#6 勾选 → 8/10**（§7.1 部分红线可放宽） |
| **D** | 1–2 周 | **D8** OpenAPI 导出 → CI 生成 TS → `/v2` 策略草案（文档） | **#9 勾选 → 9/10** |
| **E** | 2–4 周 | **D1** 巨型文件收尾（见下表顺序） | **#10 勾选 → 10/10 定型** |
| **F** | 6–12 个月 | **P2：** D11 → D14 → D13 → D12 | 定型后 |

**阶段 A 同期辅线（D1）：** D6 PR 触及的大文件，同 PR 或紧接 follow-up 拆分。

**阶段 C 同期辅线（D1）：** 动到 `compaction.rs` / persistence monitor 时再拆。

#### D1 — **已闭合**（阶段 E，2026-05-26）

**原则：** 软上限 ~1000 行为**指南**而非硬性拆文件 KPI；已拆的 runtime 四模块保留；其余 >1k 行 **维护者决定不拆**，避免小文件碎片化；**后续大改/触及时再拆**。

**已模块化（runtime，2026-05-26）：** `config/` · `compaction/` · `mcp/` · `client/`（实现 ≤~650 行；测试卷 `tests.inc.rs` 可 >1k）。

**明确不拆分（维护者 2026-05-26）：**

> 判定：非 D1 收益路径 · ~1k–1.5k 仍可维护 · 避免 IPC/工具链碎片化。`crates/tui` 非桌面路径与 `desktop` 分表列出。

| 类别 | 代表路径（~行） | 理由 |
|------|-----------------|------|
| **Zagens 桌面 IPC** | `desktop/src/commands.rs` ~1.5k | Tauri `#[tauri::command]` 集中；~1.5k 尚可；后续按域拆 `commands/{auth,vision,…}` **按需** |
| **ratatui TUI（freeze）** | `tui/ui.rs` ~7.4k · `tui/app.rs` ~4.4k · `tui/history.rs` ~4.1k · `tui/widgets/mod.rs` ~2.6k · 等 | 仅 `tui-ui`；与桌面 Web UI 无关 |
| **CLI** | `cli/commands/legacy.rs` ~3.3k | CLI 入口 |
| **TUI 文案** | `localization.rs` ~1.9k | 仅 TUI chrome；桌面 i18n 在 `web-ui/` |
| **工具 / 引擎** | `tools/subagent/mod.rs` ~4.0k · `tools/shell.rs` ~2.4k · `task_manager.rs` ~1.8k · 等 | sidecar 内实现 |
| **LLM 客户端** | `client/chat.rs` ~1.5k | `client/` 已拆；`chat.rs` 单体保留 |
| **HTTP 服务端** | `runtime_api/*`（已子模块化） | 触及子文件时再拆 |
| **测试卷** | `*/tests.rs` · `tests.inc.rs` | 测试可 >1k |

#### P2 内部顺序（阶段 F，§1 = 10/10 后）

| 顺序 | ID | 理由 |
|------|-----|------|
| F1 | **D11** metrics + 健康页 | 可观测性；不破坏架构（2–3 周） |
| F2 | **D14** MCP 池稳定性 | 运维 / 可靠性；sidecar 形态稳定后再加固 |
| F3 | **D13** Capability Manifest + Landlock | 跨平台、工作量大；需 runtime 边界清晰 |
| F4 | **D12** 每 workspace 一 sidecar | 最大改动（6–8 周）；依赖 supervisor / 端口 / 持久化均已定型 |

#### 不建议的顺序

- **先 D7 再 D6** — 持久化仍在 `tui` 巨壳，合并成本翻倍。
- **先 D12 再 D6/D7** — 多 sidecar 放大三套持久化与单 sidecar 假设的所有问题。
- **D8 与 D7 全并行且同步改 API** — 代码生成反复 churn。
- **一口气拆完 D1 再 D6** — `config.rs`  alone 可拖住关键路径 1–2 个月。

#### 粗略日历（自 2026-05-26）

| 时段 | 焦点 | §1 |
|------|------|-----|
| 2026-05 下旬 – 06 月中旬 | D6 | 7/10 |
| 2026-06 中下旬 | D9 + D10（插空） | 7/10 |
| 2026-06 中旬 – 07 月底 | D7 | 8/10 |
| 2026-08 | D8 | 9/10 |
| 2026-08 – 09 | D1 收尾 | **10/10** |
| 2026-Q4 起 | P2（D11→D14→D13→D12） | 定型后 |

---

## 6. "如果只能做一件事"

**做 D6（抽 `crates/runtime-server`，sidecar 去 ratatui）。**

D5（M-series）✅ 已闭合。下一刀直接解锁 §1 第 5 项，并让 sidecar 二进制体积 / 攻击面 / 冷启动与文档叙事一致。

理由：

1. Engine struct + op loop 已在 core — **物理拆分 sidecar 已无结构 blocker**；
2. D1（拆巨型文件）、D7（持久化整合）在 runtime-server 独立 crate 后边界更清晰；
3. 桌面 / CLI / 未来插件仍消费同一 HTTP 契约 + `DS_PICK_READY` 握手；
4. 工作量 bounded：复用现有 `runtime_api/*` + `runtime_threads/*`，从 `deepseek-tui` 二进制剥离 ratatui / TUI freeze 代码路径。

完整排期见 **§5.1**（D6 完成后 D9/D10 插空 → D7 → D8 → D1 → P2）。

---

## 7. 功能冻结期的纪律（执行守则）

为了让定型期不被反复打断，建议团队遵守：

### 7.1 PR 准入红线

- ⛔ **禁止**在 `crates/runtime-server` 新建顶层文件（子模块拆分除外）
- ⛔ **禁止**新增 `/v1/*` 端点，除非配套补 OpenAPI schema 草案
- ⛔ **禁止**让 `desktop` crate 直接 `use deepseek_core` 或 `deepseek_runtime`——它只能消费 HTTP + IPC
- ⛔ **禁止**给 `deepseek_core::engine::Engine` 加新字段（M-series 35 字段表已封口；变更需 spike + owner）
- ⚠ **谨慎**：往 **`desktop/commands.rs`** 或 §5.1「D1 不拆分」表内文件 **大量** 加代码时，评估是否触及再拆（非 D1 硬性门槛）；新 `/v1/*` 与 `Engine` 字段仍须 owner

### 7.2 允许且鼓励

- ✅ 桌面 `web-ui/` 内部 UX 改进、a11y、i18n
- ✅ bug 修复、性能优化、错误信息改善
- ✅ 测试补充（`runtime_api/tests.rs`、`runtime_threads/tests.rs`、`core/engine/tests.rs`）
- ✅ 文档：本文件、`RUNTIME_EVOLUTION_ROADMAP.md`、`API_DESIGN.md`、ADR
- ✅ M-series PR（M1→M8）— **✅ 已闭合 2026-05-26**
- ✅ D6 `runtime-server` — **✅ Phase B 已闭合 2026-05-26**（见 [`D6_PHASE_B_CLI_SUNSET.md`](./D6_PHASE_B_CLI_SUNSET.md)）

### 7.3 评审升级路径

任何看起来不属于 7.2 的 PR：
1. 在描述里指出"是否触发 7.1 红线"；
2. 如果触发，在 PR 顶上贴 `[ASSESS-2026-05-25 §1 未勾选项: …]`；
3. 由架构 owner 决定接受、改写或推迟到定型后。

---

## 8. 重新评估时点

- **M-series M3 合并后**（✅ 2026-05-25）：评估 §1 第 4-5 项是否可勾 — **结论：仍 `[ ]`**。M3 立起的是边界 trait（call-graph driven，方法表小），Engine struct 35 字段 / op_loop / engine_new 主体仍在 tui。第 4 项的勾选条件是 spike §6 M7 (Engine struct + engine_new + op_handlers 进 core，`crates/tui/src/core/engine.rs` ≤ 80 LOC) 闭合；M3 是必要前置（trait surface 就绪），不是 sufficient 条件。第 5 项（sidecar 不再链 ratatui）依赖 M7+M8 完成后才能拆 `crates/runtime-server`，同样未勾。
- **M-series M4 合并后**（✅ 2026-05-25）：M4 把 §3.4 中点名的 `mcp.rs` 2.2k 行子系统补齐了第 5 个 host trait — 至此 5 大 tui-only 子系统（LSP / SubAgent / Shell / Sandbox / MCP）全部有了 `deepseek_core::engine::hosts::*` 边界 trait。剩余 §3.4 trait 化工作量：Seam / Cycle / Workshop / TopicMemory（M5–M6）。§1 第 4-5 项**仍 `[ ]`** — M4 同 M3 一样只是 trait surface 推进，Engine struct 字段 / op_loop / engine_new 主体仍在 tui，等 M7+M8 完成后才能勾。
- **M-series M6 合并后**（✅ 2026-05-25）：M6 把 §3.4 中 `capacity_controller` 字段（677 LOC controller body）+ M1-deferred coherence reducer 原子搬核进 `deepseek_core::{capacity, coherence}`，tui 两个文件收缩到纯 re-export shim（只保留 tui-`Config`-coupled 的 `capacity_config_from_app` 适配器）。**spike R10 兑现**：单 PR 原子移动 + 同 PR 删除 tui 原 body，没有 double-implementation 窗口。剩余 M-series 工作量：`Engine` struct + `engine_new` + `op_handlers` 进核（M7，最重一刀），`op_loop` 入核 + final cleanup（M8）。§1 第 4-5 项**仍 `[ ]`** — M6 是类型搬核而非 Engine struct 整体迁移，35 字段构造 / engine_new / op_loop 仍在 tui，等 M7+M8 才能勾。

- **M-series M5 合并后**（✅ 2026-05-25）：M5 立起 `SeamHost`（M-series 至今最宽 trait，10 方法覆盖整条 layered-context Flash pipeline #159 — `crates/tui/src/seam_manager.rs` 712 LOC 子系统补齐边界）+ `WorkshopHost`（空 marker，`large_output_router.rs` 604 LOC 体不动）+ `TopicMemoryHost`（2 方法，`topic_memory.rs` 307 LOC 子系统补齐边界，settings 移入实现避免 R9 spike crate dep）+ `ScratchpadStepState` 类型搬核（`scratchpad_flow.rs` 484 LOC UI/审计/覆盖 helpers 保留 tui 侧 per R12，用 re-export shim）。至此 §3.4 列出的全部 tui-only 子系统 host trait 全部立起（LSP / SubAgent / Shell / Sandbox / MCP / Seam / Workshop / TopicMemory — 8 个）。剩余 M-series 工作量：CapacityController + coherence reducer 入核（M6），`Engine` struct + `engine_new` + `op_handlers` 入核（M7），`op_loop` 入核 + final cleanup（M8）。§1 第 4-5 项**仍 `[ ]`** — M5 只是 trait surface + 字段语义清理，Engine struct 整体 35 字段 / engine_new / op_loop 主体仍在 tui，等 M7+M8 完成后才能勾。
- **D4 决策（2026-05-26）**：app-server **deprecated** — §1 #7 勾选；进度 6/10；crate/CLI 保留，见 [`D4_APPSERVER_DEPRECATED.md`](./D4_APPSERVER_DEPRECATED.md)。
- **§5.1 实施顺序签收（2026-05-26）**：维护者签收 D6→D9/D10→D7→D8→D1→P2 主线；§0 冻结窗口估算更新为 10–14 周；P2 内部 D11→D14→D13→D12。
- **D9 + D10 落地（2026-05-26）**：阶段 B 插空完成 — 见 [`D9_D10_DESKTOP_UX.md`](./D9_D10_DESKTOP_UX.md)；下一主线 **D7**；§1 仍 7/10。
- **D8 落地（2026-05-26）**：OpenAPI + TS 生成 — [`D8_OPENAPI_TS_GENERATION.md`](./D8_OPENAPI_TS_GENERATION.md)；§1 #9 → **9/10**；下一主线 **D1**。
- **D1 范围（2026-05-26）**：`crates/tui` 非桌面 >1k 行单体 **不拆分** — §5.1 表。
- **D1 闭合（2026-05-26）**：**`desktop/commands.rs` 不拆分**（~1.5k 可接受，避免碎片化）；§1 **10/10** 架构定型。
- **D7 落地（2026-05-26）**：阶段 C 闭合 — C1–C6；§1 #6 → **8/10**。
- **M-series M8 合并后**（✅ **2026-05-26**）：§1 第 4 项 **勾选** — core 侧 `Engine` struct、`Engine::with_hosts`、`Engine::run()` op loop + `EnginePlatformExt` 平台分发；tui 侧 newtype shim ~130 LOC。§1 第 5 项**仍 `[ ]`**（sidecar 仍链 ratatui → **D6**）。进度 **5/10**。2 个 pre-existing engine 集成测修复；[`BACKLOG_ENGINE_STRUCT_IN_CORE.md`](./BACKLOG_ENGINE_STRUCT_IN_CORE.md) Closed；`HANDOFF_M7_M8.md` 已删。
- **M-series M8 合并后（归档说明）**：当 D6 + §1 ≥ 8/10 时，可将本文档归档并另起 `ARCHITECTURE_ASSESSMENT_<date>.md` v2 全量重写；当前在 **同文件内追加 2026-05-26 复评** 以保持链接稳定。
- **D6 Phase B 落地（2026-05-26）：** `crates/runtime-server` 单 crate；删 `crates/cli`、`crates/tui`、ratatui；§1 #5 最终勾选；见 [`D6_PHASE_B_CLI_SUNSET.md`](./D6_PHASE_B_CLI_SUNSET.md) · commit `613a6e3`。
- **§1 = 10/10（2026-05-26）**：架构定型；§7.1 结构性红线仍有效（`/v1/*`、Engine 字段、`desktop` 边界等），巨型文件拆分为**按需**而非 KPI。
- **归档时机**：P2 阶段性复盘或下一 major 架构变更时，可将本文档另存为历史快照。

---

## 9. 参考与交叉链接

- 系统架构图：[`RUNTIME_ARCHITECTURE.md`](../RUNTIME_ARCHITECTURE.md)
- 长期演进：[`RUNTIME_EVOLUTION_ROADMAP.md`](../RUNTIME_EVOLUTION_ROADMAP.md)
- HTTP/IPC 契约：[`API_DESIGN.md`](../API_DESIGN.md)
- 实施快照：[`IMPLEMENTATION_SUMMARY_2026-05-24.md`](./IMPLEMENTATION_SUMMARY_2026-05-24.md)
- M-series spike（**Closed**）：[`PR_M0_ENGINE_STRUCT_TO_CORE_SPIKE.md`](./PR_M0_ENGINE_STRUCT_TO_CORE_SPIKE.md) · [`BACKLOG_ENGINE_STRUCT_IN_CORE.md`](./BACKLOG_ENGINE_STRUCT_IN_CORE.md)
- 持久化合并 backlog：[`BACKLOG_RUNTIME_UNIFICATION.md`](./BACKLOG_RUNTIME_UNIFICATION.md) · [`BACKLOG_STATESTORE_JSONL.md`](./BACKLOG_STATESTORE_JSONL.md)
- 沙盒增强 backlog：[`BACKLOG_LANDLOCK_ENFORCE.md`](./BACKLOG_LANDLOCK_ENFORCE.md)
- 阻塞 I/O 审计：[`A1_PERSIST_BLOCKING_AUDIT.md`](./A1_PERSIST_BLOCKING_AUDIT.md)
- 产品战略：[`../../desktop/DEV_NOTES.md`](../../desktop/DEV_NOTES.md)（D12 Desktop-only）
- M7/M8 变更记录：[`CHANGELOG.md`](../../../CHANGELOG.md) `[Unreleased]`
