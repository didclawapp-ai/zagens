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

- [ ] 维护者签收 §11.0 ADR（G3 门）
- [x] PR1 **局部**：`deepseek-core` 子模块（`chat`/`models`/`turn`/`compaction`/`capacity`/`workshop` 等）+ tui re-export（2026-05-22；**非** §12.3 完成）
- [x] PR2 **局部（2026-05-22）：** `session`、`working_set`、`project_context`、`ApprovalMode`、`CycleBriefing` → `deepseek-core`；tui 薄 re-export；`core::engine` 仅导出 session 类型
- [x] PR3 **局部（2026-05-22）：** `StartTurnParams` + `TurnEnginePort` in `deepseek-core`；`RuntimeThreadManager::start_turn` delegates via `EngineHandle::start_turn`（`turn_loop`/`Engine` 仍在 tui）
- [x] PR4 **局部（2026-05-22）：** `loop_guard` + `streaming` + `dispatch` + `context` + **`approval`** + **`tool_bridge`/`tool_progress`** → `deepseek-core::engine`；`RegistryToolDispatch` 接线 `execute_tool_with_lock`；tui 薄 re-export（`dispatch` 仍含 `ToolExecutionPlan` + `arg_repair`；`approval` 仍发 `UserInputRequired`；`tool_execution` 仍含终端 pause/MCP/并行）；`Engine`/`turn_loop` 仍留 tui
- [x] A4.6 **局部：** `runtime_threads/{routing,engine_load,active,monitor}.rs` 自 `manager.rs` 拆出
- [ ] PR4 剩余：`Engine` + `turn_loop` + 子模块迁入 core；`engine.rs` < 300 行

### 4.1 `turn_loop` 迁入前置（2026-05-22 草图）

| 仍留 tui 直至壳层就绪 | 已可在 core 复用 |
|----------------------|------------------|
| `Engine` 字段：`LlmClient`、`McpPool`、`LspManager`、`SubAgentRuntime`、事件通道 | `session`、`loop_guard`、`streaming`、`dispatch`、`context` |
| `EngineConfig` 构建（`spawn_engine`）、`turn_loop/run.rs` 主体（compaction/MCP/工具执行） | `Event`、`TurnEnginePort`、`TurnLoopHost`、`TurnContext`、`turn_loop::helpers` |
| `tool_catalog`、`tool_execution`（执行锁/MCP/终端 guard）、`capacity_flow` | `compact_tool_result_for_context`、`RegistryToolDispatch`、`tool_bridge`、`tool_progress`、`await_tool_approval` |
| `AppMode`、TUI `ToolRegistry` builder | `chat::{Message,Tool}`、`ToolResult` |

**建议下一刀：** 扩展 `TurnLoopHost`（compaction、MCP pool、`execute_tool_with_lock`）后将 `turn_loop/run.rs` 迁 core。~~`Event`~~、~~helpers~~、~~`TurnLoopHost` 脚手架~~ 已就绪（2026-05-22）；`run.rs` 仍 L2。
- [ ] PR2 剩余：`turn_loop` + `Engine` 迁入 core（须 `EngineToolDispatch` 接线）
- [ ] PR1 剩余：`Engine`/`turn_loop` 主逻辑迁入 core
- [ ] A5.5 回放 fixture 就位
- [ ] A+.4 契约测就位
