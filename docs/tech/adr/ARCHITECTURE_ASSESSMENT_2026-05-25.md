# 架构评估快照 — 2026-05-25（"先定型，再迭代功能"）

> **类型：** 架构评估 / 决策依据（非功能 ADR）
> **作者职责：** 维护者 / 架构 owner
> **配套 SSOT：** [RUNTIME_ARCHITECTURE.md](../RUNTIME_ARCHITECTURE.md)（系统架构图）· [RUNTIME_EVOLUTION_ROADMAP.md](../RUNTIME_EVOLUTION_ROADMAP.md)（演进排期）· [API_DESIGN.md](../API_DESIGN.md)
> **相关 backlog ADR：** [PR_M0_ENGINE_STRUCT_TO_CORE_SPIKE.md](./PR_M0_ENGINE_STRUCT_TO_CORE_SPIKE.md) · [BACKLOG_ENGINE_STRUCT_IN_CORE.md](./BACKLOG_ENGINE_STRUCT_IN_CORE.md) · [BACKLOG_RUNTIME_UNIFICATION.md](./BACKLOG_RUNTIME_UNIFICATION.md) · [BACKLOG_STATESTORE_JSONL.md](./BACKLOG_STATESTORE_JSONL.md) · [BACKLOG_LANDLOCK_ENFORCE.md](./BACKLOG_LANDLOCK_ENFORCE.md) · [A1_PERSIST_BLOCKING_AUDIT.md](./A1_PERSIST_BLOCKING_AUDIT.md)

---

## 0. 摘要与决策建议（最重要的一段）

**结论：架构尚未定型，建议进入「功能冻结 + 结构收尾」窗口期。**

| 决策 | 推荐 |
|------|------|
| 新增大型功能（新面板、新工具链路、新协议） | **暂缓** |
| 现有功能 bug 修 / 性能 / a11y / 文案 | **正常推进** |
| 桌面 UI/UX 小迭代（不引入新运行时概念） | **正常推进** |
| 任何「往 `crates/tui` 加新文件」的改动 | **需 owner 评审**（避免抬高 M-series 难度） |
| 启动 M-series（D5 = `Engine` struct → core） | **立即** |
| 端口动态化、删 legacy crate、`commands.rs` 拆分 | **顺手做掉**（≤2 天小债清理） |

**为什么暂缓功能迭代：**
- 当前 `crates/tui` 承担了"运行时 + HTTP 服务端 + ratatui freeze + 工具实现"四种角色，每多写一行就让 M-series 重构更难；
- 持久化三套（Sessions / Runtime threads / `deepseek-state`）尚未合并，新功能落到哪一套都会成为后续债；
- HTTP 契约没有版本化策略，新端点一旦发出去就要长期兼容。

**冻结窗口预计长度：** M-series 7 PR + 持久化整合 ≈ **8-12 周**，期间产品壳层 UX 迭代不受影响（desktop crate 不变）。

---

## 1. 定型判定（满足后即可解冻功能迭代）

以下 10 条全部勾选 = 架构定型，可大胆做功能。**当前进度：4/10**（2026-05-25 D3 闭合 + M1 落地，但 Engine struct 主体尚未 in core，第 4 项仍 `[ ]`）。

- [x] **L1 turn loop 在 core**（P2 PR6 / G3 已闭合，见 [P2_G3_ENGINE_L2_SIGNOFF.md](./P2_G3_ENGINE_L2_SIGNOFF.md)）
- [x] **L2 契约稳定**（`/v1/*` 路由 + `event_schema_version: 2`，[`runtime_api/router.rs`](../../../crates/tui/src/runtime_api/router.rs)）
- [x] **桌面 ↔ sidecar 双通道安全模型**（Bearer 不出 WebView + path 白名单，[H06 完成](./IMPLEMENTATION_SUMMARY_2026-05-24.md)）
- [ ] **Engine struct 在 core**（M-series 进行中：M1 已落地 — `Op` / `EngineHandle` / `ThreadContextSnapshot` 入核 + `impl TurnEnginePort for EngineHandle<P,R>` core 侧实现；M2→M8 待启动，见 [`PR_M0_ENGINE_STRUCT_TO_CORE_SPIKE.md`](./PR_M0_ENGINE_STRUCT_TO_CORE_SPIKE.md) §6）
- [ ] **sidecar 二进制不再链接 ratatui / CLI**（M-series 完成的副产品；可选独立成 `crates/runtime-server`）
- [ ] **持久化单库**（Sessions + Runtime threads → 单 SQLite + 视图，[`BACKLOG_RUNTIME_UNIFICATION.md`](./BACKLOG_RUNTIME_UNIFICATION.md)）
- [ ] **`deepseek-state` / `app-server` 实验路径决策**（晋升或下线，二选一）
- [x] **`crates/tui-core` legacy 删除**（**2026-05-25 完成**，`cargo check --workspace --all-targets` 全绿）
- [ ] **HTTP 契约 OpenAPI 自动生成 + TS 类型自动产出**（消除手写 30+ interface 的飘移源）
- [ ] **关键巨型文件拆到软上限 1000 行内**（`config.rs` 3.5k+ 行、`compaction.rs`、`mcp.rs`、`client.rs`、`commands.rs(desktop)`、`localization.rs`）

---

## 2. 当前架构的优点（值得保留 / 推广）

### 2.1 进程边界划得很硬

- [`crates/desktop`](../../../crates/desktop/) **只依赖 `config + secrets`**，不依赖 `core / tui`。
  - 桌面壳 OTA 升级 sidecar 二进制时不需要重链整个 tui；
  - sidecar crash 不会拖死 Tauri 主进程；
  - Zagens v0.4.x 与 workspace v0.8.15 两条独立 SemVer 已经在 [CHANGELOG.md](../../../CHANGELOG.md) 体现。
- [`crates/desktop/build.rs`](../../../crates/desktop/build.rs) 自动从 `target/` 复制 sidecar 二进制到 `binaries/`，解决跨工程构建顺序——简单但极其有效。

### 2.2 双通道 + Bearer 注入是 Tauri 桌面安全的优等解

- token 仅在 Rust 进程注入（[`runtime_proxy.rs`](../../../crates/desktop/src/runtime_proxy.rs)），WebView 拿不到，DevTools 也拿不到；
- 路径白名单 `validate_runtime_path` 拒绝 `..` / 非 `/v1` 前缀，配合 sidecar 端 [`auth::require_runtime_token`](../../../crates/tui/src/runtime_api/auth.rs) 中间件——**双重防御**；
- CORS 白名单含 `http(s)://tauri.localhost`，对齐 Tauri 2 WebView2 实际 origin（注释里有写历史伤痕）。

### 2.3 `TurnEnginePort` 切口干净

- [`RuntimeThreadManager::start_turn`](../../../crates/tui/src/runtime_threads/manager.rs) 通过 `StartTurnParams` 显式校验后才发 `Op::SendMessage`；
- **核心校验在 core，副作用在 tui**——这是 M-series 重构的唯一支点，价值很高。

### 2.4 事件 broadcast 设计成熟

- `tokio::sync::broadcast::Sender<RuntimeEventRecord>` 同时喂 SSE handler 与持久化 monitor；
- `event_coalesce` 对 `item.delta` 合并；`RecvError::Lagged` 触发客户端 `since_seq` 回放（B3.3）；
- 配合 `EVENT_CHANNEL_CAPACITY` + LRU `ActiveThreads` + `spawn_blocking` 异步落盘——三个常见性能坑都关掉了。

### 2.5 Supervisor 协议是工程派

`DS_PICK_READY {port, pid, token_fp, version}` 行协议握手 + stdin `op: ping/drain` 心跳 + 退避（`MAX_RAPID_RESTARTS=3 / RAPID_RESTART_WINDOW_SECS=60`）+ Windows `EADDRINUSE` 端口让出循环（`PORT_FREE_POLL_MS`）——这些细节都来自真实事故复盘，**应当作为内部 SDK 模板沉淀**。

### 2.6 CLI / 桌面 / TUI 共享同一 sidecar 语义

`deepseek` 命令通过 `delegate_to_tui` 转调同一 `deepseek-tui` 二进制（CLI 内置命令仅做 config/auth/sandbox），**避免 CLI/Desktop 行为漂移**——这是很多 Agent 工具栈最终崩盘的来源。

---

## 3. 问题与债务（按严重度排序）

### 3.1【高】`crates/tui` 是事实上的胖 crate（最大债）

`tui` 顶层 80 个文件，里面同时塞了：

| 类别 | 代表文件 | 量级 |
|---|---|---|
| Engine 运行时 | `core/engine.rs` + `core/engine/` 子模块 | 209 + ~5.0k |
| HTTP 服务端 | `runtime_api/*` (16 个文件) | ~200k |
| 线程管理 | `runtime_threads/*` (16 个文件) | ~280k |
| ratatui UI（**已 freeze**） | `tui/*` | 大量 |
| 工具实现 | `tools/*` | ~28k LOC / ~55 files |
| 巨型单文件 | `config.rs` **195k** · `compaction.rs` **97k** · `mcp.rs` **76k** · `client.rs` **71k** · `task_manager.rs` **67k** · `localization.rs` **94k** · `prompts.rs` **51k** | 远超软上限 1000 行 |

**症结：**
- sidecar 二进制 = ratatui + 工具 + 服务端的合体 → 体积虚胖、冷启动慢、攻击面变宽；
- "新功能往哪放"在 tui crate 内部已经不可判断；
- core 已经准备好（P2 PR6 闭合），但 Engine struct 没真正搬过去。

**已记账：** [`BACKLOG_ENGINE_STRUCT_IN_CORE.md`](./BACKLOG_ENGINE_STRUCT_IN_CORE.md) + [`PR_M0_ENGINE_STRUCT_TO_CORE_SPIKE.md`](./PR_M0_ENGINE_STRUCT_TO_CORE_SPIKE.md)（M1→M8 七 PR 计划）。

### 3.2【高】持久化三套并存

| 轨 | 位置 | 用途 |
|---|---|---|
| Sessions | `~/.deepseek/sessions/` | 桌面/TUI 会话恢复 |
| Runtime threads | `~/.deepseek/tasks/runtime/` | HTTP `/v1/threads/*` |
| `deepseek-state` SQLite | `crates/state/` | CLI `thread list` / `app-server` |

边界靠注释维护、靠 ADR 维护，没物理隔离。**实际故事**：桌面看到的 thread 与 CLI `thread list` 不一致已经发生过，是双 schema 飘移的经典症状。

**已记账：** [`BACKLOG_RUNTIME_UNIFICATION.md`](./BACKLOG_RUNTIME_UNIFICATION.md) + [`BACKLOG_STATESTORE_JSONL.md`](./BACKLOG_STATESTORE_JSONL.md)。

### 3.3【中】~~端口 7878 写死~~ ✅ **2026-05-25 完成（基础设施部分）**

桌面端口管理已从 `u16` 升级为 `tokio::sync::watch::channel::<u16>`：

- **Sidecar**（[`runtime_api/mod.rs`](../../../crates/tui/src/runtime_api/runtime_api/mod.rs)）：`DS_PICK_READY.port` 现在报告 `listener.local_addr().port()`——**实际绑定**端口而非请求端口；
- **Desktop 壳**：`AppContext::runtime_port` 改为 `watch::Receiver<u16>`，supervisor 持 `Sender`；所有 IPC handler（`runtime_proxy::{http,post_stream,get_sse}` + `commands::{export_thread_json, export_session_json, rebuild_symbol_index, read_thread_workspace_binary}`）通过 `AppContext::require_port()` 读真实端口；
- **`get_runtime_port`**：改为 `rx.changed().await` 等待第一次发布——web-ui `initRuntimeConfig` 调用时自然阻塞到 sidecar 就绪；
- **重启路径**：spawn 新 sidecar 前 `port_tx.send(0)`，IPC handler 期间 fast-fail 或 await 新发布，杜绝拿到 stale 端口；
- **回归测试**：`sidecar_contract_full_lifecycle` ✅；`runtime_proxy::tests` ✅；`desktop::architecture_boundary` ✅。

**遗留（独立 PR 即可完成）：** 默认初始端口仍传 `7878`（向后兼容 / 易于 `curl localhost:7878` 调试）；让 sidecar 接受 `--port 0` 仅需移除 [`runtime_api/mod.rs` 中 `if options.port == 0 { bail!(...) }`](../../../crates/tui/src/runtime_api/mod.rs) 一行。基础设施已就绪。

### 3.4【中】Engine struct 与工具实现耦合在 tui

- `mcp.rs` 2.2k 行、`tools/*` 28k、`lsp/*` 1.3k、`sandbox/*` 2k 行全部和 Engine 同 crate；
- 想做"sidecar 不带 ratatui"必须先把这些 trait 化；
- `SandboxBackend` 已经是 `dyn trait` ✅；其余（MCP / LSP / Subagent / Shell / Seam / Cycle）需要补 trait 边界——这是 M-series 的核心工作量。

### 3.5【中】`commands.rs` (desktop) 49k 单文件

至少 7 类混在一起：API key / Vision 桥 / 系统设置 / 符号索引 / 终端 PTY / 窗口管理 / 文件二进制读取。应当拆 `commands/{auth,vision,settings,symbol_index,terminal,workspace}.rs`。

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

### 3.9【低】`crates/app-server` + `deepseek-state` 实验路径长期残留

存在 ≥ 半年，没有"晋升或下线"的明确决策。是认知带宽税。

### 3.10【低】HTTP 契约没有版本化策略 / 自动 ts 类型

- 30+ 个 `/v1/*` 端点，desktop ts 类型靠手写 + `streamNormalize.ts` 防护性兼容；
- 已有 `schemars` 但没生成 OpenAPI；
- 未来想 break change 时没有 `/v2` 路径策略。

---

## 4. 评分汇总

| 维度 | 评分 | 简评 |
|---|---|---|
| 分层与边界（L1/L2/L3） | ★★★★☆ | 三层模型清晰；唯一缺口是 Engine struct 还留在 tui |
| 进程模型 / 安全 | ★★★★★ | 每次 UUID Bearer + 路径白名单 + token 不出 WebView，业界水平 |
| 通信契约（HTTP+IPC 双通道） | ★★★★☆ | 切分干净；缺少版本化策略与 OpenAPI 生成 |
| 持久化 | ★★★☆☆ | 三套并存，需要长期合并 |
| 可观测性与错误处理 | ★★★★☆ | `ErrorEnvelope` + `tracing` + supervisor.log 完备；缺指标化 |
| 并发 / 取消 / 背压 | ★★★★☆ | `broadcast + coalesce + Lagged catch-up` 成熟；取消两层需文档化 |
| 测试基线 | ★★★★☆ | `runtime_api/tests.rs` 76k、`runtime_threads/tests.rs` 103k，回归网密 |
| **代码物理组织** | **★★☆☆☆** | **巨型文件 + tui crate 过载是最大债** |

---

## 5. 迭代方向（按优先级）

### P0 · 0-2 个月（结构定型期，必须做）

| ID | 内容 | 工作量 | 已记账 |
|----|------|--------|--------|
| **D1** | 巨型文件拆分（`config.rs` / `compaction.rs` / `mcp.rs` / `commands.rs` / `localization.rs`），按 [code-organization](../../../.cursor/rules/code-organization.mdc) 软上限 1000 行 | 每个文件 1 PR × 5-7 个 | — |
| **D2** | 端口动态化：桌面消费 `DS_PICK_READY {port}`，去掉 7878 写死 | ≤2 天 | ✅ **基础设施完成 2026-05-25**（剩 sidecar `--port 0` 一行） |
| **D3** | 删 `crates/tui-core` legacy | ≤0.5 天 | ✅ **完成 2026-05-25** |
| **D4** | 决策 `crates/app-server + deepseek-state`：晋升或 `#[deprecated]` | 决策 0.5 天 + 执行 1-2 天 | — |
| **D5** | **Engine struct → core（M-series M1→M8）** | 4-6 周 / 1 人 | 🟡 **进行中**：[`PR_M0_*`](./PR_M0_ENGINE_STRUCT_TO_CORE_SPIKE.md) — M1（`Op`/`EngineHandle`/`ThreadContextSnapshot` 入核 + `impl TurnEnginePort` core 端泛型化）**已落地 2026-05-25**，net +99 LOC、§7.4 8/8 测试 + sidecar contract 全绿；M2–M8 排队中 |

### P1 · 3-6 个月（解锁未来）

| ID | 内容 | 工作量 | 已记账 |
|----|------|--------|--------|
| **D6** | 抽 `crates/runtime-server`（sidecar 不再链 ratatui / CLI） | 2-3 周（D5 完成后） | 隐含在 M-series §1.1 invariants 之外 |
| **D7** | 持久化整合 Sessions ⊕ Runtime threads → 单 SQLite + 视图 | 4-6 周 | ✅ [`BACKLOG_RUNTIME_UNIFICATION.md`](./BACKLOG_RUNTIME_UNIFICATION.md) · [`BACKLOG_STATESTORE_JSONL.md`](./BACKLOG_STATESTORE_JSONL.md) |
| **D8** | OpenAPI schema 导出 + `web-ui` ts 类型自动生成 | 1-2 周 | — |
| **D9** | 取消/打断两层契约文档化 + `api/client.ts` 统一封装 | 3-5 天 | — |
| **D10** | 跨窗口事件按 thread owner 过滤（消除"幽灵渲染"） | 1 周 | — |

### P2 · 6-12 个月（产品形态升级）

| ID | 内容 | 工作量 | 已记账 |
|----|------|--------|--------|
| **D11** | `/v1/metrics`（Prometheus 兼容）+ 桌面运行时健康分页 | 2-3 周 | — |
| **D12** | 每 workspace 一个 sidecar（多 sidecar 隔离） | 6-8 周 | — |
| **D13** | Capability Manifest（合并 `sandbox` / `execpolicy` / `network_policy` / `command_safety`） | 4-6 周 | ✅ [`BACKLOG_LANDLOCK_ENFORCE.md`](./BACKLOG_LANDLOCK_ENFORCE.md) |
| **D14** | MCP 池稳定性增强（健康检查 + 背压 + 子进程纳入 supervisor） | 2-3 周 | — |

---

## 6. "如果只能做一件事"

**做 D5（Engine struct → core，M-series）。**

理由：

1. 这是 P2 最后一步，做完之后 `crates/tui` 真正瘦身为 "ratatui freeze 维护态"，**整个仓库的结构叙事终于和文档一致**；
2. **D1（拆巨型文件）、D7（持久化整合）、D11（观测）都会自然变容易**——因为有了边界清晰的"运行时服务"crate，新代码不会再无脑塞 tui；
3. 桌面用户、CLI 用户、未来的 IDE 插件用户（如果有）都消费同一 `runtime-server` 二进制 / 同一 OpenAPI，**长期摩擦最小**；
4. **已有完整 spike**：[`PR_M0_ENGINE_STRUCT_TO_CORE_SPIKE.md`](./PR_M0_ENGINE_STRUCT_TO_CORE_SPIKE.md) 已经给出 35 字段所有权表、12 风险、M1→M8 七 PR 序列、§1.1 硬不变式（无 `/v1` breaking、tools 不动、每 PR ≤700 行净），**工作量是确定的，不会陷"再设计一轮"**；
5. 关键切口 `TurnEnginePort` 已经存在。

---

## 7. 功能冻结期的纪律（执行守则）

为了让定型期不被反复打断，建议团队遵守：

### 7.1 PR 准入红线

- ⛔ **禁止**在 `crates/tui` 新建顶层文件（子模块拆分除外）
- ⛔ **禁止**新增 `/v1/*` 端点，除非配套补 OpenAPI schema 草案
- ⛔ **禁止**让 `desktop` crate 直接 `use deepseek_core` 或 `deepseek_tui`——它只能消费 HTTP + IPC
- ⛔ **禁止**给 `Engine` struct 加新字段（M-series 35 字段表已封口）
- ⚠ **谨慎**：往 `commands.rs`、`config.rs`、`compaction.rs` 等巨型文件加代码——必须同 PR 把该文件继续拆小

### 7.2 允许且鼓励

- ✅ 桌面 `web-ui/` 内部 UX 改进、a11y、i18n
- ✅ bug 修复、性能优化、错误信息改善
- ✅ 测试补充（`runtime_api/tests.rs`、`runtime_threads/tests.rs`、`core/engine/tests.rs`）
- ✅ 文档：本文件、`RUNTIME_EVOLUTION_ROADMAP.md`、`API_DESIGN.md`、ADR
- ✅ M-series PR（M1→M8 是最高优先级）

### 7.3 评审升级路径

任何看起来不属于 7.2 的 PR：
1. 在描述里指出"是否触发 7.1 红线"；
2. 如果触发，在 PR 顶上贴 `[ASSESS-2026-05-25 §1 未勾选项: …]`；
3. 由架构 owner 决定接受、改写或推迟到定型后。

---

## 8. 重新评估时点

- **M-series M3 合并后**：重新评估 §1 第 4-5 项是否可勾；
- **M-series M8 合并后**：本文档升级为 **`ARCHITECTURE_ASSESSMENT_<date>.md` 第二版**，重新跑 §1 checklist；
- **§1 ≥ 8/10 勾选时**：解除 §7.1 全部红线；
- **§1 = 10/10 时**：架构定型，本文档归档为历史快照。

---

## 9. 参考与交叉链接

- 系统架构图：[`RUNTIME_ARCHITECTURE.md`](../RUNTIME_ARCHITECTURE.md)
- 长期演进：[`RUNTIME_EVOLUTION_ROADMAP.md`](../RUNTIME_EVOLUTION_ROADMAP.md)
- HTTP/IPC 契约：[`API_DESIGN.md`](../API_DESIGN.md)
- 实施快照：[`IMPLEMENTATION_SUMMARY_2026-05-24.md`](./IMPLEMENTATION_SUMMARY_2026-05-24.md)
- M-series spike：[`PR_M0_ENGINE_STRUCT_TO_CORE_SPIKE.md`](./PR_M0_ENGINE_STRUCT_TO_CORE_SPIKE.md)
- 持久化合并 backlog：[`BACKLOG_RUNTIME_UNIFICATION.md`](./BACKLOG_RUNTIME_UNIFICATION.md) · [`BACKLOG_STATESTORE_JSONL.md`](./BACKLOG_STATESTORE_JSONL.md)
- 沙盒增强 backlog：[`BACKLOG_LANDLOCK_ENFORCE.md`](./BACKLOG_LANDLOCK_ENFORCE.md)
- 阻塞 I/O 审计：[`A1_PERSIST_BLOCKING_AUDIT.md`](./A1_PERSIST_BLOCKING_AUDIT.md)
- 产品战略：[`../../desktop/DEV_NOTES.md`](../../desktop/DEV_NOTES.md)（D12 Desktop-only）
