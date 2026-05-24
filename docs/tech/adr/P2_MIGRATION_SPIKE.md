# P2 迁移 PR0 Spike 笔记（R-013）

> **日期：** 2026-05-22  
> **状态：** 设计笔记（无行为变更）  
> **参考：** [RUNTIME_EVOLUTION_ROADMAP.md](../RUNTIME_EVOLUTION_ROADMAP.md) §11.0–11.4

## 0. 目标

验证 §11.0 ADR 中定义的迁移边界在代码层面可行，输出 **trait 边界草图** +
**EngineConfig 落点** + **ThreadManager→core 调用草图**，不产生行为变更。

## 1. 关键决策确认

### 1.1 Crate 边界：扩展 `deepseek-core`
- `deepseek-core` 现有 ~1700 行 `Runtime`（含 `handle_thread` 占位）
- P2 将 Engine/turn_loop/session 迁入此 crate
- **不**新建 `runtime` crate — 避免循环依赖和编译单元碎片化
- ✅ 确认可行

### 1.2 迁入 core 的类型列表

| 类型 | 当前路径 | P2 后路径 |
|------|----------|-----------|
| `Engine` | `crates/tui/src/core/engine.rs` | `crates/core/src/engine.rs` |
| `EngineConfig` | `crates/tui/src/core/engine.rs` | `crates/core/src/engine.rs` |
| `EngineHandle` | `crates/tui/src/core/engine.rs` | `crates/core/src/engine.rs` |
| `turn_loop` | `crates/tui/src/core/engine/turn_loop.rs` | `crates/core/src/turn_loop.rs` |
| `Session` | `crates/tui/src/core/session.rs` | `crates/core/src/session.rs` |
| `TurnContext` | `crates/tui/src/core/turn.rs` | `crates/core/src/turn.rs` |
| `CompactionConfig` | `crates/tui/src/compaction.rs` | `crates/core/src/compaction.rs` |

### 1.3 留在 `deepseek-tui` 的类型

| 类型 | 理由 |
|------|------|
| `RuntimeThreadManager` | thread 生命周期、JSONL 持久化、broadcast — L2 适配层 |
| `runtime_api` | HTTP/SSE 路由 — L2 壳契约 |
| `tools/*` | 工具实现 — P2 不整体搬迁 |
| `mcp.rs` | MCP 池管理 |
| `LlmClient` / `DeepSeekClient` | P2 仅暴露 trait 边界；具体实现留在 tui |

### 1.4 工具回调 trait 边界

Engine 需要通过 trait 回调到工具注册表。当前 Engine 直接依赖 `crate::tools::*`，迁移后需要：
```rust
// deepseek-core 中定义
pub trait ToolRegistry: Send + Sync {
    fn execute(&self, name: &str, input: Value) -> BoxFuture<'_, Result<ToolResult, ToolError>>;
    fn list_tools(&self) -> Vec<ToolDef>;
}
```

**风险：** `ToolResult` 和 `ToolError` 也在 `deepseek-tools` crate 中（独立 workspace crate），不随 Engine 迁移。P2 PR1 阶段需要确认 trait 是否在 `deepseek-tools` 中定义，或新建在 `deepseek-core` 中。

### 1.5 ThreadManager→core 调用草图

```rust
// deepseek-core 中的入口
impl Runtime {
    pub async fn run_turn(
        &self,
        config: EngineConfig,
        session: Session,
        ops: mpsc::Receiver<Op>,
        events: mpsc::Sender<Event>,
        tool_registry: &dyn ToolRegistry,
    ) -> TurnOutcome {
        let mut engine = Engine::new(config);
        engine.run(config, session, ops, events, tool_registry).await
    }
}
```

**风险：** `Op` 和 `Event` 类型需要与 core 共享。当前在 `crates/tui/src/core/` 下 — P2 PR1 需平移到 `deepseek-core`。

## 2. PR 切片可行性评估

### PR1：类型/配置平移
- `EngineConfig`、`Op`、`Event`、`TurnOutcomeStatus` 平移到 core
- `deepseek-tui` 依赖 `deepseek-core`，通过 re-export 兼容
- 风险：**中** — 类型平移可能触发大量 import 变更

### PR2：turn_loop + session 迁移
- 核心逻辑迁入 core
- 回放测通过（需要 A5.5 fixture）
- 风险：**中** — session 模块与 `LlmClient` 紧密耦合

### PR3：RuntimeThreadManager 委托 core
- `start_turn` 调用 `core::Runtime::run_turn`
- 契约测通过（需要 A+.4）
- 风险：**低** — 清晰的委托模式

### PR4：削薄 tui engine.rs
- 目标 < 300 行（当前 ~2170 行）
- 删除已迁移到 core 的代码
- 风险：**低** — 纯删除操作

### PR5：多窗口并行 turn 回归
- `handle_thread(Message)` 委托 core
- 多窗口冒烟测试
- 风险：**中** — 需要桌面端冒烟

## 3. 已知风险与缓解

| 风险 | 缓解 |
|------|------|
| `StateStore` 与 JSONL 双持久化 | 标为 backlog，P2 不统一 |
| `LlmClient` trait 耦合 | PR0 确认 trait 在 core 中定义 |
| 类型平移触发大规模 import 变更 | 用 re-export 兼容层过渡 |

## 4. 下一步

> **新会话对接：** [P2_PR4_SESSION_HANDOFF.md](./P2_PR4_SESSION_HANDOFF.md)

- [x] 维护者签收 §11.0 ADR（G3 门）— [P2_G3_ENGINE_L2_SIGNOFF.md](./P2_G3_ENGINE_L2_SIGNOFF.md)（2026-05-23，L2 终态）
- [x] PR1 **局部**：`deepseek-core` 子模块（`chat`/`models`/`turn`/`compaction`/`capacity`/`workshop` 等）+ tui re-export（2026-05-22；**非** §12.3 完成）
- [x] PR2 **局部（2026-05-22）：** `session`、`working_set`、`project_context`、`ApprovalMode`、`CycleBriefing` → `deepseek-core`；tui 薄 re-export；`core::engine` 仅导出 session 类型
- [x] PR3 **局部（2026-05-22）：** `StartTurnParams` + `TurnEnginePort` in `deepseek-core`；`RuntimeThreadManager::start_turn` delegates via `EngineHandle::start_turn`（`turn_loop`/`Engine` 仍在 tui）
- [x] PR4 **局部（2026-05-23）：** `handle_deepseek_turn` + `TurnLoopHost` → `deepseek-core::engine::turn_loop`；tui `host_impl` / `streaming_phase` / `tool_phase` L2；`Event`/`error_taxonomy`/`coherence`/`subagent`/`user_input` → core
- [x] PR4 **engine.rs < 300 行（2026-05-23）：** 拆 `types`/`handle`/`engine_new`/`engine_helpers`/`session_messages`/`mock` + 早前 `op_loop`/`cycle_hooks`/`message_handlers` 等；**~201 行 @ `3264419`**
- [x] A4.6 **局部（2026-05-23）：** `engine/capacity_flow/{checkpoints,observation,events,interventions,replay,persistence}.rs`；monolith ~985 → 最大 ~344 行（`interventions.rs`）
- [x] A4.6 **局部（2026-05-23）：** `runtime_threads/turn_control.rs`（interrupt/steer/compact）；`manager.rs` ~829 → ~589
- [x] A4.6 **局部（2026-05-23）：** `runtime_threads/{thread_crud,turn_lifecycle}.rs`；`manager.rs` ~1673 → ~829
- [x] A4.6 **局部：** `runtime_threads/{routing,engine_load,active,monitor}.rs` 自 `manager.rs` 拆出
- [x] P2 PR4 **局部（2026-05-23）：** `deepseek-core::engine::tool_catalog`（策略 + tool search）；tui 保留 `code_execution` / `AppMode` 壳
- [x] P2 PR4 **Desktop spike（2026-05-23）：** [P2_DESKTOP_TURNLOOP_SPIKE.md](./P2_DESKTOP_TURNLOOP_SPIKE.md) + `deepseek-desktop` 架构边界测
- [x] P2 PR4 **局部（2026-05-23）：** `tool_execution/` 子模块 + `McpPoolPort`（`McpPoolHandle`）
- [x] A5.5 **回放 fixture（2026-05-23）：** `runtime_turn_minimal.jsonl` + `runtime_turn_replay.jsonl`（15 步）+ `runtime_event_replay_fixture` 测；见 [G2_GATE_ACCEPTANCE.md](./G2_GATE_ACCEPTANCE.md)
- [x] A+.4b **`event_schema_version` 代码（2026-05-23）：** `CURRENT_EVENT_SCHEMA_VERSION`、`/health`、SSE payload
- [x] A+.7 **审批回归（2026-05-23）：** approve/deny/非法 decision + turn 终态（`runtime_threads/tests.rs`）
- [x] PR5 **局部（2026-05-23）：** 双 thread 并行 turn 回归测；`handle_thread(Message)` 委托 `ThreadMessageTurnPort`（app-server 单轮 LLM）
- [x] **F0（2026-05-23）：** `start_turn_applies_route_intent_routing_rule_to_model` — `route_intent` 全链路透传
- [x] A+.4 **已有：** `sidecar_contract_full_lifecycle`（`runtime_api/tests.rs`，CI）
- [x] P2 PR4 **局部（2026-05-23）：** `TurnLoopToolExecutor` 使用 `TurnLoopToolRegistry` 关联类型；`Engine` + `McpPoolHandle` 实现
- [x] **P2 PR6（2026-05-24）：** `tool_parser` + `streaming_phase` + `tool_phase` + `capacity_policy` → `deepseek-core`；tui `tool_plans_exec` + `host_impl/{mod,capacity,no_tool_uses}`；见 [P2_PR6_TURN_LOOP_L2_MIGRATION_PLAN.md](./P2_PR6_TURN_LOOP_L2_MIGRATION_PLAN.md)

### 4.1 `turn_loop` L2 终态（PR6 后）

| 仍留 tui（L2 壳） | 已在 core |
|------------------|-----------|
| `Engine` 字段：`LlmClient`、`McpPool`、`LspManager`、`SubAgentRuntime`、事件通道 | `handle_deepseek_turn`、`streaming_phase`、`tool_phase`、`capacity_policy` |
| `spawn_engine` / `EngineConfig`、`tool_plans_exec`（并行/顺序执行） | `tool_parser`、`TurnLoopHost` trait、`loop_guard`、`streaming`、`dispatch`、`context` |
| `tool_execution`（MCP/终端/审批 `context_override`） | `tool_catalog` 策略、`approval`/`tool_bridge`/`compact_tool_result_for_context` |
| `capacity_flow/` 检查点与干预（`TurnLoopMode` 接线） | `should_run_capacity_error_escalation` 纯策略 |
| `AppMode`、TUI `ToolRegistry` builder | `chat::{Message,Tool}`、`ToolResult`、`TurnLoopMode` |

**建议下一刀：** D10 解冻评审（§12.3 已 G3 签收）；或 A1/A3 路线图 ROI；可选拆 `streaming_phase` 子模块（~700 行）。
- [ ] 远期：`Engine` struct 整包进 core（非 §12.3 阻塞）
- [ ] `capacity_flow` 全量端口（PR6d 仅策略 + `TurnLoopMode` 接线）
- [x] A5.5 回放 fixture 扩展（10–20 步）
- [x] A+.4 契约测就位（`sidecar_contract_full_lifecycle`）
