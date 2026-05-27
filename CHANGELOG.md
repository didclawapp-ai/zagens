# Changelog

All notable changes to **Zagens** and its embedded runtime will be documented in this file.

**Update policy:** Record **every notable change** (features, fixes, docs, Zagens desktop, runtime, tooling) in this file—typically under `[Unreleased]`, in the **same PR/commit** as the change when practical. Cursor agents: see `.cursor/rules/zagens-repo.mdc` § Changelog.

**Licensing:** Zagens (desktop app in `crates/desktop/`) is **proprietary** — see [LICENSE](LICENSE). Third-party runtime MIT license: [third-party/deepseek-tui/LICENSE](third-party/deepseek-tui/LICENSE) and [NOTICE.md](NOTICE.md).

**Zagens** (desktop app in `crates/desktop/`) has its **own** version line:
**MAJOR.MINOR.PATCH** in **SemVer** (e.g. **v0.5.0**). Display form **vX.Y.Z**;
each numeric segment is one or more digits (e.g. `0.2.1`, `0.10.3`). This line
**does not** follow the embedded runtime workspace version in root `Cargo.toml`
`[workspace.package] version`.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

> **2026-05-19:** `[0.3.0]` consolidates work since v0.2.2 (backfilled from `git log`).
> Prefer updating `[Unreleased]` incrementally going forward.

## [Unreleased]

### Architecture

- **Docs (D17 修订):** [`D17_ARCHITECTURE_FREEZE.md`](docs/tech/adr/D17_ARCHITECTURE_FREEZE.md) 与实现对齐 — Turn 链（orchestrator → `TurnEnginePort` → sidecar `dispatch_op` → `handle_deepseek_turn`）、SubAgent 锚点路径、`I1`/边界测试范围、`I7`/F2 去 ratatui 误述、OpenAPI 护栏（脚本 + `ci.yml`）、持久化默认路径与环境变量覆盖说明。
- **D17 (Landed):** Architecture Freeze v1 — 重构主线关闭；D16 Closed (Checkpoint)；明确 **不执行** E1 阶段 2 / E4 / runtime-server <500 行 KPI / Harness 分离。见 [`docs/tech/adr/D17_ARCHITECTURE_FREEZE.md`](docs/tech/adr/D17_ARCHITECTURE_FREEZE.md)。
- **D17 F1/F2 (Landed):** stale `deepseek-tui` 生产注释清理（core/runtime-server shim/config）；headless `CLIENT_IDENTITY_HEADLESS` 替代 TUI 文案；`architecture_boundary` 补 `deepseek-core` 检查；`scripts/check-architecture-freeze.{ps1,sh}`。
- **D15 (Landed):** Final architecture convergence — removed `deepseek-state` crate and legacy `core::Runtime` / `ThreadMessageTurnPort`; Zagens Desktop is the sole user entry; sidecar spawn unified to `deepseek-runtime` only. Session remains a projection of `RuntimeThreadStore` (D7 `runtime_thread_id` link). See [`docs/tech/adr/D15_FINAL_ARCHITECTURE_CONVERGENCE.md`](docs/tech/adr/D15_FINAL_ARCHITECTURE_CONVERGENCE.md).
- **Docs (D16):** Phase E maintainability split plans — [`docs/tech/adr/D16_PHASE_E_MAINTAINABILITY.md`](docs/tech/adr/D16_PHASE_E_MAINTAINABILITY.md) (`runtime-server` crate、SubAgent、`App.tsx` hooks；不阻塞发布).
- **D16 E2 (Landed):** Split `tools/subagent/mod.rs` (~4340 行) into focused modules — `mod.rs` ~82 行、`manager.rs` / `executor.rs` / `tools/*` / `parse.rs` / `router.rs` / `prompts.rs` 等；108 个 subagent 单元测试全绿。
- **D16 E3-a (Landed):** Extract `hooks/useRuntimeConnection.ts` from `App.tsx` (Sidecar boot、probe、重连、runtime 状态).
- **D16 E3-b/c/d (Landed):** `useTurnSession` / `useTurnStream` / `useTurnApproval` / `useAgentPanelState` / `useTurnSend`；`AppShell` + `App.tsx` **776 行**。
- **D16 E1-a3–a6 (Landed):** tools host 端口、workspace_walk/arg_repair、network_gate；fetch_url/web_run/web_search 去重。
- **D16 E1-a7:** `skills/install.rs` 复用 `network_gate::check_host_with_policy` / `host_policy_decision`。
- **D16 E1-a8:** `tools/shell/tools.rs`（~1057 行）再拆为 `shell/tools/{exec,wait,cancel,note,helpers}.rs`；22 个 shell 单元测试全绿。
- **D16 E1-a8 (WIP):** `tools/file.rs`（~1991 行）模块内拆为 `file/{read,write,edit,list_dir}.rs` + `tests.rs`；`sniff_encoding_label` 仍由 `file_info` 复用；30 个 file 单元测试全绿。
- **D16 E1-a8 (WIP):** `tools/web_run.rs`（~1638 行）模块内拆为 `web_run/{types,state,tool,search,page,html}.rs` + `tests.rs`；11 个 web_run 单元测试全绿。
- **D16 E1-c6:** task wire 类型（`TaskRecord`、`TasksResponse`、`NewTaskRequest` 等）迁入 `runtime-api/src/task.rs` 并加入 `SCHEMA_EXPORTS`；sidecar `runtime_api/openapi.rs` 瘦身为 re-export；`task_manager` 删除重复 struct；`check-openapi-contract` + `runtime_api`/`task_manager` 回归全绿。
- **D16 E5 (Landed):** OpenAPI + TS 契约重新对齐 E1-c；CI `generate:api-types` diff gate；`check-openapi-contract.{sh,ps1}`。
- **D16 E1-b (WIP, phase 1):** 新建 `crates/runtime-orchestrator` — 迁入 `runtime_threads/{types,persist}`、`thread_store_sqlite`、`pricing`（usage 聚合）；`RuntimeThreadManager` 等 live orchestration 仍留 `runtime-server`；40 个 `runtime_threads` 单元测试全绿。
- **D16 E1-b (WIP, phase 2):** 迁入 `runtime_threads/{routing,events,event_coalesce}` 至 orchestrator；新增 `engine`/`engine_host`（`EngineHandle<P,R>` + `RuntimeThreadHost` trait）；`active`/`turn_wait`/`turn_control`/`turn_lifecycle` 核心迁入 orchestrator；`RuntimeThreadManager<P,R>` 核心 + `thread_crud` 在 orchestrator；sidecar `Deref` 包装实现 host（`engine_load`/`monitor`/`prepare_start_turn_params`）；server 保留 Config/task/scratchpad 与 symbol index hook。
- **D16 E1-b (WIP, phase 3):** `monitor_turn` 事件循环（~930 行）迁入 `runtime-orchestrator`/`monitor.rs`；新增 `RuntimeThreadMonitorHost`（panel SSE、artifact refs、全权限 sandbox policy）与 `monitor_persist` 阻塞落盘 helper；sidecar `monitor_host.rs` 实现 host hook；删除 `runtime-server`/`monitor.rs`。
- **D16 E1-b (WIP, phase 4):** `ensure_engine_loaded` 通用路径（缓存、session sync、LRU）迁入 orchestrator/`engine_load.rs`；`RuntimeThreadHost::spawn_engine_for_thread` 由 sidecar `engine_spawn.rs` 实现；`turn_lifecycle`/`turn_control` 直接调用 orchestrator `ensure_engine_loaded(mgr, host, …)`。
- **D16 E1-b (WIP, phase 5):** 新增 `RuntimeThreadTaskPort`（background task 最小 turn 面）与 `RuntimeThreadBackgroundSlots`（task/automation 注入 `RuntimeToolServices`）；`EngineTaskExecutor` 改走 task port。
- **D16 E1-c (WIP, phase 1):** 新建 `crates/runtime-api`（`deepseek-runtime-api`）；OpenAPI `paths`/核心 `schemas` 迁入；sidecar `runtime_api/openapi.rs` 合并 task  schema；`export-runtime-openapi` 行为不变。
- **D16 E1-c (WIP, phase 2):** `auth`/`health`/`cors`/`compose_router` 迁入 runtime-api；`RuntimeApiAuthState`/`RuntimeApiProbeState` host trait；sidecar `router.rs` 仅保留 `/v1/*` handler 接线；`/v1/*` bearer 中间件仍在 sidecar 挂载以满足 Axum 0.8 状态类型。
- **D16 E1-c (WIP, phase 3):** `ApiError` 与 `IntoResponse` 错误 envelope 迁入 runtime-api；handler 仍留 sidecar。
- **D16 E1-c (WIP, phase 4):** 共享 wire response（`SessionsListResponse`、`SessionDetailResponse`、`ResumeSessionResponse`、`StartTurnResponse`、`ThreadSummary`）由 runtime-api 导出；sidecar handler 删除重复 struct。
- **D16 E1-c (WIP, phase 5):** `StreamTurnRequest` 由 runtime-api 导出；`stream.rs` handler 复用 wire 类型（`workspace: Option<String>` → `PathBuf` 在 handler 内转换）。
- **D16 E1-d2:** `deepseek_runtime::run_http_server` crate 根 re-export；`RUNTIME_ARCHITECTURE` / D8 对齐 runtime-api OpenAPI SSOT 与 D16 crate 依赖图。
- **D16 E1-b (WIP, phase 6):** `task_manager.rs`（~1500 行）模块内拆为 `task_manager/{config,executor,manager,persist,helpers,tests}.rs`；wire 类型仍用 runtime-api；4 个 task_manager 单元测试全绿。
- **D16 E1-a8:** `skills/install.rs`（~1534 行）模块内拆为 `install/{types,api,local,registry,download,tests}.rs`；`pub mod skills` 供集成测试；16 单元测试 + `skill_install` 集成测试全绿。

### Changed

- **Docs / Harness 文档集：** 新建 [`docs/harness/`](docs/harness/README.md) — 迁入 [`Agent+Harness组合式编程方案.md`](docs/harness/Agent+Harness组合式编程方案.md)、[`HARNESS_INTEGRATION_PROPOSAL.md`](docs/harness/HARNESS_INTEGRATION_PROPOSAL.md)；新增 [`ANTHROPIC_MANAGED_AGENTS_AND_HARNESS.md`](docs/harness/ANTHROPIC_MANAGED_AGENTS_AND_HARNESS.md)（Managed Agents 时间线、官方 Engineering 文章、三模式与组合式方案对照）；`docs/tech/adr/HARNESS_INTEGRATION_PROPOSAL.md` 保留重定向 stub。
- **Docs / Harness v1.3：** [`Agent+Harness组合式编程方案.md`](docs/harness/Agent+Harness组合式编程方案.md) 增补 **阶段六「自适应主动 Harness」**（§3.4 定义、Manifest 一等公民、§10 路线图阶段六）；[`README.md`](docs/harness/README.md) 演进假设表；归并提案 §3 映射「自适应主动」行。
- **Docs：** [`docs/prompt-architecture.md`](docs/prompt-architecture.md) 对齐 D6（`crates/runtime-server` 路径、`task overlay`、Engine 模块拆分、`DEEPSEEK_CLIENT_SURFACE=zagens`）。
- **Zagens desktop / 图标资产：** 新增 `crates/desktop/icons/svg/` — 5 种 SVG 变体及 `preview.html`；神经网络另含 `variants/` 下 6 种配色 + `preview-palettes.html` 对比页（基准：暖白 + 琥珀）。

### Fixed

- **Runtime / prompts：** `DEEPSEEK_CLIENT_SURFACE=zagens`（sidecar 实际值）现与遗留 `ds-pick` 一并识别，恢复 Zagens 客户端身份与 `## Environment` 的 `ui_shell: Zagens (desktop)`；此前仅匹配 `ds-pick` 时桌面会话误用 “DeepSeek TUI” 身份文案。
- **Zagens desktop / CRAFT：** `GET /v1/blackboards` 支持 `?workspace=`（与 `/v1/workspace/browse` 一致）；AgentPanel 按当前 Composer 工作区拉取黑板，修复 D6 后 sidecar 默认 cwd（用户目录）与子 Agent 写入项目 `.deepseek/blackboards/` 不一致导致 CRAFT 任务列表为空。
- **Runtime：** 移除已删除 `eval.rs` 的孤儿集成测 `eval_harness.rs`（D6 迁移遗留，阻塞 `cargo test -p deepseek-runtime-server`）。

## [0.5.0] - 2026-05-26

### Zagens (desktop)

- **v0.5.0** — 架构升级里程碑：`deepseek-desktop`、`tauri.conf.json`、`web-ui/package.json` 与 About 面板对齐 **v0.5.0**。主线：D6 Phase B（`deepseek-runtime` 单 crate、移除 CLI/TUI）、M7/M8（Engine 入 core）、D1/D4/D7/D8/D9/D10 与 Assessment **10/10** 定型；含多窗口空白修复与会话侧栏就绪重载。

### Fixed

- **Zagens desktop / 多窗口：** 修复第二个（及后续）窗口空白——Windows 上在同步托盘/命令里创建 `WebviewWindow` 会触发 WebView2 死锁；`create_agent_window` 改为 `async`，托盘与单实例路径改 `spawn`；新建窗与主窗一致先 `visible(false)`，sidecar 已就绪时 `emit_to` 补发 `sidecar://ready`，前端启动门增加就绪探测。
- **Zagens desktop / 侧栏会话列表：** sidecar 就绪前 `GET /v1/sessions` 失败后于 `sidecar://ready` 自动重载；回合结束时在 `finishOnce` 兜底 `persist-session`（修复 SSE 事件异步过滤导致未写入）；工作区路径比较改为大小写/分隔符无关，避免会话被误过滤。

### Changed

- **Architecture / D6 Phase B 文档同步（2026-05-26）：** [`RUNTIME_ARCHITECTURE.md`](docs/tech/RUNTIME_ARCHITECTURE.md)、[`D6_IMPLEMENTATION_PLAN.md`](docs/tech/adr/D6_IMPLEMENTATION_PLAN.md)、[`D6_RUNTIME_SERVER.md`](docs/tech/adr/D6_RUNTIME_SERVER.md)、[`API_DESIGN.md`](docs/tech/API_DESIGN.md)、[`ARCHITECTURE_ASSESSMENT_2026-05-25.md`](docs/tech/adr/ARCHITECTURE_ASSESSMENT_2026-05-25.md)、[`DEV_NOTES.md`](docs/desktop/DEV_NOTES.md)、[`README.md`](README.md) 同步 Phase B 落地态（`deepseek-runtime` 单 crate；路径 `crates/runtime-server`）。
- **Architecture / D6 Phase A+ (2026-05-26):** `deepseek-runtime` binary sidecar contract test — [`sidecar_binary_contract.rs`](crates/runtime-server/tests/sidecar_binary_contract.rs)（spawn 真实 binary + `DS_PICK_READY` + health/thread/SSE/interrupt）；CI ubuntu job 新增 `cargo test -p deepseek-runtime-server --test sidecar_binary_contract`；D6 ADR acceptance 第 4 项 ✅。
- **Docs / D6 implementation plan (2026-05-26):** [`D6_IMPLEMENTATION_PLAN.md`](docs/tech/adr/D6_IMPLEMENTATION_PLAN.md) — Phase A 回顾清单、Phase A+ binary CI 契约测、Phase B `runtime_api`/`runtime_threads` 物理迁移 PR 链；配套 [`D6_RUNTIME_SERVER.md`](docs/tech/adr/D6_RUNTIME_SERVER.md)。
- **Docs / RUNTIME_ARCHITECTURE (2026-05-26):** [`RUNTIME_ARCHITECTURE.md`](docs/tech/RUNTIME_ARCHITECTURE.md) 与代码复核对齐 — 生产 sidecar 改为 **`deepseek-runtime`**（D6）；`Engine` + `op_loop` 落点改为 **core**（M-series）；移除已删 `app-server`；§1/§2/§3 依赖图与 §10 架构定型 **10/10** 叙事同步 Assessment。
- **Architecture / D1 (2026-05-26):** `config.rs` → `config/{mod,providers,types,load/}`；`load/` 再拆为 `impl_config`、`paths`、`env_overrides`、`model`、`merge`、`credentials`（实现均 ≤644 行；`tests.inc.rs` ~1.8k 经 `load/mod.rs` include）。脚本 [`split-config-load-rs.py`](scripts/split-config-load-rs.py) 按函数边界切片；`config::load::` **78** 项测试通过。
- **Architecture / D1 (2026-05-26):** `compaction.rs`（~2.7k 行）→ `compaction/{plan,tokens,prune,execute,prompt}.rs` + `tests.inc.rs`（实现 ≤589 行；[`split-compaction-rs.py`](scripts/split-compaction-rs.py)）；`compaction::` **54** 项测试通过。
- **Architecture / D1 (2026-05-26):** `client.rs`（~2.1k 行）→ `client/{tool_names,types,http,client_impl,llm,api_parse,fim}.rs` + 既有 `chat.rs`（[`split-client-rs.py`](scripts/split-client-rs.py)）；`client::` **88** 项测试通过。
- **Architecture / D1 (2026-05-26):** `mcp.rs`（~2.2k 行）→ `mcp/{diagnostics,config,types,transport,connection,pool,config_io,format}.rs`（[`split-mcp-rs.py`](scripts/split-mcp-rs.py)）；`mcp::` **15** 项测试通过。
- **Docs / D1 scope (2026-05-26):** [`ARCHITECTURE_ASSESSMENT_2026-05-25.md`](docs/tech/adr/ARCHITECTURE_ASSESSMENT_2026-05-25.md) — **`tui/ui.rs` 不纳入 D1 拆分**（TUI-only，与 Zagens 桌面无关）；`ui.rs` 模块注释交叉引用。
- **Docs / D1 scope (2026-05-26):** Assessment §5.1 — 扩充「**D1 明确不拆分**」表（ratatui TUI、`localization.rs`、`tools/*`、`legacy.rs`、测试卷等）；§1 #10 收窄为 **`desktop/commands.rs`**（runtime 四模块 `config`/`compaction`/`mcp`/`client` 已拆 ✅）；`localization.rs` 模块注释交叉引用。
- **Architecture / D1 closed (2026-05-26):** **`desktop/commands.rs` 不拆分**（~1.5k，维护者决定；避免碎片化）；Assessment §1 **10/10** 架构定型；`commands.rs` 模块注释。
- **Architecture / D8 (2026-05-26):** OpenAPI 3.1 导出 + `web-ui` TS 自动生成 — [`D8_OPENAPI_TS_GENERATION.md`](docs/tech/adr/D8_OPENAPI_TS_GENERATION.md)；`export-runtime-openapi` + [`zagens-runtime-v1.openapi.json`](docs/tech/openapi/zagens-runtime-v1.openapi.json)；`openapi-typescript` → `web-ui/src/api/generated/runtime-api.ts` + [`runtimeTypes.ts`](crates/desktop/web-ui/src/api/runtimeTypes.ts)；`/v2` 草案 [`V2_API_VERSIONING.md`](docs/tech/adr/V2_API_VERSIONING.md)。Assessment §1 **9/10**。
- **Docs / Harness integration proposal (2026-05-26):** [`HARNESS_INTEGRATION_PROPOSAL.md`](docs/tech/adr/HARNESS_INTEGRATION_PROPOSAL.md) — 把 [`docs/Agent+Harness组合式编程方案.md`](docs/Agent+Harness组合式编程方案.md) v1.2 远景方案落到 Zagens 现状：名词映射表（黑板/任务图/决策日志/笔记 → scratchpad + CRAFT + plan/todo/tasks + topic-memory + execpolicy 等已有承载）、Phase 0–3 全部搭车 D6/D7/D8/D13（**零新主线**、零 Engine 字段新增、零新持久化轨、零新顶层 `/v1/*` 路径），§11–§12 数学基础 11 个模型降级/删除清单（粗糙集 → TOML 规则表；贝叶斯/SPRT/指数衰减 → 删除；可能性论 → UI 文案标签；进化引擎 → 延期 v1.0+）。Status: **Proposed**；§1 不变（7/10）。
- **Architecture / D7 complete (2026-05-26):** [`D7_PERSISTENCE_UNIFICATION.md`](docs/tech/adr/D7_PERSISTENCE_UNIFICATION.md) — C1 `runtime_thread_id` SQLite；C2 [`PERSISTENCE.md`](docs/tech/PERSISTENCE.md)；C3 resume 集成测；C4 `deepseek thread list --source runtime`；C5 删除 `app-server`；§1 **8/10**。
- **Architecture / D9 + D10 (2026-05-26):** [`D9_D10_DESKTOP_UX.md`](docs/tech/adr/D9_D10_DESKTOP_UX.md) — `turnControl.ts` 两层 Stop 契约；`filterThreadStreamEvents` + `windowOwnsThreadForStream` 消除多窗口幽灵 SSE；API_DESIGN §2.1.1–2.1.2。§1 仍 **7/10**（体验债）。
- **Architecture / D6 (2026-05-26):** [`D6_RUNTIME_SERVER.md`](docs/tech/adr/D6_RUNTIME_SERVER.md) — 新增 `crates/runtime-server` + 二进制 **`deepseek-runtime`**（不链 ratatui）；`deepseek-tui` 特性 **`tui-ui`** 门控 TUI 栈；共享模块 `agent_surface` / `auto_route` / `context_reference` / `runtime_serve`；Zagens `externalBin` → `deepseek-runtime-*`；Assessment §1 #5 勾选（**7/10**）。
- **Architecture / D4 (2026-05-26):** [`app-server` 实验栈标记 deprecated](docs/tech/adr/D4_APPSERVER_DEPRECATED.md) — 决策 ADR、Assessment §1 #7 勾选；`deepseek app-server` CLI help、`deepseek-app-server` crate 文档 + `#[deprecated]` on `run`/`run_stdio`；**crate 代码移除 defer**。
- **Docs / architecture assessment (2026-05-26):** [`ARCHITECTURE_ASSESSMENT_2026-05-25.md`](docs/tech/adr/ARCHITECTURE_ASSESSMENT_2026-05-25.md) M7/M8 复评 + D4 — 进度 **6/10**；D5 ✅；下一优先 D6 `runtime-server`。
- **Docs / architecture assessment (2026-05-26):** [`ARCHITECTURE_ASSESSMENT_2026-05-25.md`](docs/tech/adr/ARCHITECTURE_ASSESSMENT_2026-05-25.md) 新增 **§5.1 推荐实施顺序**（维护者签收）— 主线 D6→D9/D10→D7→D8→D1→P2；§0 摘要与冻结窗口估算同步；§5 表注明「已记账 ≠ 已落地」。[`RUNTIME_EVOLUTION_ROADMAP.md`](docs/tech/RUNTIME_EVOLUTION_ROADMAP.md) 头部交叉引用同步（6/10 + §5.1）。

- **Runtime / M-series M8 (PR_M0 §6 M8):** Final strangler step — core **`Engine::run()`** op loop lands in `deepseek-core::engine::op_loop` (cancel / approve / deny / truncate handled core-side; platform ops via `EnginePlatformExt`). Tui `EngineRuntimeExt` implements dispatch in `platform_dispatch.rs`; `op_loop.rs` + `op_handlers.rs` deleted from tui. **`Engine::ext`** is now `Box<dyn EnginePlatformExt<P,R>>` (was `Box<dyn Any>`). Pre-existing engine integration tests **`refresh_system_prompt_under_capacity_omits_topic_memory_block`** (3× `on_turn_complete` fixture) and **`engine_mock_capacity_pre_request_observes_mock_and_emits_decision`** green after partition-trim bulk fast-path. Closes [BACKLOG_ENGINE_STRUCT_IN_CORE.md](docs/tech/adr/BACKLOG_ENGINE_STRUCT_IN_CORE.md); deletes [HANDOFF_M7_M8.md](docs/tech/adr/HANDOFF_M7_M8.md).

- **Runtime / M-series M7 (PR_M0 §6 M7):** Seventh strangler step — `Engine` struct + `Engine::with_hosts` + tui `build_engine` builder land in `deepseek-core`; tui keeps a **newtype wrapper** (`#[repr(transparent)] Engine(pub(crate) core::Engine<…>)`) so inherent impls / `TurnLoopHost` remain legal. Host fields swap to trait objects; concrete handles + `EngineConfigExt` live in `EngineRuntimeExt` behind `EnginePlatformExt`. `engine_new.rs` deleted; `spawn_engine` → `build::build_engine`. Shim split: `engine.rs` (~130 LOC) + `prelude_uses.rs` include.

- **Runtime / M-series M6 (PR_M0_ENGINE_STRUCT_TO_CORE_SPIKE §3 rows #20 + #22, §5 R10, §6 M6; ARCHITECTURE_ASSESSMENT §1 #4, §3.4):** Sixth strangler step — `CapacityController` (677-LOC body) + the M1-deferred coherence reducer (`CoherenceSignal` + `next_coherence_state`) move atomically into `deepseek-core` per spike R10 ("single atomic move, delete tui copy in same PR; no double-implementation period"). Zero behavior change — pure type move.
  - **`crates/core/src/capacity.rs`** (41 → 706 LOC): `CapacityControllerConfig` (already there since P2 PR4) is joined by the full controller surface — `GuardrailAction` (4 variants), `RiskBand` (3 variants), `CapacityObservationInput`, `DynamicSlackProfile`, `CapacitySnapshot`, `CapacityDecision`, `GuardrailRuntimeState` (private), `CapacityController` (with `new`, `observe_pre_turn`, `observe_post_tool`, `decide`, `mark_turn_start`, `mark_intervention_applied`, `mark_replay_failed`, `last_snapshot`, private `observe` + `model_prior`), `decide_policy(config, snapshot) -> GuardrailAction` free fn, and the math/window helpers (`normalize_model_prior_key`, `log2_1p`, `push_window`, `compute_profile`, `sigmoid`). 12 unit tests + 1 `#[ignore]` microbench (`bench_compute_profile`) move with the body — only the tui-`Config`-coupled `app_config_without_capacity_uses_default_disabled` test stays tui-side along with its adapter.
  - **`crates/core/src/coherence.rs`** (39 → 157 LOC): `CoherenceState` ladder enum (P2 PR4) is joined by `CoherenceSignal` enum (5 variants) + `next_coherence_state(current, signal) -> CoherenceState` reducer + the `synthetic_capacity_event_log_drives_plain_language_ladder` log-replay unit test. The reducer references `super::capacity::{GuardrailAction, RiskBand}` locally — this dependency is exactly why M1 (spike row #22) deferred the reducer: it could only land after `capacity::{GuardrailAction, RiskBand}` themselves landed in core, which happens in this same commit.
  - **`crates/tui/src/core/capacity.rs`** (677 → 102 LOC, net −575 LOC): pure re-export shim — `pub use deepseek_core::capacity::{CapacityController, CapacityControllerConfig, CapacityDecision, CapacityObservationInput, CapacitySnapshot, DynamicSlackProfile, GuardrailAction, RiskBand, decide_policy};`. Keeps only `capacity_config_from_app(config: &crate::config::Config) -> CapacityControllerConfig` (the tui-side adapter that projects the flat `crate::config::Config` onto the core controller config — stays tui because the type cannot cross the layering boundary) and its single unit test.
  - **`crates/tui/src/core/coherence.rs`** (102 → 14 LOC, net −88 LOC): pure re-export shim — `pub use deepseek_core::coherence::{CoherenceSignal, CoherenceState, next_coherence_state};`. All 15 call sites under `crates/tui/src/core/engine/capacity_flow/*`, `crates/tui/src/runtime_threads/*`, `crates/tui/src/tui/ui*`, `crates/tui/src/tui/widgets/mod.rs`, `crates/tui/src/cli/commands/legacy.rs`, and the engine state (`tui::core::engine::types::EngineConfig`) keep compiling unchanged — **zero Engine call-site swaps required** (the type-move semantics let the shims handle the entire fan-out).
  - **Not in M6 scope (intentional):** `crates/tui/src/core/capacity_memory.rs` (286 LOC) — disk persistence (`save_metrics`, `load_metrics`, JSONL append fallback chain) is an engine-flow concern, not part of spike row #20's controller field, and uses no tui-only deps beyond `crate::config` paths from its callers — can opportunistically move later if M7/M8 needs it. `crates/tui/src/core/engine/capacity_flow/*` (5 files, ~1.3k LOC of engine-side checkpoints / replay / interventions / persistence / observation orchestration) stays tui until M7 (Engine struct migration) — they own `&mut Engine` state and depend on tui-side messaging plumbing. `crates/tui/src/core/engine/turn_loop/host_impl/capacity.rs` (turn-loop host impl, ~80 LOC) similarly stays until M7.
  - Net diff `git diff --stat HEAD~..HEAD`: 4 files (2 in core, 2 in tui), 858 insertions / 783 deletions — **~+75 LOC net** (cap ≤700; verbatim type move shifts code rather than adding it). Acceptance per spike §6 M6: `core --lib capacity` 11/11 + bench ignored 1, `core --lib coherence` 1/1, `core --lib engine::turn_loop::capacity_policy` 4/4, `tui --lib capacity_escalation` 2/2, `tui --lib coherence` (footer chip) 1/1, `tui --lib core::capacity_memory` 3/3, `tui --lib capacity_disabled_by_default_keeps_messages_intact` ok, `tui --lib seam_manager` 7/7 ok, `tui --lib mcp` 36/36 ok, `tui --lib tools::subagent` 108/108 ok, `tui --lib runtime_api::tests::sidecar_contract_full_lifecycle` ok, `tui --lib history_isomorphism` 9/9 ok, `tui --test protocol_recovery` 9/9 ok, `cargo build -p deepseek-{core,tui}` clean, `npm run test:f3 && npm run build` (web-ui) clean. The same 2 pre-existing `core::engine::tests::{refresh_system_prompt_under_capacity_omits_topic_memory_block, engine_mock_capacity_pre_request_observes_mock_and_emits_decision}` failures **persist with identical line numbers (tests.rs:991 / tests.rs:2452) and assertion text** as on M3/M4/M5 HEAD — confirmed unrelated to M6 (which is zero-behavior-change type move). I had hypothesized M6 might fix the `engine_mock_capacity_pre_request_observes_mock_and_emits_decision` failure since the test exercises the capacity decision path, but the persistence confirms the bug is in engine-flow wiring (`capacity_flow/observation.rs` or similar) rather than in `CapacityController` itself — that bug is M7 territory (Engine struct + engine-flow integration).
  - Promotes [BACKLOG_ENGINE_STRUCT_IN_CORE.md](docs/tech/adr/BACKLOG_ENGINE_STRUCT_IN_CORE.md) Progress table: M6 row `queued` → `landed`. M7–M8 remain queued per [PR_M0_ENGINE_STRUCT_TO_CORE_SPIKE.md](docs/tech/adr/PR_M0_ENGINE_STRUCT_TO_CORE_SPIKE.md) §6.

- **Runtime / M-series M5 (PR_M0_ENGINE_STRUCT_TO_CORE_SPIKE §3 rows #21 + #25 + #28-#31, §5 R1 + R9 + R12, §6 M5; ARCHITECTURE_ASSESSMENT §1 #4, §3.4):** Fifth strangler step — three subsystem host traits (`SeamHost` / `WorkshopHost` / `TopicMemoryHost`) plus the small `ScratchpadStepState` type migrate into `deepseek-core`. The heavy implementations stay tui-side: `seam_manager.rs` (712 LOC), `large_output_router.rs` (604 LOC), `topic_memory.rs` (307 LOC) and `scratchpad_flow.rs` (484 LOC of UI / auditor / coverage helpers) are **not** moved — spike §5 R12 ("scratchpad belongs UI-side") + R9 ("prefer adapter-tui-side over pulling `deepseek-topic-memory` into core deps") honored.
  - New `deepseek_core::engine::hosts::seam::SeamHost` — widest M-series trait so far (10 methods, the entire layered-context Flash pipeline #159):
    - `config_enabled()` / `highest_level()` / `seam_level_for(active_input_tokens, highest_existing_level)` / `verbatim_window_start(message_count)` (pre-request checkpoint decision surface).
    - `collect_seam_texts(messages)` / `produce_soft_seam(messages, level, start_idx, end_idx, workspace, pinned_indices)` / `recompact(existing_seams, recent, level, start_idx, end_idx)` (seam production).
    - `seam_count()` / `produce_flash_briefing(existing_seams, state_text)` / `reset()` (cycle bookkeeping).
    - Opaque `SeamError = Box<dyn std::error::Error + Send + Sync + 'static>` so `anyhow::Error` widens via `.map_err(Into::into)` without leaking the tui-side `anyhow` / `LlmClientError` hierarchy through the core trait surface. `Display` blanket of `dyn Error` preserves the existing log shape (`cycle_hooks.rs` / `layered_context.rs` already format with `{err}`).
    - Strictly call-graph driven (R1): inherent `SeamManager` methods `new` (construction is tui's `LlmClient`-factory concern), `should_cycle` (currently dead code), and the private `summarize_messages` helper are **deliberately not on the trait**. `config(&self) -> &SeamConfig` is replaced by the narrower `config_enabled() -> bool` accessor — `SeamConfig` is a tui-only type and Engine only reads `.enabled`.
  - New `deepseek_core::engine::hosts::workshop::WorkshopHost` — **empty marker** (mirrors M3 `ShellHost`). Engine never invokes a method on `workshop_vars`; the single call site at `tool_context.rs:51` only clones the `Arc<Mutex<WorkshopVariables>>` into `ToolContext` (every `WorkshopVariables` method is called from inside tool implementations, not from Engine). `crates/tui/src/tools/large_output_router.rs` adds `pub struct TuiWorkshopHost(pub Option<Arc<Mutex<WorkshopVariables>>>)` newtype + empty `impl WorkshopHost` per R1.
  - New `deepseek_core::engine::hosts::topic_memory::TopicMemoryHost` — 2 methods (`compose_block(query_hint) -> Option<String>` / `on_turn_complete(user, assistant)`). **Settings move into the implementation** (`TopicMemoryRuntime` gains an owned `settings: TopicMemorySettings` field at construction; new `TopicMemoryRuntime::new(settings)` constructor) so the trait surface stays settings-free — avoids both spike R9 option (b) (adding `deepseek-topic-memory` to core deps) and the parallel-settings-struct anti-pattern. Settings hot-reload is not currently exposed via any slash command, so single-shot ownership at engine init is sufficient.
  - New `deepseek_core::engine::scratchpad_state::ScratchpadStepState` — small state struct (~30 LOC, 2 `usize` fields + `reset(&mut self)`) per spike §3 row #28 + R12. The heavy `crates/tui/src/core/engine/scratchpad_flow.rs` (484 LOC of audit/coverage/reminder helpers — `record_tool_outcome`, `inject_summary_if_needed`, `build_layered_summary`, `coverage_gate`, `read_inventory`, …) **stays tui-side**; the file keeps a `pub use deepseek_core::engine::ScratchpadStepState;` re-export shim so every existing `use crate::core::engine::scratchpad_flow::ScratchpadStepState` caller (engine state, `host_impl/mod.rs` turn-loop bookkeeping, `message_handlers.rs` reset, tests) compiles unchanged.
  - tui inline trait impls (one per host, no extra files):
    - `impl SeamHost for SeamManager` (in `crates/tui/src/seam_manager.rs`) — 10 thin UFCS delegations to the existing inherent methods; errors widened via `.map_err(Into::into)`.
    - `impl WorkshopHost for TuiWorkshopHost` (in `crates/tui/src/tools/large_output_router.rs`) — empty body.
    - `impl TopicMemoryHost for TopicMemoryRuntime` (in `crates/tui/src/topic_memory.rs`) — both methods clone `self.settings` to side-step the `&mut self + &self.settings` simultaneous borrow that `compose_block`'s legacy inherent signature requires (`TopicMemorySettings` is cheap to clone: `bool + PathBuf + u32 + usize + Option<String>`).
  - Engine call-site swaps (proves the trait surface actually covers Engine's needs):
    - `crates/tui/src/core/engine/layered_context.rs` — 8 `seam_mgr.method(...)` calls → `SeamHost::method(seam_mgr, ...)` via a `use deepseek_core::engine::hosts::SeamHost;` at the top of the function module. `seam_mgr.config().enabled` → `SeamHost::config_enabled(seam_mgr)`.
    - `crates/tui/src/core/engine/cycle_hooks.rs` — `collect_seam_texts` / `produce_flash_briefing` / `reset` in the cycle-advance path; `topic_memory_runtime.compose_block(&self.config.topic_memory, query_hint)` → `TopicMemoryHost::compose_block(&mut self.topic_memory_runtime, query_hint)` (settings now owned by the runtime).
    - `crates/tui/src/core/engine/message_handlers.rs` — `topic_memory_runtime.on_turn_complete(&self.config.topic_memory, user, assistant)` → `TopicMemoryHost::on_turn_complete(&mut self.topic_memory_runtime, user, assistant)`.
    - `crates/tui/src/core/engine/engine_new.rs:207` — `TopicMemoryRuntime::default()` → `TopicMemoryRuntime::new(topic_memory_settings)` (settings clone-owned at engine init).
    - `crates/tui/src/core/engine/tests.rs:974` — same constructor swap.
  - **Skipped in M5 (intentional, per call-graph audit + R12):** the field types themselves stay tui (`Option<SeamManager>` / `Option<Arc<Mutex<WorkshopVariables>>>` / `TopicMemoryRuntime`); M7 will swap them to `Option<Box<dyn SeamHost>>` etc. when the core `Engine` struct lands. The `scratchpad_flow.rs` 484 LOC of UI/auditor helpers + `seam_manager.rs` 712 LOC + `topic_memory.rs` 307 LOC body stay tui-side per R12.
  - Net diff `git diff --stat HEAD~..HEAD`: core +289 (new `hosts/seam.rs` ~126, `hosts/workshop.rs` ~41, `hosts/topic_memory.rs` ~60, `scratchpad_state.rs` ~62 incl. 2 unit tests); tui +252/−48 (4 inline trait impls + engine call-site swaps + scratchpad re-export shim + tests update) = **~+493 LOC net** (cap ≤700). Acceptance per spike §6 M5: `core --lib engine::scratchpad_state` 2/2 ok, `tui --lib seam_manager` 7/7 ok, `tui --lib compaction` 66/66 ok, `tui --lib scratchpad` 25/25 ok, `tui --lib tools::subagent` 108/108 ok, `tui --lib mcp` 36/36 ok, `tui --lib runtime_api::tests::sidecar_contract_full_lifecycle` ok, `tui --lib history_isomorphism` 9/9 ok, `tui --test protocol_recovery` 9/9 ok, `cargo build -p deepseek-{core,tui}` clean, `npm run test:f3 && npm run build` (web-ui) clean. The pre-existing `core::engine::tests::{refresh_system_prompt_under_capacity_omits_topic_memory_block, engine_mock_capacity_pre_request_observes_mock_and_emits_decision}` failures **persist on M5 HEAD with the identical assertion line / failure mode** as on M3 and M4 HEAD — confirmed unrelated to M5's 3-trait + scratchpad diff (those tests touch the topic-memory injection cadence and capacity-controller path; the assertion fires before any M5 trait dispatch is exercised).
  - Promotes [BACKLOG_ENGINE_STRUCT_IN_CORE.md](docs/tech/adr/BACKLOG_ENGINE_STRUCT_IN_CORE.md) Progress table: M5 row `queued` → `landed`. M6–M8 remain queued per [PR_M0_ENGINE_STRUCT_TO_CORE_SPIKE.md](docs/tech/adr/PR_M0_ENGINE_STRUCT_TO_CORE_SPIKE.md) §6.

- **Runtime / M-series M4 (PR_M0_ENGINE_STRUCT_TO_CORE_SPIKE §3 row #8, §6 M4; ARCHITECTURE_ASSESSMENT §1 #4, §3.4):** Fourth strangler step — `McpHost` trait promotes the empty `TurnLoopMcpPool` marker into a named host trait alongside M3's `LspHost` / `SubAgentHost` / `ShellHost` / `SandboxHost`. **Hard constraint honored (spike §6 M4):** zero changes to `crates/tui/src/mcp.rs` body (2218 LOC) — every method is a default impl delegating to the existing free functions in `core::engine::dispatch`. Net diff well under the ≤500 LOC cap.
  - New `deepseek_core::engine::hosts::mcp::McpHost` (and re-export at `deepseek_core::engine::McpHost`) — 4 default-impl methods covering the live engine's MCP predicate / metadata surface:
    - `is_mcp_tool(&self, name) -> bool` — delegates to new `core::engine::dispatch::is_mcp_tool_name(name)` free fn (mirrors the body of `tui::mcp::McpPool::is_mcp_tool` so the core turn loop can answer the same question without a tui dependency).
    - `tool_is_parallel_safe(&self, name)` — delegates to `core::engine::dispatch::mcp_tool_is_parallel_safe`.
    - `tool_is_read_only(&self, name)` — delegates to `core::engine::dispatch::mcp_tool_is_read_only`.
    - `tool_approval_description(&self, name)` — delegates to `core::engine::dispatch::mcp_tool_approval_description`.
  - `TurnLoopMcpPool` deprecation cycle: the marker stays in `core::engine::turn_loop::host` as a `#[deprecated(since = "0.8.16", note = "use deepseek_core::engine::hosts::McpHost instead")]` alias with a blanket `impl<T: McpHost + ?Sized> TurnLoopMcpPool for T {}` so existing `Self::McpPool: TurnLoopMcpPool` bounds keep building for one release. `TurnLoopHost::McpPool` associated-type bound changed from `TurnLoopMcpPool` to `McpHost`. `pub use host::TurnLoopMcpPool` in `turn_loop::mod.rs` carries `#[allow(deprecated)]` so the internal re-export does not warn.
  - tui swap: `impl TurnLoopMcpPool for McpPool {}` (`crates/tui/src/core/engine/turn_loop/host_impl/mod.rs:42`) → `impl McpHost for McpPool {}` (one-liner; uses default impls only — `McpPool` has no extra state to override). `McpPoolPort` dispatch trait (P2 PR4) and `McpPoolHandle = Arc<Mutex<McpPool>>` wrapper are **unchanged** — they own a different `self` shape (locked container vs. bare pool) and stay orthogonal to `McpHost`.
  - Drift-guard tests (M4 Q5A "zero call-site churn" mitigation — the tui inherent `McpPool::is_mcp_tool` and the core free fn `is_mcp_tool_name` are dual definitions per the spike's "zero changes to mcp.rs body" constraint):
    - `core::engine::dispatch::tests::is_mcp_tool_name_covers_prefix_and_resource_helpers` (8 names).
    - `tui::core::engine::turn_loop::host_impl::m4_drift_guard::is_mcp_tool_name_matches_tui_mcp_pool` — asserts the two definitions produce identical output on a 15-name curated set spanning `mcp_*` prefix, the three `*_mcp_resource*` literals, and known non-MCP names (`read_file`, `exec_shell`, …).
    - `tui::core::engine::turn_loop::host_impl::m4_drift_guard::mcp_pool_satisfies_mcp_host_with_default_impls` — type-level bound assertion `McpPool: McpHost`.
    - `core::engine::hosts::mcp::tests::default_impls_match_dispatch_module` + `dyn_dispatch_compiles` — stub-host coverage of the four default methods.
  - **Skipped in M4 (intentional, per call-graph audit + spike §5 R1):** `execute_tool` (lives on `McpPoolPort`, implemented on `McpPoolHandle = Arc<Mutex<McpPool>>` — different `self` shape; merging would require reworking the `mcp_pool_as_port` factory and rippling through every `Option<Arc<AsyncMutex<Self::McpPool>>>` turn-loop parameter); `ensure_pool` / `shutdown_all` (mutate engine state — `self.mcp_pool = Some(...)` — and depend on `EngineConfigExt.network_policy` + `session.mcp_config_path`; stay as inherent `Engine` methods at `tool_context.rs:112-124` and `op_loop.rs:86-89`, will move into the core `Engine` struct alongside the field in M7).
  - Net diff `git diff --stat HEAD~..HEAD`: core +185 (new `hosts/mcp.rs` ~125, `dispatch.rs` +31, host.rs +18, mod.rs/hosts.rs +10, turn_loop/mod.rs +4); tui +73/−12 (impl swap + drift guard); docs +30 = **~+275 LOC net** (cap ≤500). Acceptance per spike §6 M4: `core --lib engine::hosts::mcp` 2/2 ok, `core --lib engine::dispatch` 4/4 ok, `tui --lib mcp` 36/36 ok (includes `test_mcp_pool_is_mcp_tool` + 2 M4 drift-guard tests + `mcp_pool_handle_implements_core_mcp_port` P2 PR4 trait satisfaction), `tui --lib m4_drift_guard` 2/2 ok, `tui --lib tools::subagent` 108/108 ok, `tui --lib runtime_api::tests::sidecar_contract_full_lifecycle` ok, `tui --lib history_isomorphism` 9/9 ok, `tui --test protocol_recovery` 9/9 ok, `core --lib engine::turn_loop::capacity_policy` 4/4 ok, `cargo build -p deepseek-{core,tui}` clean, `npm run test:f3 && npm run build` (web-ui) clean. The 2 `core::engine::tests::{refresh_system_prompt_under_capacity_omits_topic_memory_block, engine_mock_capacity_pre_request_observes_mock_and_emits_decision}` failures observed on the working tree were **independently reproduced on M3 HEAD (`1db7a51`)** and confirmed pre-existing — unrelated to M4's MCP-only diff (both tests touch the topic-memory / capacity-controller path, not MCP).
  - Promotes [BACKLOG_ENGINE_STRUCT_IN_CORE.md](docs/tech/adr/BACKLOG_ENGINE_STRUCT_IN_CORE.md) Progress table: M4 row `queued` → `landed`. M5–M8 remain queued per [PR_M0_ENGINE_STRUCT_TO_CORE_SPIKE.md](docs/tech/adr/PR_M0_ENGINE_STRUCT_TO_CORE_SPIKE.md) §6.

- **Runtime / M-series M3 (PR_M0_ENGINE_STRUCT_TO_CORE_SPIKE §3 rows #6–#9 + #26–#27, §6 M3; ARCHITECTURE_ASSESSMENT §1 #4, §3.4):** Third strangler step — subsystem host traits (LspHost / SubAgentHost / ShellHost / SandboxHost) introduced + supporting data types moved into `deepseek-core`. **Strictly call-graph driven** (spike §5 R1): each trait method exists iff the live `Engine` calls it — pass-through fields (Shell, Sandbox) get marker / single-accessor traits so M7 only needs to swap the field type, not invent new surface.
  - New `deepseek_core::engine::hosts::{LspHost, SubAgentHost, ShellHost, SandboxHost}` — engine boundary trait module. `LspHost` (2 methods: `enabled()` + `diagnostics_for(file, edit_seq) -> Option<DiagnosticBlock>`); `SubAgentHost` (3 methods: `spawn_general()`, `list_with_cleanup()`, `running_count()`); `ShellHost` empty marker (Engine never invokes shell methods directly — only clones the `SharedShellManager` into `ToolContext`); `SandboxHost` single accessor `backend() -> Option<&Arc<dyn SandboxBackend>>` (Engine only forwards the optional `Arc` to `ToolContext`).
  - Data-type moves into core (matching spike §3 rows #26 + #27):
    - `deepseek_core::lsp::diagnostics` ← `tui::lsp::diagnostics` — `Diagnostic` / `DiagnosticBlock` / `Severity` / `render_blocks` + 8 unit tests. Pure `std::path` deps. The tui crate keeps `tui::lsp::diagnostics` as a re-export shim so existing `crate::lsp::DiagnosticBlock` / `crate::lsp::render_blocks` callers (engine tests, `tools/spec.rs`) compile unchanged.
    - `deepseek_core::sandbox` (new top-level module) ← `tui::sandbox::backend` (trait + types only) — `SandboxBackend` trait, `SandboxOutput`, `SandboxKind` + `SandboxKind::parse` / `as_str` + 2 unit tests. The tui `create_backend(&Config)` factory and the `OpenSandboxBackend` impl stay tui-side (depend on tui's `Config`); `tui::sandbox::backend` re-exports the trait/types from core so `use crate::sandbox::backend::SandboxBackend` etc. keep working.
  - Trait implementations (inline on existing tui types per Q3 decision):
    - `impl LspHost for crate::lsp::LspManager` — delegates `enabled()` to `config.enabled` and `diagnostics_for(...)` to the existing inherent method via UFCS.
    - `impl SubAgentHost for Engine` — replaces the old `impl SubAgentSpawnPort for Engine` (orchestration unchanged: still calls into `Engine::spawn_general_subagent` / `Engine::list_subagents`); adds `running_count` (reads `subagent_manager.read().await.running_count()`).
    - `crate::sandbox::TuiSandboxHost(pub Option<Arc<dyn SandboxBackend>>)` newtype + `impl SandboxHost` — mirrors the `SharedShellManager` ownership pattern.
    - `crate::tools::shell::TuiShellHost(pub SharedShellManager)` newtype + empty `impl ShellHost` (bare `ShellManager` is `Send` but not `Sync` — it holds `Box<dyn Write + Send>` and `Box<dyn portable_pty::Child + Send>` fields — so the trait is implemented on the `Arc<Mutex<...>>`-shaped newtype instead of the raw manager).
  - `SubAgentSpawnPort` → `SubAgentHost` rename: the old trait stays in `deepseek_core::engine::subagent_port` as a `#[deprecated(since = "0.8.16", note = "use deepseek_core::engine::hosts::SubAgentHost instead")]` alias so any out-of-tree consumers (none in this workspace) keep building for one cycle. `pub use SubAgentSpawnPort` in `engine::mod.rs` carries `#[allow(deprecated)]` so the internal re-export does not warn.
  - Engine call-site swaps (proves the trait surface actually covers Engine's needs):
    - `crates/tui/src/core/engine/lsp_hooks.rs:24,38` — `self.lsp_manager.config().enabled` / `.diagnostics_for(...)` → `LspHost::enabled(&*self.lsp_manager)` / `LspHost::diagnostics_for(...)` via a `&dyn LspHost` reborrow.
    - `crates/tui/src/core/engine/turn_loop/host_impl/no_tool_uses.rs:68` — `self.subagent_manager.read().await.running_count()` → `<Engine as SubAgentHost>::running_count(self).await`.
  - Net diff (estimated `git diff --stat HEAD~..HEAD`): core +460 (new `sandbox/mod.rs`, `lsp/diagnostics.rs`, `engine/hosts/{mod,lsp,subagent,shell,sandbox}.rs`); tui −265/+90 (shim + impls + newtypes + call-site swaps); docs +20 = **~+320 LOC net** (cap ≤700). Acceptance per spike §6 M3: `core --lib lsp/sandbox` ok (11+2 new tests), `tui --lib tools::subagent` 108/108 ok, `tui --lib history_isomorphism` ok, `core --lib capacity_policy` ok, `tui --lib config::tests::instructions_paths` ok, `tui --lib tools::subagent::tests::resident_file` ok, `tui --lib core::engine::tests::build_tool_context_wires_lsp` ok, `tui --lib capacity_escalation` ok, `tui --test protocol_recovery` 9/9 ok, `tui --lib sidecar_contract_full_lifecycle` ok, `cargo build -p deepseek-{core,tui}` clean.
  - Promotes [BACKLOG_ENGINE_STRUCT_IN_CORE.md](docs/tech/adr/BACKLOG_ENGINE_STRUCT_IN_CORE.md) Progress table: M3 row `queued` → `landed`. M4–M8 remain queued per [PR_M0_ENGINE_STRUCT_TO_CORE_SPIKE.md](docs/tech/adr/PR_M0_ENGINE_STRUCT_TO_CORE_SPIKE.md) §6.

- **Runtime / M-series M2 (PR_M0_ENGINE_STRUCT_TO_CORE_SPIKE §3 row #1, §5 R2, §6 M2; ARCHITECTURE_ASSESSMENT §1 #4):** Second strangler step — `EngineConfig` split type pillars established. The fat tui `EngineConfig` (30 fields) is now conceptually `lean (25) ⊕ ext (5/8)`:
  - New `deepseek_core::engine::config::EngineConfig` — **lean** 25-field subset depending only on core types (`model/workspace/allow_shell/trust_mode/notes_path/mcp_config_path/skills_dir/instructions/max_steps/max_subagents/subagent_step_timeout/features/compaction/cycle/capacity/max_spawn_depth/snapshots_enabled/subagent_model_overrides/memory_enabled/memory_path/goal_objective/locale_tag/strict_tool_mode/task_type/scratchpad`). Plain `Default` lands placeholder paths (`PathBuf::new()` for `skills_dir`, `model = ""`) since the tui facade owns the disk-aware defaults; core-only callers will override before use.
  - New `tui::core::engine::types::EngineConfigExt` — **ext** carry for the 8 tui-only fields (`todos/plan_state/network_policy/lsp_config/runtime_services/topic_memory/workshop/llm_client_override`). Marked `#[allow(dead_code, reason = "M2 type pillar — first consumer lands in M3")]` because production code still flows through the monolithic facade.
  - `tui::core::engine::types::EngineConfig` keeps its **flat 30-field layout** so every existing caller (≈30 literal-construction sites in `core::engine::tests`, `cli/commands/legacy.rs`, `runtime_threads/engine_load.rs`, etc.) compiles unchanged. Four new accessors carve the projection: `lean(&self) -> core::EngineConfig`, `ext(&self) -> EngineConfigExt`, `into_parts(self) -> (lean, ext)`, `from_parts(lean, ext) -> Self`. Two round-trip unit tests (`lean_into_parts_round_trip`, `lean_borrow_matches_into_parts_owned`) guarantee the projection stays aligned as fields evolve.
  - **Why facade over `Engine::new(slim, ext_via_host)` now:** spike R2's two-arg signature would force ~30 literal-construction sites to rewrite to `EngineConfig { core: core::EngineConfig { … }, ext: EngineConfigExt { … } }`, blowing the ≤700 LOC cap. M2 stops at type pillars; M7 (Engine struct → core) will atomically switch the entry point to `Engine::with_hosts(lean, ext)` once the host trait surface from M3–M6 is in place.
  - Net diff `git diff --stat HEAD~..HEAD`: `crates/core/src/engine/config.rs` +119 (new), `crates/core/src/engine/mod.rs` +1, `crates/tui/src/core/engine/types.rs` +259/−1 = **+378 LOC net** (cap ≤700). Acceptance per spike §6 M2: `engine_llm_client_override_runs_mock_turn` ok, 36 `error_taxonomy` golden ok (core suite), `sidecar_contract_full_lifecycle` ok, 2/2 new round-trip tests ok, `cargo check --workspace --all-targets` clean, `npm run test:f3` clean.
  - Promotes [BACKLOG_ENGINE_STRUCT_IN_CORE.md](docs/tech/adr/BACKLOG_ENGINE_STRUCT_IN_CORE.md) Progress table: M2 row `pending` → `landed`. M3–M8 remain queued per [PR_M0_ENGINE_STRUCT_TO_CORE_SPIKE.md](docs/tech/adr/PR_M0_ENGINE_STRUCT_TO_CORE_SPIKE.md) §6.

- **Runtime (D2 follow-up / ARCHITECTURE_ASSESSMENT §3.3):** Sidecar now accepts `--port 0` (OS-assigned ephemeral port). Removed the `if options.port == 0 { bail!("Port must be > 0"); }` guard in `crates/tui/src/runtime_api/mod.rs`; the rest of the chain (`TcpListener::bind` + `listener.local_addr().port()` + `DS_PICK_READY {port: <bound>}` + desktop `watch::Receiver<u16>` consumer) was already in place from the D2 infrastructure commit (`4d1cbab`). `bail!` removed from `anyhow` import (no other callers). `sidecar_contract_full_lifecycle` re-run green. Closes the remaining "one-liner" follow-up from the prior D2 ChangeLog entry.

- **Runtime / M-series M1 (PR_M0_ENGINE_STRUCT_TO_CORE_SPIKE §7, ARCHITECTURE_ASSESSMENT §1 #6):** First strangler step of the `Engine` struct → `deepseek-core` migration. Three carrier types moved into `crates/core/src/engine/` with **behavior-only** changes (no `/v1` wire format change, no sidecar contract change):
  - `Op` enum (15 variants) → `deepseek_core::engine::op::Op`. `Op::SendMessage.mode` / `Op::ChangeMode.mode` now use `core::turn::TurnLoopMode` instead of `tui::AppMode` (1:1 isomorphic Agent/Yolo/Plan). Producers wrap via `app_mode_to_turn_loop(...)` (5 sites: `tui/ui.rs`, `cli/commands/legacy.rs`, `tests.rs` x3); the dispatch loop unwraps via `turn_loop_to_app_mode(...)` once so all tui-side `handle_*_op` signatures stay untouched.
  - `EngineHandle` → `deepseek_core::engine::handle::EngineHandle<P, R>` — generic over sandbox policy (`P`) and `request_user_input` response (`R`); `P, R: Send + Sync + 'static`. tui crate keeps `pub type EngineHandle = ...<SandboxPolicy, UserInputResponse>;` alias so all 18 caller import paths stay intact. New `pub fn EngineHandle::new(...)` replaces the prior `pub(super)` field-literal construction at the two build sites (`engine_new.rs:211`, `mock.rs:57`). `impl TurnEnginePort for EngineHandle<P, R>` lives in core now (orphan-rule clean); the tui-side `core/engine/turn_port.rs` is deleted. New `TurnLoopMode::from_setting("agent"/"yolo"/"plan")` mirrors `AppMode::from_setting` so the runtime-API string ↔ enum boundary stays in core.
  - `ThreadContextSnapshot` struct → `deepseek_core::engine::context_snapshot::ThreadContextSnapshot`. The `build_thread_context_snapshot` helper stays tui-side because it depends on the tui-only `compaction::should_compact` (~1k LOC) — M0 §1.2 keeps that out of scope.
  - tui re-export shims: `crate::core::ops` (1-line `pub use`), `crate::core::engine::handle` (single `type` alias), `crate::context_snapshot` (re-export + retained build helper) keep every existing import working.
  - **Skipped in M1 (intentional):** `coherence.rs` reducer — `CoherenceState` is already in core (P2 PR4); `next_coherence_state` + `CoherenceSignal` depend on tui-only `core::capacity::{GuardrailAction, RiskBand}` and will migrate together with `CapacityController` in M6 per spike §3 row #22.
  - Net diff `git diff --stat HEAD~..HEAD`: tracked +59/−329 + new core files +369 = **+99 LOC net** (cap was ≤ 500). Spike §7.4 acceptance checklist all green: 8/8 regression tests pass (`capacity_policy`, `history_isomorphism`, `instructions_paths`, `tools::subagent::resident_file`, `build_tool_context_wires_lsp`, `capacity_escalation`, `protocol_recovery`, `sidecar_contract_full_lifecycle`), web-ui `npm run test:f3` + `npm run build` green, `cargo check --workspace --all-targets` clean.
  - Promotes [BACKLOG_ENGINE_STRUCT_IN_CORE.md](docs/tech/adr/BACKLOG_ENGINE_STRUCT_IN_CORE.md) `In spike` → `In progress (M1 landed)`; M2–M8 remain queued per [PR_M0_ENGINE_STRUCT_TO_CORE_SPIKE.md](docs/tech/adr/PR_M0_ENGINE_STRUCT_TO_CORE_SPIKE.md) §6.

- **Zagens / Runtime (D2 / ARCHITECTURE_ASSESSMENT §1 #3, §3.3):** Runtime port discovery is now fully dynamic — desktop shell ↔ sidecar use `tokio::sync::watch::channel::<u16>` for port publishing, sidecar reports the **actually bound** port via `DS_PICK_READY.port` (was: the **requested** port, broken when binding to `0` / ephemeral). `AppContext::runtime_port` is `watch::Receiver<u16>`; new helpers `AppContext::{current_port,require_port}`; `get_runtime_port` IPC awaits `rx.changed()` until the first non-zero publish (web-ui `initRuntimeConfig` naturally serializes behind sidecar readiness). All call sites that previously used `ctx.runtime_port: u16` now go through `ctx.require_port()?` — covers `runtime_proxy::{http,post_stream,get_sse}`, `commands::{export_thread_json, export_session_json, rebuild_symbol_index, read_thread_workspace_binary}` (5 files: `desktop/src/{main,commands,runtime_proxy,sidecar}.rs` + `tui/src/runtime_api/mod.rs`). On restart paths supervisor `port_tx.send(0)` so IPC handlers either fast-fail (`require_port`) or await (`get_runtime_port`) instead of using stale ports. Initial suggested port stays `7878` for back-compat / `curl localhost:7878` debugging; the remaining "let sidecar bind `--port 0`" change is a one-liner (remove `if options.port == 0 { bail!(...) }`) that can ship in a follow-up PR. Regressions green: `sidecar_contract_full_lifecycle`, `desktop::architecture_boundary`, `desktop::runtime_proxy_paths`, full `cargo check --workspace --all-targets` clean.

### Removed

- **Workspace (D3 / ARCHITECTURE_ASSESSMENT §1 #8):** `crates/tui-core` legacy crate (event-driven TUI state machine scaffold + snapshot test) removed — confirmed not linked by any other crate (`deepseek-tui`, Zagens, CLI 均无 path 依赖). Workspace member removed from root `Cargo.toml`; directory deleted; `cargo check --workspace --all-targets` 全绿（3m37s, exit 0, 只剩 pre-existing dead_code warnings 与本次删除无关）. [RUNTIME_ARCHITECTURE.md](docs/tech/RUNTIME_ARCHITECTURE.md) §3 依赖图 legacy 节点同步移除；[ARCHITECTURE_ASSESSMENT_2026-05-25.md](docs/tech/adr/ARCHITECTURE_ASSESSMENT_2026-05-25.md) §1 checklist 4/10、§3.8 标记完成、§5 P0 D3 行 ✅。

### Added

- **Runtime (A1 follow-up / §12.1 #1):** `App::check_live_history_isomorphism(site)` replaces the debug-only `debug_assert_live_history_isomorphism`; production builds also surface drift via `tracing::warn!(target = "tui::history_isomorphism")` + `history_isomorphism::drift_count()` (process-wide `AtomicU64`). Four call sites (`tool_complete` / `turn_complete` / `session_load` / `backtrack`) pass a static `site` label for triage. Regression tests `record_drift_increments_global_counter`, `reset_drift_count_for_test_zeroes_counter`, `drift_is_detected_when_tool_output_diverges` (serialized via per-module mutex). Roadmap §12.1 全 5 项闭合 — see [A1_PERSIST_BLOCKING_AUDIT.md](docs/tech/adr/A1_PERSIST_BLOCKING_AUDIT.md) §Status.
- **Docs:** Roadmap §17.5 / §12.1 #1 / §17.1 / §7.1 + [IMPLEMENTATION_SUMMARY_2026-05-24.md](docs/tech/adr/IMPLEMENTATION_SUMMARY_2026-05-24.md) sync for A1 live ToolCell isomorphism closure (2026-05-25).

### Fixed

- **Zagens desktop:** Right panel collapse state persists across restarts (`deepseek-desktop-right-panel-collapsed`); first launch stays collapsed; sidebar inspector tabs expand the panel on click.

### Changed

- **Branding:** Product renamed from **DS Pick** to **Zagens** (tagline: *Desktop agent harness* / 桌面 Agent 控制台). User-visible strings, README, LICENSE, NOTICE, Tauri `productName` / `identifier` (`com.zagens.desktop`), default workspace `<Documents>/Zagens` with legacy `<Documents>/DS Pick` fallback; localStorage keys migrated (`zagens-locale`, `zagens:*` prefs). CI release tags: `zagens-v*` (preferred) and legacy `ds-pick-v*`.
- **Docs:** [A2_A3_SIGNOFF.md](docs/tech/adr/A2_A3_SIGNOFF.md) — §12.1 #2（Turn 可观测）与 #3（错误分类）维护者签收（2026-05-25）；路线图 §7.2/§7.3/§12.1 勾选同步。
- **Docs:** [RUNTIME_EVOLUTION_ROADMAP.md](docs/tech/RUNTIME_EVOLUTION_ROADMAP.md) §12.1/§12.5/§17 与代码二次对齐（2026-05-25）— B2/B-L3、`events_since_async`、门控闭合表述；[IMPLEMENTATION_SUMMARY](docs/tech/adr/IMPLEMENTATION_SUMMARY_2026-05-24.md) 同步；[TUI_DS_PICK_GAP.md](docs/desktop/TUI_DS_PICK_GAP.md) 审核表（托盘/导出/记忆地图 UI）。
- **Docs:** [RUNTIME_ARCHITECTURE.md](docs/tech/RUNTIME_ARCHITECTURE.md) 与代码对齐（2026-05-25）— P2 core/tui 拆分、crate 依赖图、`runtime_api/`/`runtime_threads/` 模块路径、双持久化/双通道、Zagens sidecar 监督、D12 Desktop-only。
- **Docs:** [RUNTIME_ARCHITECTURE.md](docs/tech/RUNTIME_ARCHITECTURE.md) 图表细化第二轮（2026-05-25）— §1 顶层系统总览拆为分层 subgraph（用户/桌面壳/sidecar/外部/持久化/CLI）并附"节点 ↔ 代码出处"映射表；§2 Sidecar 内部数据流细化（router→auth→stream/threads, manager 内 active/lifecycle/monitor/persist/broadcast 拆分）；§3 crate 依赖图与各 `Cargo.toml` 一一核对（含 `agent`/`execpolicy`/`hooks`/`protocol`/`state` 等所有真实边）；§5 双通道新增 mermaid 图 + `validate_runtime_path` 白名单 + SSE 取消 + sidecar 握手 `DS_PICK_READY`；§8 改为 sequenceDiagram 并补「Op 是 mpsc」「取消两层」要点；§9 关键模块索引扩到 16 条全 clickable 链接。
- **Docs:** 新增 [ARCHITECTURE_ASSESSMENT_2026-05-25.md](docs/tech/adr/ARCHITECTURE_ASSESSMENT_2026-05-25.md) — 架构现状评估 + "先定型再迭代功能" 决策快照：§1 给出 10 条定型 checklist（当前 3/10）作为解冻判定门槛；§3 列出 10 项技术债（高/中/低）；§5 把迭代方向（D1–D14）按 P0/P1/P2 分级并交叉引用现有 backlog ADR（M-series PR_M0、RUNTIME_UNIFICATION、STATESTORE_JSONL、LANDLOCK_ENFORCE）；§7 落地"功能冻结期 PR 准入红线"（禁止在 `crates/tui` 新建顶层文件、禁止给 `Engine` struct 加新字段、禁止新增 `/v1/*` 无 OpenAPI schema 的端点等）。[RUNTIME_ARCHITECTURE.md](docs/tech/RUNTIME_ARCHITECTURE.md) 与 [RUNTIME_EVOLUTION_ROADMAP.md](docs/tech/RUNTIME_EVOLUTION_ROADMAP.md) 头部新增反向引用。
- **Docs:** [API_DESIGN.md](docs/tech/API_DESIGN.md)、[RUNTIME_EVOLUTION_ROADMAP.md](docs/tech/RUNTIME_EVOLUTION_ROADMAP.md) §3 交叉对齐（2026-05-25）— H06 代理认证、IPC ~41 条、模块路径、三文档互链。
- **Project identity:** Zagens is **proprietary** ([LICENSE](LICENSE)); third-party runtime MIT license at [third-party/deepseek-tui/LICENSE](third-party/deepseek-tui/LICENSE) (not at repo root). See [NOTICE.md](NOTICE.md). Removed upstream npm/website, CLI Docker artifacts, CLI binary Release (`auto-tag.yml`, npm/crates release scripts), and `ci.yml` npm-wrapper job. **Release:** `.github/workflows/release.yml` builds **Zagens Windows installers** on `ds-pick-v*` tags only (macOS/Linux later). **Config samples:** [`.env.example`](.env.example) and [`config.example.toml`](config.example.toml) reframed for Zagens desktop + embedded sidecar (not upstream TUI/CLI).

### Added

- **Docs:** [docs/desktop/DEV_NOTES.md](docs/desktop/DEV_NOTES.md) §2026-05-24 — product strategy memo (desktop-only shell, TUI/CLI demotion, long-horizon CRAFT ~35 min, industry alignment, D12–D14 candidates, L3 backlog).
- **B2.1:** Injection arbitration SSOT — [docs/tech/adr/B2_INJECTION_ARBITRATION.md](docs/tech/adr/B2_INJECTION_ARBITRATION.md) (tool results > CRAFT blackboard > topic_memory).
- **B-L3:** Zagens `TopicMemoryPanel` + `GET /v1/topic-memory` (graph + eval metrics); settings sidebar entry.
- **B2.5:** `scripts/topic-memory-eval.ps1` — clarification-rate baseline compare + `-Gate`; `TopicMemoryEvalReport` / `compare_eval` in `deepseek-topic-memory`.
- **B3.3:** SSE backpressure — `RecvError::Lagged` catch-up from store + `coalesce_delta_events` for `item.delta`.
- **B3.1:** `main.rs` slimmed (~350 lines); CLI tests → `cli/tests.rs`; `cli/entry.rs` (`configure_windows_console_utf8`).
- **B2.3:** k-hop subgraph retrieval (`retrieve_for_query`) seeds injection from the latest user turn instead of pasting the full hot graph.
- **B2.5:** `topic-memory-metrics.json` sidecar (turn updates, inject count, repeat-topic / clarification heuristics).
- **B2 (Zagens):** Settings panel toggles `topic_memory_enabled` and inject interval; persisted to `[topic_memory]` in `config.toml`.
- **B3 CLI:** `run_*` and helpers moved to `crates/tui/src/cli/commands/legacy.rs`; `main.rs` ~1.4k lines (was ~4.9k); `clap` in `cli/args.rs`.
- **Runtime (A1.3):** `RuntimeThreadManager::events_since_async` — HTTP/SSE/task paths offload SQLite/JSONL reads via `spawn_blocking`.
- **Docs:** Backlog ADRs (`BACKLOG_ENGINE_STRUCT_IN_CORE`, `BACKLOG_RUNTIME_UNIFICATION`, `BACKLOG_STATESTORE_JSONL`, `BACKLOG_LANDLOCK_ENFORCE`); `A1_PERSIST_BLOCKING_AUDIT.md`; `tui-core` legacy README.
- **A1:** Live history isomorphism wired at turn complete, tool complete, session load, and backtrack (full `live_history_matches_messages` path). Superseded 2026-05-25 by `App::check_live_history_isomorphism` (production-grade, see §Added above).
- **B2.1:** Capacity guardrail refresh omits `<topic_memory>` via `PromptInjectionArbitration::capacity_pressure()`; regression test in `core/engine/tests.rs`.
- **Tests (CRAFT/GAP):** Unit tests for `instructions_paths` auto-discovery, `resident_file` hard lock, and sub-agent LSP inheritance in `build_tool_context`; G2 §11 smoke runbook for integration sign-off.
- **CRAFT:** Sub-agent `ToolContext` inherits parent `lsp_manager` when LSP enabled — `diagnostics` works in child turns.
- **CRAFT (Issue 6):** `Config::instructions_paths(workspace)` auto-discovers `PROJECT_RULES.md` and `.cursor/rules/*.mdc` when `instructions = [...]` is unset or empty (pick-rules merge unchanged).
- **CRAFT:** `resident_file` hard lock — conflicting lease rejects spawn instead of warning-only.
- **Runtime (GAP):** `POST /v1/threads/{id}/fork-at-user-message` with `{ depth_from_tail }` exposes `fork_at_user_message` for backtrack-depth forking.
- **Zagens (GAP):** `agent_spawn` / `spawn_agent` tool cards show inline sub-agent status linked to AgentPanel SSE state.
- **Zagens (GAP):** User messages (non-last) offer **Branch** → `forkThreadAtUserMessage` + composer prefill from `original_user_text`.
- **Runtime (A1.4):** `history_isomorphism` — live tool-detail outputs vs message tool-results parity helpers + tests.
- **Desktop API:** `forkThreadAtUserMessage()` client helper.

### Fixed

- **Zagens (F3):** `ModelParamsDialog` — `role="dialog"`, `aria-modal`, labelled controls, Escape to close, focus on open; strings via `modelParams.*` i18n.
- **Runtime (A1.4):** `history_isomorphism` — thinking block round-trip + `history_transcript_core_matches_messages`; compaction/trim/persist paths use core check; partition trim regression tests.
- **Zagens (F3):** Right-panel workbench tabs + integrated terminal session tabs use roving `tabIndex` and Arrow/Home/End keyboard navigation (`lib/a11y/rovingTabList.ts`).
- **Runtime (A3.2/A3.4):** `ErrorRetryPolicy` + `user_hint_for_category`; `ErrorEnvelope.hint` and unified HTTP `error` payload (`class`, `retryable`, `retry_policy`, `hint`); TUI status line appends hint; `api_error_payload_includes_taxonomy_fields` regression.
- **Runtime (A+.7):** Register `pending_approvals` before emitting `approval.required` (fixes resolve-approval racing JSONL/SSE); multi-window regression tests `parallel_pending_approvals_resolve_scoped_to_thread_turn` + `sidecar_parallel_pending_approvals_resolve_then_continue`.
- **Runtime (A1.2):** Large-output routing stamps `ToolResult.metadata.large_output` with persisted `meta_path`; `monitor_turn` copies into turn-item `artifact_refs` so JSONL/SQLite items round-trip to `large_outputs/` blobs.
- **Runtime (A3.3):** Stream transparent/outer retries consult `is_stream_failure_retryable` — `InvalidInput` / auth errors no longer burn retry budget; network/timeouts still retry.
- **Runtime (A3):** `classify_error_message` recognizes DeepSeek thinking/reasoning constraint strings as `InvalidInput` (distinct from network disconnect); golden suite centralized in `deepseek-core::error_taxonomy`.
- **Desktop (approval):** 系统设置新增 **「自动批准」** 审批策略；非 auto 时 Composer 显示只读「审批：按需审批」等，不再展示无法勾选的复选框；`approval_policy=auto` 时显示可勾选的「自动批准工具调用」。
- **Desktop (F3):** Composer card markup — options/bridge/textarea/actions stay inside `.card` (removed premature close that left input chrome outside the card).
- **Runtime (A+.4):** `sidecar_contract_full_lifecycle` — interrupt endpoint aligned with Zagens (`POST .../interrupt`, not legacy `/stop`).
- **Zagens (F3):** Terminal session tabs — close control no longer nested inside tab button (valid HTML + keyboard activation).

### Changed

- **B2 topic memory:** Default graph path is `~/.deepseek/topic-memory/graph.json` (dedicated folder, not beside `config.toml`); metrics at `~/.deepseek/topic-memory/metrics.json`. Legacy `~/.deepseek/topic-memory.json` is moved on first use.
- **Runtime (A1):** `set_routing_rules` persists via `spawn_blocking` (async HTTP path no longer blocks on JSON I/O).
- **Governance (D10):** 维护者签收解除桌面 Feature freeze（Jason，2026-05-24）— [docs/tech/adr/P2_D10_UNFREEZE_RECORD.md](docs/tech/adr/P2_D10_UNFREEZE_RECORD.md) §4；路线图 §17.4 已勾选。
- **Governance (F3):** G2 手测清单 §8（键盘 a11y 8.1–8.5）维护者签收 ✅（2026-05-24）— [G2_PR5_MANUAL_SMOKE_CHECKLIST.md](docs/tech/adr/G2_PR5_MANUAL_SMOKE_CHECKLIST.md) §6。
- **Governance (§12.4 #2):** **已闭合**（2026-05-24）— Stop / 长跑双壳 / Zagens 审批（G2 §2 + §9）；全量审核 [CODE_REVIEW_2026-05-24.md](deliverables/CODE_REVIEW_2026-05-24.md)。
- **Runtime (P2 PR6a–d):** Turn loop streaming + tool planning/outcomes + `tool_parser` in `deepseek-core`; TUI `tool_plans_exec` + split `host_impl/`; `capacity_policy` + `TurnLoopMode` capacity checkpoints; `execute_plan_on_engine` / `detached_execute_with_lock`. Plan: `docs/tech/adr/P2_PR6_TURN_LOOP_L2_MIGRATION_PLAN.md`（PR6 切片已全部落地；ADR/spike 已同步）。
- **Runtime (A1.6 / R-015):** Full baseline @ `8b1538a` — median RSS **29 MB** (3×50 + 1.1 MB fixture, `-Gate` PASS vs 28.5 MB); ADR + `deliverables/runtime-baseline-full-run.log` updated.
- **Runtime (A1-full):** Emergency trim (`trim_oldest_messages_to_budget`) uses hot/cold partition — drops `ColdSummary` first, preserves hot / pinned / `[workshop-ref]` messages; `context_trim::trim_messages_partition_aware`.
- **Desktop (A6.2):** Sandbox settings on non-macOS show explicit **degraded mode** copy (`settings.sandboxDegradedMode`).
- **Runtime (A6.2):** TUI logs `policy_degraded_mode_notice()` once at interactive startup when OS sandbox is degraded.
- **Desktop (F1a):** `TerminalCard` appends `tool.progress` to xterm incrementally instead of full clear+rewrite each frame.
- **Desktop (F1b):** `MessageBubble` shows `DiffCard` while diff tools are still running when unified diff appears in streamed output.
- **Desktop (F3):** Escape stops active generation when focus is outside inputs.
- **Desktop (F3):** Sidebar `role="navigation"`; Composer `#composer-input`, options/actions `role="toolbar"`, DOM tab order (input before options bar; CSS `order-*` keeps visual layout), skip link to composer; send `aria-keyshortcuts="Enter"`.

### Added

- **Runtime (GAP 8a):** `StartTurnRequest` / `StartTurnParams` / session sampling fields (`temperature`, `top_p`, `max_output_tokens`); `streaming_phase` forwards them to the API request.
- **Runtime (GAP F4):** `POST /v1/threads/{id}/edit-last-turn` — truncate last user turn on live engine session and start a new turn (TUI `/edit` parity).
- **Zagens (GAP 8a):** Composer gear opens `ModelParamsDialog`; params persist in localStorage and pass through `startThreadTurn` / `POST /v1/stream`.
- **Zagens (GAP F4):** Edit last user message from `MessageBubble` → dialog → `editLastThreadTurn` + SSE replay.
- **Docs:** 路线图 §17.3 / `IMPLEMENTATION_SUMMARY` / `TUI_DS_PICK_GAP` 按 2026-05-24 代码审计更新（manager 已拆、F0–F3/路由/导出/托盘/智能粘贴已闭合）。
- **Docs:** G2 §10 B-L1 CRAFT 手测签收（2026-05-24）— §12.5 #1 闭环、AgentPanel、`craft.*` SSE；[G2_PR5_MANUAL_SMOKE_CHECKLIST.md](docs/tech/adr/G2_PR5_MANUAL_SMOKE_CHECKLIST.md) §10。
- **Runtime (B-L1 / CRAFT):** Blackboard APIs bind to thread **workspace** (not sidecar `cwd`); `GET /v1/blackboards` + `GET /v1/blackboards/{id}`; subagent done sentinel includes `structured_verdict` only when present; Verifier failures写入黑板；`<deepseek:craft.fix_loop>` 程序化修复提示；SSE `craft.verdict` / `craft.board_updated`。
- **Zagens (B-L3):** AgentPanel「CRAFT 任务」区域 — 轮询 `/v1/blackboards`，展示 explorer / 实现轮次 / reviewer 裁决 / verifier 摘要。
- **Docs:** `docs/tech/adr/IMPLEMENTATION_SUMMARY_2026-05-24.md` — 路线图门控链与 A/A+/P2/F/D10 实施现状归档；路线图 §17 已链入。
- **Runtime (A1.4):** `tui/history_isomorphism` — user/assistant transcript parity with `history_cells_from_message`; tests after compaction, trim, and JSONL reconstruct.
- **Runtime (A1.1):** `deepseek_core::context_partition` — hot window / cold zone tiers (`Hot`, `Pinned`, `ColdSummary`, `ColdExternalRef`); `CompactionPlan::context_partition`.
- **Runtime (A1.2):** Large tool output blobs persist under `~/.deepseek/sessions/<session_id>/large_outputs/` (`persist_large_output_blob`, workshop-ref round-trip test); registry hooks on routed synthesis when `state_namespace` is a session id.
- **Docs (A6.1):** `docs/tech/SANDBOX_CAPABILITY_MATRIX.md` — macOS / Linux / Windows enforcement vs policy-declaration matrix.
- **Runtime (A5.2):** `EngineConfig::llm_client_override` — inject `Arc<dyn LlmClient>` for mock-LLM engine tests (`engine_llm_client_override_runs_mock_turn`).
- **Runtime (A5.3):** Full-engine mock integration tests — compaction, parallel read-only tools, subagent spawn/list, capacity pre-request + mock LLM (`engine_mock_capacity_pre_request_observes_mock_and_emits_decision`).
- **Desktop (F3):** App shell Tab order — Composer before chat in DOM (flex `order` preserves layout); `main`/`aside` landmarks; `role="complementary"` on right panel.
- **Desktop (A+.5):** `runtime_proxy` path allowlist regression tests (`/health`, `/v1/*`; reject traversal and non-v1 prefixes).
- **Runtime (A1.2):** `large_output_tool_item_detail_matches_jsonl_and_persisted_blob` — turn item `detail`, JSONL `item.completed`, and `large_outputs/` blob agree on workshop-ref.
- **Runtime (A1.4):** `reconstruct_messages_matches_jsonl_item_completed_details` — turn-item reconstruct vs JSONL `item.completed` user-visible text isomorphism.
- **Runtime (A1.4):** `compact_messages_safe_preserves_pinned_text_in_result_messages` — compaction pin isomorphism regression.
- **Runtime (P2):** `Op::ApproveToolCall` / `DenyToolCall` route through `tx_approval` (same channel as `EngineHandle`).
- **Desktop (A+.3):** `KNOWN_DESKTOP_SSE_EVENTS` + `streamNormalize.selfcheck.ts` — unknown SSE events return `null`.
- **Desktop (F3):** `npm run test:f3` — roving tablist helper self-check (`rovingTabList.selfcheck.ts`).
- **Runtime (P2):** `SubAgentSpawnPort::list_subagents` — op-loop `ListSubAgents` delegates through port; tui adapter runs manager cleanup + list.
- **Runtime (A2):** `monitor_turn` logs `TurnSummary` on `TurnComplete` with `thread_id` + `turn_id`.
- **Runtime (P2):** `op_handlers.rs` — cancel/approve/deny/list/change-mode/query-context ops; `op_loop` match thinned further.
- **Runtime (P2):** `compaction_ops.rs` / `edit_turn_ops.rs` — manual compaction and `/edit` extracted from `message_handlers`; RLM/compaction delegate via `op_handlers`.
- **Docs:** `A2_TURN_OBSERVABILITY_V1_DRAFT.md` — A2.3 internal + L2 `turn_summary` alignment draft.
- **Runtime (A2):** `TurnSummary::log_turn_complete` — structured `tracing::info!` on engine turn end (aligned with `turn.completed` payload).
- **Runtime (P2):** `handle_spawn_subagent_op`, `apply_set_model_op` / `apply_set_compaction_op` — op-loop delegates; sync status emit folded into `sync_session_from_op`.
- **Runtime (A2 / A2.5):** `turn_streaming` / `turn_tools` `.instrument` spans on streaming + tool phases (`turn_id`/`step`); structured `tracing` events on turn loop + `monitor_turn`.
- **Runtime (P2):** `Engine::engine_context_snapshot` — `Op::QueryContext` delegates through `session_ops.rs`.
- **Runtime (P2):** `session::truncate_before_last_user_message` — `Op::EditLastTurn` via `message_handlers::handle_edit_last_turn`.
- **Runtime (P2):** `deepseek-core::session::apply_sync_session_payload` — `Op::SyncSession` via `session_ops::sync_session_from_op`.
- **Runtime (A2):** `deepseek-core::events::TurnSummary` — structured `turn_summary` on `turn.completed` (monitor uses core type, not ad-hoc JSON).
- **Runtime (P2):** `deepseek-core::session::{is_auto_model_label, apply_model_selection}` — op-loop `SetModel` / `SyncSession` via `session_ops.rs`.
- **Desktop (F3):** Skip links use `:focus-visible` focus ring (keyboard-only, aligned with primary controls).
- **Docs:** `G2_PR5_MANUAL_SMOKE_CHECKLIST.md` §8 — F3 keyboard a11y smoke (Tab / skip link / Escape / focus ring).
- **Runtime (A1.5):** `count_oldest_messages_to_drain` — batch `Vec::drain` instead of repeated `remove(0)` during emergency trim.
- **Runtime (A1-MVP.1):** `LargeOutputExternalRef` + `[workshop-ref: …]` header on routed large tool output.
- **Runtime (A1-MVP.2):** compaction end-to-end test — working-set pinned messages survive LLM summary (`compact_messages_preserves_working_set_pinned_message`).
- **Runtime (A1.3):** runtime thread event append + checklist/scratchpad metadata saves use `spawn_blocking`; crash-safe checkpoint table in `RUNTIME_BASELINE.md`.
- **Runtime (P2):** `lsp_edit_paths` in `deepseek-core` — edit-tool path extraction for LSP hooks (tui re-uses core).
- **Runtime (P2):** `SubAgentSpawnPort` in `deepseek-core::engine` — op-loop spawn surface; tui `subagent_spawn.rs` adapter.
- **Runtime (PR5):** `RuntimeThreadMessageTurnPort` — sidecar `ThreadMessageTurnPort` adapter over `RuntimeThreadManager::start_turn` + regression test.
- **Runtime (A3.4):** HTTP `ApiError` responses include `category` / `code` / `recoverable` / `severity` from `ErrorEnvelope`.
- **Runtime (PR5):** `ThreadMessageTurnPort` — `handle_thread(Message)` delegates real turn when port is wired; app-server installs `AppServerLlmTurnPort` when `api_key` is configured (legacy `queued` fallback otherwise).
- **Runtime (A1 / R-015):** Full baseline @ `10972e4` — median RSS **28.5 MB**, `-Gate` PASS; log `deliverables/runtime-baseline-full-run.log`.
- **Runtime (A1 / R-015):** `runtime-longrun-baseline.ps1` — deterministic **1.1 MB** workspace fixture for large-tool turn; `-Gate` RSS regression vs ADR baseline (+10%); CI ubuntu dry-run step.
- **Runtime (A1 / R-015):** `large_output_router` **1 MB+** boundary unit test (`synthesise_at_one_megabyte_boundary`).
- **Docs:** G2/PR5 手测签收（2026-05-23）：§0.4 health、§1 单窗、§3 双窗并行、§5.1 Stop；审批 UI 暂缓（`approval_policy` ↔ `auto_approve` 接线债）。
- **Runtime (PR5 局部):** 双 thread 并行 turn 回归测（`parallel_turns_on_two_threads_overlap_then_complete`、`sidecar_parallel_turns_on_two_threads`）；`app-server` `Message` 仍 queued 占位（注释 SSOT）。
- **Runtime (G2 门控):** `CURRENT_EVENT_SCHEMA_VERSION`；`GET /health` 与 SSE payload 暴露 `event_schema_version`（A+.4b）。
- **Runtime (tests):** A5.5 完整回放 fixture `runtime_turn_replay.jsonl`（15 步：thinking/tool/approval/完成）。
- **Docs:** `docs/tech/adr/G2_PR5_MANUAL_SMOKE_CHECKLIST.md` — G2/PR5 桌面与多窗口手测勾选清单。
- **Docs:** `docs/tech/adr/G2_GATE_ACCEPTANCE.md` — G2 自动化验收记录与维护者待办。
- **Docs:** `docs/tech/adr/P2_PR4_SESSION_HANDOFF.md` — 新窗口继续 P2 PR4 / A4.6 / R-015 的对接说明。
- **Runtime (P2 PR4 局部):** `deepseek-core::engine::{dispatch,context}`（工具 JSON/上下文预算/plan 策略）；tui 薄 re-export；`RegistryToolDispatch` 接线 `execute_tool_with_lock`；`Engine`/`turn_loop` 仍留 tui。
- **Runtime (P2 PR4 局部):** `deepseek-core::engine::approval`（`await_tool_approval` / `recv_user_input_for_tool`、泛型 `ApprovalDecision<P>`）；tui `approval.rs` 薄壳（`UserInputRequired` 事件仍 L2）；core 加 `tokio`/`tokio-util`。
- **Runtime (P2 PR4 局部):** `deepseek-core::engine::{tool_bridge,tool_progress}`（`ToolCall`↔`ToolOutput` 转换、`emit_tool_audit`、进度文案）；tui `tool_dispatch_port` / `tool_execution` 薄壳（`RegistryToolDispatch`、`InteractiveTerminalGuard`、MCP/进度仍 L2）。
- **Runtime (P2 PR4 局部):** `deepseek-core::{events,error_taxonomy,coherence,user_input,subagent}` + tui re-export（`Event`/`ErrorEnvelope`/`CoherenceState`/`UserInputRequest`/subagent 类型）；`envelope_from_llm_error` 保留 tui（`LlmError` 孤儿规则）。
- **Runtime (P2 PR4 局部):** `TurnContext`/`TurnLoopMode`/`StreamError` 迁入 `deepseek-core`；`TurnLoopHost` + `tool_phase.rs` / `streaming_phase.rs`；**`deepseek-core::engine::handle_deepseek_turn`**（generic `TurnLoopHost`）。
- **Runtime (A4.6 局部):** `engine.rs` 拆出 `types.rs`（`EngineConfig`）、`handle.rs`、`engine_new.rs`、`engine_helpers.rs`、`session_messages.rs`、`mock.rs`；`engine.rs` ~618 → **~201 行**（达 PR4 spike **< 300** 目标）。
- **Runtime (A4.6 局部):** `engine.rs` 拆出 `op_loop.rs`、`cycle_hooks.rs`、`message_handlers.rs`（`handle_send_message` / 手动 compaction）；`engine.rs` ~2177 → ~1220 行。
- **Runtime (P2 PR4 局部):** `TurnLoopToolExecutor` + `TurnLoopToolRegistry` 关联类型；`Engine` / `McpPoolHandle` 端口实现。
- **Runtime (tests):** A5.5 最小回放 fixture `tests/fixtures/runtime_turn_minimal.jsonl` + 顺序/seq 断言。
- **Runtime (P2 PR4 局部):** `deepseek-core::engine::tool_catalog`（deferral、tool search、missing-tool 文案）；tui 薄壳保留 `AppMode` 适配与 `code_execution` 子进程。
- **Docs:** `docs/tech/adr/P2_DESKTOP_TURNLOOP_SPIKE.md` — Zagens 经 sidecar HTTP 使用 `TurnLoopHost`（tui `host_impl`），desktop crate 不链接 `Engine`。
- **Runtime (A4.6 局部):** `engine/capacity_flow/{checkpoints,observation,events,interventions,replay,persistence}.rs`；原 monolith ~985 行拆为 6 个子模块（最大 ~370 行）。
- **Runtime (A4.6 局部):** `runtime_threads/turn_control.rs`（`interrupt_turn` / `steer_turn` / `compact_thread`）；`manager.rs` ~829 → ~589 行。
- **Runtime (A4.6 局部):** `runtime_threads/thread_crud.rs`（create/list/get/update/fork/resume/seed 等）；`manager.rs` ~1673 → ~1032 行。
- **Runtime (A4.6):** `runtime_threads/engine_load.rs`（`ensure_engine_loaded`）；`routing.rs` 路由读写。
- **Runtime (tests):** `runtime_api` 测试显式隔离 `data_dir`，不再受工作区 `DEEPSEEK_RUNTIME_DIR` 污染。
- **Runtime (A4.6):** `runtime_threads/routing.rs` 自 `manager.rs` 拆出路由规则读写。
- **Runtime (A4.6):** `runtime_threads/{active,monitor}.rs` 自 `manager.rs` 拆出（LRU/活跃 turn 状态 + `monitor_turn`）；`manager.rs` ~1.8k 行。
- **Runtime (P2 PR3 局部):** `deepseek-core::engine::{StartTurnParams, TurnEnginePort}`；`RuntimeThreadManager::start_turn` 经 core 委托 `EngineHandle`（`turn_loop` 仍在 tui）。
- **Docs:** [RUNTIME_EVOLUTION_ROADMAP.md](docs/tech/RUNTIME_EVOLUTION_ROADMAP.md) **v2.0-final** — 维护者签收 §4.2（D4–D7、D9）；§17 实施后审核（2026-05-22）；[adr/RUNTIME_BASELINE.md](docs/tech/adr/RUNTIME_BASELINE.md) R-015 占位（基准填数并行）。
- **Runtime (P2 PR1 局部):** Shared types and `LlmClient` trait in `deepseek-core` (`chat`, `models`, `turn`, `compaction`, `capacity`, `workshop`, …) with `deepseek-tui` re-exports; `deepseek-tui` **lib** target (`crates/tui/src/lib.rs`).

### Changed

- **Runtime (R-003 / A4.6 阶段 3):** Extract `runtime_threads/tests.rs`；`mod.rs` ~275 行（契约测外置）。
- **Runtime (P2 PR2 局部):** Move `Session`/`SessionUsage`、`working_set`、`project_context`、`ApprovalMode`、`CycleBriefing` into `deepseek-core` with tui re-exports; `turn_loop` still in `deepseek-tui::core::engine`.
- **Runtime (R-015):** `runtime-longrun-baseline.ps1` — release sidecar (fix debug stack overflow on Windows), load repo `.env`, poll turn `in_progress` between turns; ADR full-run RSS **26.6 MB** median @ `ab4c3c4` (`deepseek-v4-pro`, 50×3 turns); dry-run p99 **0.16 ms**.
- **Runtime (R-003 / A4.6 阶段 2):** Extract `runtime_threads/manager.rs`（`RuntimeThreadManager`、LRU、routing、turn 生命周期）；`mod.rs` ~2.4k 行（契约测仍留主文件）；`manager.rs` ~2.9k 行待后续 `tests.rs` 外置。
- **Runtime (R-003 / A4.6 阶段 1):** Extract `runtime_threads/persist.rs`（`RuntimeThreadStore` + 磁盘/事件/usage 聚合）与 `events.rs`（agent rebind hints）；`mod.rs` ~5.2k 行（`RuntimeThreadManager` 仍留主文件）。
- **Runtime (R-003 / A4.5):** Extract `health.rs`、`workspace.rs`、`usage.rs`（含 routing/symbol-index）；契约测迁至 `runtime_api/tests.rs`；`mod.rs` ~500 行（达 §12.6 <800 目标）。
- **Runtime (R-003 / A4.5):** Extract `runtime_api/skills.rs`、`mcp.rs`、`automations.rs`。
- **Runtime (R-003 / A4.5 部分):** Extract `runtime_api/sessions.rs`（会话 + resume-thread 播种）与 `runtime_api/tasks.rs`（后台任务队列）。
- **Runtime (R-003 / A4.4):** Extract `runtime_api/threads.rs` — thread CRUD, turns, snapshots, workspace browse/read (~1k lines).
- **CI (R-009):** Ubuntu job runs `sidecar_contract_full_lifecycle` explicitly (`cargo test -p deepseek-tui --lib`).
- **Runtime (R-003 / A4.2–A4.3):** Extract `runtime_api/auth.rs` and `runtime_api/stream.rs` from monolith `mod.rs`.

### Fixed

- **Runtime (RLM):** `RlmLlmClient` blanket impl uses `?Sized` so `Arc<dyn LlmClient>` compiles after `LlmClient` moved to `deepseek-core`.
- **Tests:** `cargo test -p deepseek-tui --lib` green (2368 passed) — JSON-only fixtures for schema-rejection tests (SQLite migration), `read_file` metadata key `total_lines`, Windows `pwsh` shell/display_command, approval resolve `tokio::join!` + stale-turn immediate deny, mock-engine turn timeout 8s for `QueryContext` panel emit.
- **Tests:** `subagent` stub runtime wraps client in `Arc` for P2 client type.

### Changed

- **Runtime (R-003 / A4.1):** Extract `runtime_api/router.rs` (`build_router`); handlers remain in `mod.rs` for now.

## [0.4.3] - 2026-05-21

### Zagens (desktop)

- **v0.4.3** — `deepseek-desktop`、`tauri.conf.json`、`web-ui/package.json` 与 About 面板对齐 **v0.4.3**。

### Fixed

- **Zagens (desktop):** Fix multi-window / continued-session chat stream duplication (`看到了看到了` / `TheThe user`) — runtime SSE proxy uses `emit_to` per window; Web UI binds SSE via `getCurrentWebviewWindow().listen`; resumed turns poll `replay_only` events instead of a long-lived `GET …/events` SSE (avoids stacked `runtime_get_sse` streams); `runtime_cancel_sse` stops in-flight proxy reads on abort; `finishOnce` aborts the turn `AbortSignal` after `turn.completed`.

## [0.4.2] - 2026-05-21

### Zagens (desktop)

- **v0.4.2** — `deepseek-desktop`、`tauri.conf.json`、`web-ui/package.json` 与 About 面板对齐 **v0.4.2**。

### Added

- **Zagens (desktop):** True multi-window (Cursor / VS Code model) — `WebviewWindow` per project, `tauri-plugin-single-instance`, tray/menu **新建窗口**, TitleBar + **Ctrl/Cmd+Shift+N**; per-window workspace `localStorage`, session list filter + **显示全部会话**; parallel turns per `thread_id` (switch session no longer aborts other streams); terminal `emit_to` per window; approval routed via `register_window_thread` / `thread_owned_by_window`.
- **Docs:** [multi-window-plan.md](docs/desktop/multi-window-plan.md) — multi-window plan **closed** (M1–M4 shipped; M5 deferred to backlog §7.5).

## [0.4.1] - 2026-05-21

### Zagens (desktop)

- **v0.4.1** — `deepseek-desktop`、`tauri.conf.json`、`web-ui/package.json` 与 About 面板对齐 **v0.4.1**。

### Added

- **Docs:** [workspace-directory-plan.md](docs/desktop/workspace-directory-plan.md) — workbench Directory tab phased UI/feature plan with implementation checklist (§0, §10).
- **Zagens (web UI):** Workbench Directory tab — flat stroke icons, toolbar (up/refresh/open folder), search filter, hidden-folder toggle (`target`, `node_modules`, etc.), merged workspace path row, scrollable list, preview highlight, `WorkspaceFilesPanel` + i18n `workspaceFiles.*`.
- **Zagens (web UI):** Workbench Directory **tree view** (phase D) — lazy-loaded `WorkspaceFileTree`, per-workspace expanded-state in `sessionStorage`, list/tree toggle with flat stroke icons.
- **Zagens (web UI):** i18n for workbench panel — `panels.*`, `workbench.*`, `workspaceFiles.tab` / `workspaceFiles.errors.*`; `RightPanel` and workspace file open errors use `useT`.
- **Zagens (web UI):** Tasks panel — **Clear finished** removes completed / failed / canceled records via runtime `POST /v1/tasks/clear` (queued and running tasks kept); confirmation dialog + toast.
- **Zagens (web UI):** Workbench Files tab — workspace-wide file search (same input box, BFS via browse API; skips denylisted dirs), virtual list for large flat/search result sets (B4); chat/Diff「在目录中显示」, scroll-to-reveal, keyboard shortcuts, Office directory presets.
- **Zagens (Phase D1):** Audit scratchpad — expandable inventory list from `scratchpad/status` `areas[]`, U1 contract violation highlight (notes without accounted areas), i18n strings; path click opens workspace preview.
- **Zagens (web UI):** Audit scratchpad colors aligned with app theme (`bg-card`, `text-t-text`, accent/error tokens) — readable in light mode; inventory status chips match ToolCard-style badges.
- **Zagens (Phase D2):** Scratchpad status API — `checklist_completed/total`, `contract_warnings`, findings severity tallies; audit panel dual-track (inventory vs checklist), findings strip, sub-agent active count + narrative-spawn warning; checklist tool events refresh panel.
- **Zagens (Phase D U2):** Sidebar separates **Tasks** (`GET /v1/tasks`) and **Sub-agents** (`agent_*` SSE) into top-level inspector entries; **Skills** stays under Settings. Checklist / Tasks / Sub-agents show a **small activity dot** when there is in-flight work or unseen updates (pulse while running); opening the panel clears the indicator.
- **Zagens (web UI):** **Usage** dashboard moved to the same sidebar tier as Checklist / Tasks / Sub-agents (display panels); Settings keeps API Key, MCP, Skills, routing, index, and system config only.
- **Zagens (web UI):** Audit scratchpad moved from the chat composer strip to a sidebar **审计** entry and right **Audit scratchpad** panel (same behavior as Checklist); sidebar activity dot when a run is active.
- **Zagens (web UI):** Chat **Reasoning** and **tools** sections get clipboard copy (section header + per-tool card); uses shared `copyPlainText` helper.
- **Zagens (web UI):** Sub-agent panel shows spawn **objective**, type/role, work-package id, and progress line; runtime `agent.spawned` SSE includes `prompt`; bogus `call_*` tool-call ids are no longer listed as agents.
- **Zagens (panel channel C):** Runtime emits `panel.scratchpad` / `panel.checklist` / `panel.context` on the live SSE stream; Web UI applies them directly and uses slow B-channel polls only as fallback while streaming.

### Changed

- **Zagens (web UI):** Audit scratchpad panel — uniform card border on all sides (removed thick left accent stripe); attention state uses a subtle amber border tint instead.
- **Zagens (web UI):** Remove redundant「打开文件夹」button from workbench panel header; use「在文件管理器中打开」in the Files tab toolbar instead.

### Fixed

- **Zagens (web UI):** Workbench Files「添加至对话」/「Add to chat」 now inserts an `@` workspace path into Composer instead of opening the preview panel.
- **TUI / runtime (security, L7d P1/P2):** Session `/load` `/save` `/export` paths resolved under workspace (`path_guard`); MCP no longer trusts client `approved` for shell/write tools when `require_approval` is on; default MCP expose list is read-only (`file_read`, `search`, `file_search`); cancel token reset order fixed (H01); `Config` Debug redacts API keys; file-picker labels strip control chars; scratchpad coverage preview uses char-safe truncation; Linux/Windows sandbox types surface an explicit unenforced warning on `exec_shell` (H12); Python REPL spawns with `-I` and docs state no OS isolation (H13).
- **Zagens (security, L7d follow-up):** P0 from `2026-05-20-001` audit — `export_*_json` validates `.json` path (no `..`/system dirs); runtime Bearer no longer exposed via `get_runtime_token` (Tauri `runtime_http` / `runtime_post_stream` / `runtime_get_sse`); Explore sub-agent `explicit_tools` intersected with read-only cap; blackboard `task_id` restricted to safe charset.
- **Zagens (web UI):** Mermaid SVG, diff2html output, and clipboard HTML paste sanitized with DOMPurify before `innerHTML`.
- **Zagens (web UI):** Long-audit HTTP poll storm — coalesced in-flight GETs for context/checklist/scratchpad status; longer staggered intervals while streaming; session checkpoint 60s (turn-complete + tab-hide still persist); runtime probe 18s during stream with immediate light `/health` on stream start.
- **Zagens (runtime):** `scratchpad/status` and `checklist` handlers run on the blocking pool (2s status cache) so `/health` and SSE stay responsive under audit load.
- **Zagens (web UI):** Sidebar「未连接」during long audits while generation still runs — periodic probe no longer requires `/v1/sessions` (2.5s timeout) while streaming; uses `/health` only so busy sidecar is not misread as offline.
- **Zagens (web UI):** Right panel (workspace browse, MCP, tasks/skills, routing, usage) no longer hard-blocks on probe `offline` during streaming; session-list refresh failures use light probe; sidebar shows amber「繁忙（生成中）」when degraded.
- **Zagens (web UI):** `runtimeSessionEstablished` keeps checklist, workspace, audit bar, and MCP panels on API paths after connect/resume — probe blips no longer gate panel fetches; probe requires 3 consecutive failures before `offline`; poll GETs use 45s timeout and retain last checklist/scratchpad snapshot on busy errors.

## [0.4.0] - 2026-05-20

### Zagens (desktop)

- **v0.4.0** — `deepseek-desktop`、`tauri.conf.json`、`web-ui/package.json` 与 About 面板对齐 **v0.4.0**。

### Added

- **Zagens (web UI):** Runtime-aligned context usage via `GET /v1/threads/{id}/context` (TUI `estimate_input_tokens_conservative` + compaction policy); Composer shows runtime estimate when sidecar is connected.
- **Zagens (web UI):** Fix context usage indicator resetting to 0% after switching sessions and back (per-thread snapshot cache, stale refresh guard, transcript fallback when runtime snapshot is empty).
- **Zagens (web UI):** Dual-track context display — progress ring uses conservative estimate; Composer also shows last API `input_tokens` from the provider when available.
- **TUI / runtime:** Engine records per-round API `input_tokens` (`last_api_input_tokens`); context snapshot exposes `last_api_usage_percent`; token estimate uses DeepSeek doc ratios (CJK ~0.6, ASCII ~0.3 per char).
- **Zagens (system settings):** `[compaction]` — `auto_compact` toggle and `token_threshold` (synced to `config.toml`, shared with TUI engine compaction).
- **Audit scratchpad (Phase B):** Runtime tools `scratchpad_*`; `ScratchpadStore` + layered P2 summary injection, readonly nudge (B4), cycle handoff pointer (B3b), `ThreadRecord.scratchpad_run_id` (B2), TTL cleanup (B7), `GET /v1/threads/{id}/scratchpad/status`, Zagens `AuditScratchpadBar` (B5). Config: `[scratchpad]` in `config.toml`.
- **Audit scratchpad (B7 hardening):** `supersedes` transitive closure; `scratchpad_append` schema tightened; per-turn single `<scratchpad_summary>`; `git_blame` counts toward readonly nudge.
- **Audit scratchpad (Phase A):** Full-repo review external memory — `pick-rules.md` §7, `base.md`, bundled **`audit-repo`** skill. Design: [audit-scratchpad-design.md](docs/desktop/audit-scratchpad-design.md).
- **Docs:** [audit-scratchpad-test.md](docs/desktop/audit-scratchpad-test.md) — Phase A smoke, resume, and **14-area** `crates/tui/src/` run (`2026-05-19-tui-src-review`).
- **Docs:** [audit-scratchpad-test.md](docs/desktop/audit-scratchpad-test.md) — Phase B smoke/gate/resume/synthesize on `2026-05-19-phase-b-smoke`; design §6 marked implemented.
- **Docs:** [audit-scratchpad-design.md](docs/desktop/audit-scratchpad-design.md) — Phase C plan §6.12 (C0–C4: compaction, coverage gate, Auditor binding, blackboard mirror); §6.10 B7 marked shipped.
- **Docs:** Phase C design review §13.5 — deferred meta gate, L0-only compact, C1/B1 boundary, Auditor track B prose check, soft-warn format.
- **Audit scratchpad (Phase C0/C1):** Compaction pin + L0-only handoff; `coverage_gate` (soft/hard); `set_area(deferred)` requires `kind=meta`; `[scratchpad]` config fields.
- **Audit scratchpad (Phase C2/C3):** `agent_spawn(type=auditor)` builds track A/B from scratchpad; blackboard `scratchpad` mirror partition; `scratchpad_run_id` spawn param.
- **Skills:** `audit-repo` — append-before-`done` ordering; bundled skills marker **v4** (`tool_search` before `write_file`; resume via `scratchpad_status`).
- **Docs:** [audit-scratchpad-design.md](docs/desktop/audit-scratchpad-design.md) §2 — product essence, philosophy (**实事求是，实践出真知**), §2.1–§2.6 (contract, onboarding brainstorm, multidisciplinary memo, short-term roadmap).
- **Docs:** [audit-scratchpad-test.md](docs/desktop/audit-scratchpad-test.md) §L7b — root-cause table and link to §14.
- **Docs:** [HARNESS.md](docs/desktop/HARNESS.md) — Agent Harness 定位（社招 JD 映射、Zagens 栈位、会话恢复案例、与 DeepSeek 关系备忘 §7）。
- **Docs:** [audit-scratchpad-design.md §6.13](docs/desktop/audit-scratchpad-design.md) — Phase D 审计过程可视化路线图（D1–D3、产品/模型边界）；[audit-scratchpad-test.md §L7c/L8/L9](docs/desktop/audit-scratchpad-test.md) — `2026-05-20-audit` 试跑记录、可视化验收、地狱级四维暂缓。

### Fixed

- **Zagens (web UI):** Saving system settings restarts the sidecar — UI no longer stays on「生成中」while the sidebar shows「未连接」; `sidecar://restarting` clears the stream; confirm dialog when saving during an active turn.
- **Zagens (web UI):** Stop assistant reply body **jitter while streaming** — plain pre-wrap during tokens (Markdown after turn completes), single scroll owner (no outer 200ms poll vs inner 48vh cap), fixed-height「生成中」footer.
- **Zagens (desktop):** Content-Security-Policy — add `font-src` (`'self'`, `data:`, dev localhost / `tauri.localhost`) so bundled UI fonts are not blocked (console CSP violation on `data:font/woff2`).

### Changed

- **Zagens (web UI):** Audit scratchpad bar — dismiss control (×, top-right); hidden for the same thread/run until a new scratchpad run (sessionStorage).
- **Zagens (web UI):** Audit scratchpad bar — neutral `canvas-alt` shell (aligned with tool cards); contract reminders use amber pill + left rail instead of full error red styling.
- **Zagens (web UI):** Context ring prefers runtime snapshot over client transcript estimate; polls during streaming.
- **Audit scratchpad (L7b short-term):** Expand `[scratchpad] inject_on_report_keywords` (E1); block `write_file` to `deliverables/` audit/CODE_REVIEW paths when bound scratchpad inventory incomplete or C1 hard gate fails (E2, `scratchpad_flow::check_write_file_audit_report_gate`); **E5** — during bound audit scratchpad defer/block `task_create` and eager-load `agent_spawn` (+ join tools) so P1 parallel review uses sub-agents not TaskManager.
- **Zagens / config:** `[subagents] step_timeout_secs` — configurable default per-step sub-agent LLM API timeout (10–600 s); system settings slider; `agent_spawn` uses it when `step_timeout_ms` is omitted (replaces hard-coded 120 s default).
- **Sub-agents / prompts:** Step API timeout errors and `base.md` / `audit-repo` spell out that omitted `step_timeout_ms` is not unlimited time; parents must re-spawn or shrink scope on timeout — not mark audit areas done.
- **Prompts / tools:** Clarify **Task** (`task_*`, peer durable work) vs **Sub-agent** (`agent_*`, parent-dispatched) in `base.md`, `tasks.rs` tool descriptions, and `agent_spawn` / `agent_result` / `agent_list` descriptions; `task_id` spawn param documented as blackboard key only.
- **Zagens (web UI):** Custom context menu on chat workspace file links — open with system app, copy absolute path, copy relative path; suppresses the native WebView link menu (e.g. non-functional “open in new window” on `href="#"`).
- **Zagens (web UI):** Right workbench panel **collapsed by default** on launch (left sidebar stays open); use the edge strip to expand.
- **Zagens (web UI):** Composer **Stop** calls `POST …/turns/{turn_id}/interrupt` (runtime `engine.cancel()`), not only aborting the SSE client—matches TUI Ctrl+C / Esc interrupt semantics.
- **Audit scratchpad (discoverability):** `scratchpad_*` tools eager-loaded in Agent (not deferred); `scratchpad_status` / `scratchpad_list_notes` bind `thread.scratchpad_run_id`; `GET …/scratchpad/status` auto-discovers latest `inventory.json` when unbound (Zagens bar). `audit-repo` skill + `base.md` / pick-rules §7: `tool_search` before `write_file` fallback.
- **Zagens (web UI):** Audit bar shows **accounted** progress (done + in_progress + deferred), faster poll while streaming, refresh on scratchpad tool completion. **Sub-agent panel:** forward `agent.spawned` / `agent.progress` / `agent.completed` / `agent.list` on compat SSE (`POST /v1/stream`).
- **Zagens (web UI):** Dark theme — user message text uses dedicated high-contrast `--color-msg-user-text` (fixes faint prose grays in user bubbles).
- **Zagens (web UI):** While an assistant reply is **streaming**, the main body uses the same **48vh scroll cap** as Reasoning so CoT stays on screen; after the turn completes, the body **ease-out expands** to full height (respects `prefers-reduced-motion`).
- **Zagens (web UI):** Sidebar / right-panel **collapse** controls move to the resize gutter—hidden until hover on the `col-resize` seam; panel-indent icon replaces header chevrons.
- **Zagens (web UI):** Global **toast** notifications (no new npm deps) replace the chat-column amber **banner**; stack centered above the composer with success / error / warning / info variants; runtime reachability errors include **Retry connection** and auto-dismiss when the sidecar probe is healthy.
- **Zagens (web UI):** Assistant **Reasoning** and **工具调用** blocks default to **collapsed**; click header to expand (streaming shows “推理中…” / “N 个进行中” hints while folded).
- **Docs:** [README.md](README.md) — lead with verified differentiators; split desktop vs shared runtime; trim misleading feature tables; fix dev commands (`cargo tauri dev`); align doc links (`API_DESIGN.md`, `DEV_NOTES.md`). Cursor/portable rules updated for dead links.

## [0.3.0] - 2026-05-19

### Zagens (desktop)

- **v0.3.0** — `deepseek-desktop`、`tauri.conf.json`、`web-ui/package.json` 与侧栏标签对齐 **v0.3.0**。
- **会话回放与 diff** — 会话回放、diff 面板、上下文用量展示；Code 工作区 **交互式 PTY 终端**；切换会话时恢复上下文用量；聊天代码块复制。
- **Composer UI** — 新版输入区/composer 布局；Mermaid 渲染修复。
- **系统设置** — 完整系统设置视图迁入右侧面板；**系统托盘**（关闭窗口可隐藏到托盘）；turn 完成且窗口最小化/隐藏时 **原生桌面通知**。
- **i18n** — Web UI 国际化框架（中/英等）。
- **子代理设置** — 与 TUI 双轨对齐：`max_subagents` 优先级与 subagents 功能开关。
- **侧栏/技能** — 技能导入；Office/Code 任务类型；Documents 默认工作区；搜索类工具 UI/配置。
- **文档** — 技术文档迁至 `docs/tech/`；diff 面板暗色主题修复。

### Added

- **符号索引 V3–V5** — 懒加载按工作区构建（去除启动阻塞）；MermaidPanel 图表实时渲染；`edit_file` v0/v1；索引管理面板；符号变更追踪与 bridge 关联（V5）；调用关系与置信度（见 `docs/symbol-index-v*.md`）。
- **Sidecar 加固** — Supervisor 强化、SQLite 存储、启动门控、keyring 密钥。
- **CRAFT** — P0 结构化裁决、P1 黑板、P3 工具裁剪、fix-loop 协议（`docs/craft-v2-improvements.md`）。
- **Vision** — 默认切换 **Qwen3-VL**；模型感知提示词；代理/超时配置；`describe_image` 工具与 OCR bridge。
- **Office** — PDF 读取支持；`write_office` / xlsx 生产级修复（6 项）；工具进度流式输出与 LLM 使用指南。
- **搜索工具** — Office/Code 任务类型相关检索能力扩展。
- **提示词** — V4 幻觉抑制类系统提示调整。

### Changed

- **Zagens** — Light theme “极淡乳白” palette (warm stone canvas/card, softer dividers, blue accent retained); center chat column uses `card` surface.
- **Zagens** — Dark theme “深色暖 · 护眼” palette (warm gray-black shell, amber accent, softer status colors).
- **Zagens** — Sidebar brand row: flat logo + accent “Zagens” (no gray pill), aligned with reference mockups.
- **Zagens** — Bundled UI fonts: Plus Jakarta Sans Variable (Latin) + Noto Sans SC (CJK); JetBrains Mono for terminal/code; replaces system Segoe/Roboto stack.
- **Zagens** — Sidebar: app icon in brand row; title bar brand text removed; connection status at bottom (“连接正常” / “未连接”); version blurb moved to **关于** panel under Settings.
- **Runtime** — 消除 tokio worker 中的阻塞 I/O；诊断与相关文档更新。

### Fixed

- **Zagens** — After app restart, **Reasoning** and **tool** cards restore correctly: `resume-thread` reuses persisted `runtime_thread_id` for event replay (instead of seeding a blank thread); `persist-session` stores that id; web UI mirrors UI snapshots to `localStorage` as fallback.
- **Zagens** — Session restore `GET …/events?replay_only=1` no longer returns HTTP 400 (accept `1`/`0` query booleans; client uses `replay_only=true`).
- **Zagens** — Switching sessions keeps in-memory UI snapshots (tools + thinking); thread event replay still refreshes from runtime when available.
- **Zagens** — Checklist panel: sidebar **清单** entry, persist `checklist_write` on thread record (survives sidecar restart), faster poll while streaming; auto-switch no longer blocks manual **工作台** tab during streaming.
- **Zagens** — Left sidebar width is draggable on the column edge (persisted; 180px–45% viewport); fixes broken resize handle that was absolutely positioned without a containing block.
- **Zagens** — Unified divider tokens (`chrome-seam` vs `divider` vs `card-border`); column resize gutters use inset seams only; shell panels use tint bands instead of stacked border lines.
- **Desktop** — Unicode 工作区路径下的可点击聊天链接。
- **xlsx** — 生成路径六项生产级修复。

### Security

- CODE_REVIEW **Step 1** — 依赖与安全清理（见 `docs` 中 Step 1 跟踪）。

### Docs

- Agent 可靠性/CRAFT/符号索引/V5 等规划与实施记录更新；部分旧版 brief 归档或移除。

### Process

- **Changelog 维护** — 写入 `.cursor/rules/zagens-repo.mdc` 与 `project_rules.md`（notable 变更需同步本文件）。
- **Portable rules** — `project_rules.md` 聚合 `.cursor/rules/*.mdc`（原 `CURSOR_RULES.md` 更名）。

---

## [0.2.2] - 2026-05-11

### Zagens (desktop)

- **v0.2.2** — `deepseek-desktop`、Tauri `version`、`web-ui/package.json` 与侧栏标签对齐 **v0.2.2**；打包脚本与 bundled Python 准备（`prepare-python.mjs`、`docs/bundled-python-plan.md`）。
- **任务与技能** — 侧栏「任务与技能」；`POST /v1/skills` 创建 `SKILL.md` 模板；全局/工作区技能根与桌面文件夹选择器；定时自动化列表 UI 暂缓展示（`fetchAutomations` 保留）。
- 关闭应用时 **自动结束 sidecar**；Web UI 样式 refinements；用户消息气泡可读文本色修复；用量扫描性能、终端主题、**USD 成本** 标签。

### Added

- **办公文档** — `read_file` 支持 `.xlsx` / `.pptx` 文本提取；**`write_office`** 从 JSON 生成 `.xlsx`（Rust）、`.docx` / `.pptx`（Python，主题与 16:9 布局）；详见 [docs/office-doc-capability-plan.md](docs/office-doc-capability-plan.md)。
- **Python 运行时** — `python_env.rs`：`find_python()`、`ensure_office_venv()`（`~/.deepseek/office-py/`）；RLM / `code_execution` 去除硬编码 `python3`。
- **Runtime API** — `POST /v1/skills`；交互式工具审批 HTTP 流（`approval.required` + `resolve-approval`，默认 120s 超时）。

### Fixed

- Shell 工具终端 UX；PPTX 引擎资源扩展。

---

## [0.2.1] - 2026-05-10

### Zagens (desktop)

- **v0.2.1** — 独立桌面 SemVer；侧栏 **Zagens v0.2.1**（workspace crate 依赖仍为共享线，如 `0.8.15`）。
- **三栏 shell 增强** — 统一聊天壳、右侧面板（runtime API 对接）、devtools 开关、窗口权限与标题栏。
- **Markdown** — 聊天区 Markdown 渲染；`` `path/to/file` `` 与相对 `[](…)` 链接在工作台打开预览；预览区内相对链接 **应用内** 打开（外链新窗口，`#` 锚点滚动）。
- **启动** — Zagens 与 sidecar 就绪速度优化。

### Docs

- `docs/desktop/DEV_NOTES.md`；[TUI vs Zagens 差距表](docs/desktop/TUI_DS_PICK_GAP.md) 初版。

---

## [0.2.0] - 2026-05-09

### Zagens (desktop)

- **v0.2.0** — Zagens 产品化里程碑：文档布局、Cursor 规则、桌面与 runtime 一批修复。
- **Tauri 壳** — Sidecar 生命周期、三栏布局、会话文件同步、工作区与附件、运行模式。
- **工作台** — 文件预览模块（文本/图片/二进制）、安全 `read_workspace_binary`；`web_search` Bing 回退。
- **主题** — 明/暗样式与 Markdown 预览 demo 打磨。
- **安全** — CODE_REVIEW：sidecar token 环境变量、CSP、MCP 过滤、updater 相关加固。

### Added

- **Runtime HTTP** — Phase 1 SSE `id` 序列；Phase 2 Tauri + Web MVP；thinking/reasoning 经 SSE 流式输出；工具审批范围与超时配置（Phase 2a）。
- **TUI** — `read_file` / `file_info` 工具链；大文件 plaintext 流式读取；`file_info` RFC3339 `mtime`。

### Fixed

- 会话恢复时还原已保存工作区；sidecar 默认 `cwd` 合理化。

### Docs

- README Zagens 章节与仓库布局表；桌面文档迁入 `docs/desktop/`。

---

## [0.1.0] - 2026-05-07

### Added

- **Initial fork** — DeepSeek TUI desktop 工作区：共享 `deepseek` CLI/TUI/runtime crates 与 **Zagens**（`crates/desktop/`）骨架。
- **Runtime API** — `/v1/...` 契约与 [docs/RUNTIME_API.md](docs/RUNTIME_API.md) 实施文档（Phase 1）。

### Changed

- Desktop sidecar / main Rust 源码 `rustfmt`。
