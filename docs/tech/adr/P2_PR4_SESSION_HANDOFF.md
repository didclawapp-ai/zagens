# P2 PR4 / R-015 / A4.6 — 新会话对接（2026-05-23）

> **用途：** 在新 Cursor 窗口继续本方案时，把本文 + 下方 **§7「复制给 Agent 的提示」** 一并贴上。  
> **权威路线图：** [RUNTIME_EVOLUTION_ROADMAP.md](../RUNTIME_EVOLUTION_ROADMAP.md) §11–12、§17  
> **迁移笔记：** [P2_MIGRATION_SPIKE.md](./P2_MIGRATION_SPIKE.md)  
> **基线 ADR：** [RUNTIME_BASELINE.md](./RUNTIME_BASELINE.md)

---

## 1. 我们在做什么

在 **不新建 `runtime` crate** 的前提下，把 **Engine 可复用逻辑** 迁入 `deepseek-core`，TUI/Desktop 保留 **L2 壳**（`RuntimeThreadManager`、`runtime_api`、`ToolRegistry` 实现）。并行做 **A4.6** 拆分大文件，以及 **R-015** 长跑 RSS 基线。

**PR4 spike 收官标准（engine 壳）：** `crates/tui/src/core/engine.rs` **< 300 行** — **✅ 已达标（~201 行 @ `3264419`）**。  
**PR4 仍开放：** `tool_execution` 等 L2 执行路径仍在 tui；Desktop 经 sidecar 已满足 `TurnLoopHost`（见 [P2_DESKTOP_TURNLOOP_SPIKE.md](./P2_DESKTOP_TURNLOOP_SPIKE.md)）。

---

## 2. 已完成（可直接当事实用）

### Git 锚点

**HEAD：** `0d8523e` — `G2 gate: event_schema_version, A5.5 replay fixture, acceptance record.`
- 上一刀：`a2a62d3` — engine policy modules → core

### R-015 基线

| 项 | 状态 |
|----|------|
| `scripts/runtime-longrun-baseline.ps1` | release 二进制、`.env`、`DEEPSEEK_RUNTIME_DIR` 隔离、turn **poll-until-idle** |
| 首份 RSS | **中位 ~26.6 MB** @ `ab4c3c4`、`deepseek-v4-pro`、3×50 turns — 见 `RUNTIME_BASELINE.md` |
| 未闭环 | HTTP p99（隔离 SQLite 多为 0）；≥1MB 工具 RSS 断言；>10% 回归门 |

**注意：** Windows **debug** `deepseek-tui serve --http` 曾 stack overflow；基线/长跑用 **release**。

### P2 PR3

- `deepseek-core::engine::{StartTurnParams, TurnEnginePort}`
- `RuntimeThreadManager::start_turn` → `EngineHandle` via **`TurnEnginePort`**（`turn_lifecycle.rs` 须 `use TurnEnginePort`）

### P2 PR4 — core 已迁入

**`crates/core/src/engine/`：**

| 模块 | 说明 |
|------|------|
| `loop_guard` | 重复工具调用防护 |
| `streaming` | 流解析、`ToolUseState`、重试常量 |
| `dispatch` | JSON 解析、并行/MCP/plan 策略 |
| `context` | 上下文预算、`compact_tool_result_for_context` |
| `approval` | `await_tool_approval` / `recv_user_input_for_tool` |
| `tool_bridge` / `tool_progress` | 工具 I/O 转换、审计/进度文案 |
| **`turn_loop/`** | **`handle_deepseek_turn<H: TurnLoopHost>`**（`run.rs` ~237 行）、`TurnLoopHost` trait、`exec`/`helpers`/`control` |

**`crates/core/src/` 共享类型（tui re-export）：** `events`、`error_taxonomy`、`coherence`、`user_input`、`subagent`

### P2 PR4 — tui L2 壳 / 接线

| 路径 | 说明 |
|------|------|
| `engine.rs` | **~201 行** — `Engine` struct、`spawn_engine`、子模块 import 层 |
| `engine/types.rs` | `EngineConfig` |
| `engine/handle.rs` | `EngineHandle` |
| `engine/engine_new.rs` | `Engine::new` |
| `engine/{engine_helpers,session_messages,mock}.rs` | 小 helper / 测试 double |
| `engine/{op_loop,cycle_hooks,message_handlers,context_recovery,tool_context,layered_context}.rs` | Op 循环、周期、消息、RLM 等 |
| `engine/turn_loop/{host_impl,streaming_phase,tool_phase}.rs` | **`impl TurnLoopHost for Engine`** + 两阶段 L2 |
| `engine/turn_port.rs` | `EngineHandle: TurnEnginePort` |
| `engine/dispatch.rs` | **`arg_repair`** 包装的 `parse_tool_input` / `final_tool_input`（勿绕过） |
| `engine/tool_catalog.rs` | `AppMode` 适配 + `code_execution` L2（策略在 core `tool_catalog`） |
| `engine/tool_dispatch_port.rs` | `RegistryToolDispatch` |
| `engine/tool_execution/` | exec / parallel / mcp / progress / terminal_guard / port（`McpPoolHandle`） |
| `engine/capacity_flow/{checkpoints,observation,events,interventions,replay,persistence}.rs` | 最大 ~344 行 — checkpoint / 干预 / replay / 持久化 |

**架构：**

```
deepseek-core::engine::handle_deepseek_turn<H: TurnLoopHost>
  ├─ host.run_streaming_phase()  → tui streaming_phase.rs
  └─ host.run_tool_execution_phase() → tui tool_phase.rs
```

### A4.6 — runtime_threads

| 文件 | 行数（约） | 内容 |
|------|-----------|------|
| `manager.rs` | ~589 | scratchpad/checklist panels、approval、recovery helpers（turn control → `turn_control.rs`） |
| `turn_control.rs` | ~254 | `interrupt_turn` / `steer_turn` / `compact_thread` |
| `thread_crud.rs` | ~650 | create/list/get/update/fork/resume/seed |
| `turn_lifecycle.rs` | ~215 | `start_turn` |
| `active.rs` / `monitor.rs` / `routing.rs` / `engine_load.rs` | — | 已拆 |

`RuntimeThreadManager` 部分字段已 `pub(crate)` 供子模块访问（拆模块时按需放宽）。

### 测试

- **`cargo test -p deepseek-tui --lib` → 2336 passed**（@ `3264419`）
- `runtime_api/tests.rs`：`spawn_test_server` 须显式 `data_dir`，勿依赖工作区 `DEEPSEEK_RUNTIME_DIR`
- `tool_kind_for_name` 必须 **非** `#[cfg(test)]` re-export

---

## 3. 仍未做（下一窗口优先级）

1. **PR5 剩余** — `core::Runtime::handle_thread(Message)` 委托真 turn（app-server）；DS Pick 多窗口手测

2. **§12.3** — `runtime_threads` 经 core 跑 turn；Engine 是否 L2 终态决议

3. **R-015 可选** — 1MB 工具 RSS；真实 store HTTP p99；回归门

### 已完成（2026-05-23 门控 + PR5 局部）

- ✅ G2：`event_schema_version`、A5.5 15 步回放、A+.7、见 [G2_GATE_ACCEPTANCE.md](./G2_GATE_ACCEPTANCE.md)
- ✅ PR5 局部：`parallel_turns_on_two_threads_*` + `sidecar_parallel_turns_on_two_threads`

---

## 4. 关键路径速查

```
docs/tech/adr/P2_DESKTOP_TURNLOOP_SPIKE.md
docs/tech/RUNTIME_EVOLUTION_ROADMAP.md
docs/tech/adr/RUNTIME_BASELINE.md
scripts/runtime-longrun-baseline.ps1

crates/core/src/engine/turn_loop/run.rs    # handle_deepseek_turn
crates/tui/src/core/engine.rs              # ~201 行壳
crates/tui/src/core/engine/turn_loop/      # L2 phases + host_impl
crates/tui/src/core/engine/capacity_flow/   # checkpoints / interventions / replay
crates/tui/src/runtime_threads/turn_control.rs
crates/tui/src/runtime_threads/manager.rs
crates/tui/src/runtime_threads/turn_lifecycle.rs
crates/tui/src/runtime_api/tests.rs
```

**规则：** `.cursor/rules/ds-pick-repo.mdc`、`code-organization.mdc`；变更记 `CHANGELOG.md`；**不要**未询问就 `git commit`。

---

## 5. 验证命令（新窗口第一件事）

```powershell
cd F:\DeepSeek-TUI-desktop

cargo build -p deepseek-core -p deepseek-tui

cargo test -p deepseek-core engine::
cargo test -p deepseek-tui --lib runtime_threads
cargo test -p deepseek-tui --lib

# 基线（需 .env API key；耗时长）
.\scripts\runtime-longrun-baseline.ps1
```

---

## 6. 已知坑

| 坑 | 处理 |
|----|------|
| 工作区 `DEEPSEEK_RUNTIME_DIR` | 污染 runtime 测试；测试显式 `data_dir` |
| `final_tool_input` | **必须**经 tui `dispatch`（`arg_repair`） |
| `EngineToolDispatch` | MCP / LocalShell **不走** adapter；仍 `McpPool` |
| `EngineHandle::start_turn` | 经 **`TurnEnginePort`** trait，非 inherent method |
| engine 子模块 `use super::*` | 父 `engine.rs` 须保留 import 层（勿删顶/底 `use`） |
| 嵌套 `capacity_flow/*` 子模块 `pub(super)` | 仅暴露到 `capacity_flow` 父模块；跨 sibling 须 `pub(in crate::core::engine)` |
| `Engine` 字段 privacy | 子模块是 engine 后代，private 字段可访问；跨 sibling 构造 `EngineHandle` 用 `pub(super)` 字段 |
| debug `serve` stack overflow | 基线用 **release** |
| 未跟踪会话 JSON | 勿提交 `deepseek-session-*.json`、`.env` |

---

## 7. 复制给新窗口 Agent 的提示（整段粘贴）

```markdown
继续 DS Pick monorepo 的 **P2 PR4 + A4.6 + R-015** 方案。

**必读：**
- `docs/tech/adr/P2_PR4_SESSION_HANDOFF.md`（本对接）
- `docs/tech/adr/P2_MIGRATION_SPIKE.md` §4、§4.1
- `docs/tech/RUNTIME_EVOLUTION_ROADMAP.md` §11–12
- `AGENTS.md`

**当前状态（@ `3264419`）：**
- ✅ `deepseek-core::engine::handle_deepseek_turn` + `TurnLoopHost`
- ✅ tui `turn_loop/{host_impl,streaming_phase,tool_phase}` L2 接线
- ✅ `engine.rs` **~201 行**（PR4 <300 达标）
- ✅ A4.6：`thread_crud` / `turn_lifecycle` / engine 子模块拆分
- ✅ A4.6：`turn_control.rs`；`manager.rs` ~589 行
- ✅ A4.6：`capacity_flow/{checkpoints,observation,events,interventions,replay,persistence}.rs`
- ✅ `deepseek-core::engine::tool_catalog`；tui 保留 `code_execution` / `AppMode`
- ⏳ `tool_execution` 深迁（MCP/终端仍 L2）

**请先做：**
```powershell
cd F:\DeepSeek-TUI-desktop
cargo test -p deepseek-core tool_catalog::
cargo test -p deepseek-tui --lib
```
下一刀：
- **优先 A：** `tool_execution` 端口化
- **优先 B：** A5.5 / A+.4 门控 fixture

**约束：** 最小 diff；`CHANGELOG.md` 记用户可见变更；用户未要求时不 commit；保留 canonicalize/路径安全；`final_tool_input` 保留 tui `arg_repair`。

完成后更新本文 §2–§3 与 `P2_MIGRATION_SPIKE.md` §4 勾选。
```

---

## 8. Git / 工作区说明

@ `3264419` 主变更已提交。工作区可能仍有未跟踪本地文件：

```
_turn_body.txt
deepseek-session-*.json
deepseek-thread-*.json
```

**勿提交** 会话 JSON、`.env` 密钥。新窗口先 `git status` / `git log -1`。

---

## 9. 会话记录

- 上一窗口 transcript：  
  `C:\Users\Administrator\.cursor\projects\f-DeepSeek-TUI-desktop\agent-transcripts\a08af5fd-bbff-4d80-aa12-c2b606513b94\a08af5fd-bbff-4d80-aa12-c2b606513b94.jsonl`

---

*维护：每完成 PR4/A4.6 切片后更新 §2–§3 与 `P2_MIGRATION_SPIKE.md`。*
