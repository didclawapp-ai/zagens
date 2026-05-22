# P2 PR4 / R-015 / A4.6 — 新会话对接（2026-05-22）

> **用途：** 在新 Cursor 窗口继续本方案时，把本文 + 下方「复制给 Agent 的提示」一并贴上。  
> **权威路线图：** [RUNTIME_EVOLUTION_ROADMAP.md](../RUNTIME_EVOLUTION_ROADMAP.md) §11–12、§17  
> **迁移笔记：** [P2_MIGRATION_SPIKE.md](./P2_MIGRATION_SPIKE.md)  
> **基线 ADR：** [RUNTIME_BASELINE.md](./RUNTIME_BASELINE.md)

---

## 1. 我们在做什么

在 **不新建 `runtime` crate** 的前提下，把 **Engine 可复用逻辑** 迁入 `deepseek-core`，TUI/Desktop 保留 **L2 壳**（`RuntimeThreadManager`、`runtime_api`、`ToolRegistry` 实现）。并行做 **A4.6** 拆分 `manager.rs`，以及 **R-015** 长跑 RSS 基线。

**PR4 收官标准（spike）：** `crates/tui/src/core/engine.rs` **< 300 行**（仅 `spawn_engine` / `EngineHandle` / re-export）；`turn_loop` + `Engine` 主体在 core。

---

## 2. 已完成（可直接当事实用）

### R-015 基线

| 项 | 状态 |
|----|------|
| `scripts/runtime-longrun-baseline.ps1` | 已加固：release 二进制、`.env`、`DEEPSEEK_RUNTIME_DIR` 隔离、turn **poll-until-idle** |
| 首份 RSS | **中位 ~26.6 MB** @ `ab4c3c4` 附近、`deepseek-v4-pro`、3×50 turns — 见 `RUNTIME_BASELINE.md` |
| 未闭环 | HTTP p99 在隔离 SQLite 下多为 0；≥1MB 工具 RSS 断言；与历史基线 >10% 回归门 |

**注意：** Windows 上 **debug** `deepseek-tui serve --http` 曾 stack overflow；基线用 **release**。

### P2 PR3

- `deepseek-core::engine::{StartTurnParams, TurnEnginePort}`
- `RuntimeThreadManager::start_turn` → `EngineHandle::start_turn`（校验在 core）

### P2 PR4（局部，多轮会话）

**已在 `crates/core/src/engine/`：**

| 模块 | 说明 |
|------|------|
| `loop_guard` | 重复工具调用防护 + 单测 |
| `streaming` | 流解析、`ToolUseState`、重试常量 |
| `dispatch` | JSON 解析、并行/MCP/plan 策略、`ToolParallelPlanFlags` |
| `context` | 上下文预算、`compact_tool_result_for_context`、`summarize_text` |
| `tool_dispatch` | **`EngineToolDispatch`** trait |
| `start_turn` / `turn_port` | PR3 |

**TUI 薄壳 / 接线：**

- `crates/tui/src/core/engine/{loop_guard,streaming,context}.rs` → `pub use deepseek_core::...`
- `dispatch.rs` — 保留 **`ToolExecutionPlan`**、`ToolExecGuard`、**`arg_repair`** 包装的 `parse_tool_input` / `final_tool_input`
- `tool_dispatch_port.rs` — **`RegistryToolDispatch`**：`execute_tool_with_lock` 在无 progress/context 时走 trait；否则 `execute_full_with_context`
- `turn_port.rs` — `EngineHandle` impl `TurnEnginePort`

**粗行数（2026-05-22）：** `engine.rs` ~2177，`turn_loop.rs` ~2008 — **仍未达标 <300**。

### P2 PR2（早前）

- `session`、`working_set`、`project_context`、`ApprovalMode` 等 → core；tui re-export

### A4.6（局部）

| 文件 | 内容 |
|------|------|
| `runtime_threads/active.rs` | LRU、活跃 turn |
| `runtime_threads/monitor.rs` | `monitor_turn` |
| `runtime_threads/routing.rs` | 路由规则读写 |
| `runtime_threads/engine_load.rs` | **`ensure_engine_loaded`** |
| `manager.rs` | ~1670 行（自 ~2860 拆出） |

`RuntimeThreadManager` 上 **`config` / `task_manager` / `automations`** 已 `pub(crate)` 供 `engine_load` 使用。

### 测试 / 修复

- `tool_kind_for_name` 必须 **非** `#[cfg(test)]` re-export（否则 `monitor.rs` 编译失败）
- `runtime_api/tests.rs`：`spawn_test_server` **显式** `RuntimeThreadManagerConfig.data_dir`，勿依赖工作区 `DEEPSEEK_RUNTIME_DIR`
- 最近：**`cargo test -p deepseek-tui --lib` → 2342 passed**

### 文档

- `CHANGELOG.md` `[Unreleased]` 已有 PR4/A4.6/R-015 条目  
- `P2_MIGRATION_SPIKE.md` §4、§4.1 已更新  

---

## 3. 仍未做（下一窗口优先级）

1. **PR4 剩余（主战场）**  
   - 仍在 tui：`engine.rs`、`turn_loop.rs`、`tool_catalog`、`tool_execution`、`approval`、`capacity_flow`、`tool_setup`、`scratchpad_flow`、`lsp_hooks`  
   - `deepseek-core/Cargo.toml` 迁入完整 Engine 时需 **`tokio`** 等依赖  
   - 可选路径见 spike §4.1：**先 `approval` + 端口化 `tool_execution`**，或 **`turn_loop.rs` 整文件迁 core**（单 PR 最大）

2. **A4.6 可选** — 继续拆 `manager.rs`（turn CRUD、`create_thread` 等）

3. **R-015 可选** — 1MB 工具输出断言；真实 store 路径上的 HTTP p99；`deepseek-chat` vs `v4-pro` 对比说明

4. **门控** — A5.5 回放 fixture、A+.4 契约测（spike 仍 unchecked）

---

## 4. 关键路径速查

```
docs/tech/RUNTIME_EVOLUTION_ROADMAP.md
docs/tech/adr/P2_MIGRATION_SPIKE.md
docs/tech/adr/RUNTIME_BASELINE.md
scripts/runtime-longrun-baseline.ps1

crates/core/src/engine/          # 已迁入模块
crates/core/Cargo.toml           # 尚无 tokio（PR4 剩余要加）

crates/tui/src/core/engine.rs
crates/tui/src/core/engine/turn_loop.rs
crates/tui/src/core/engine/tool_dispatch_port.rs
crates/tui/src/runtime_threads/manager.rs
crates/tui/src/runtime_threads/engine_load.rs
crates/tui/src/runtime_api/tests.rs   # 隔离 data_dir 范例
```

**规则：** `.cursor/rules/ds-pick-repo.mdc`、`code-organization.mdc`；变更记 `CHANGELOG.md`；**不要**未询问就 `git commit`。

---

## 5. 验证命令（新窗口第一件事）

```powershell
cd F:\DeepSeek-TUI-desktop

# 编译
cargo build -p deepseek-core -p deepseek-tui

# 核心单测（快）
cargo test -p deepseek-core engine::
cargo test -p deepseek-tui --lib runtime_threads
cargo test -p deepseek-tui --lib tool_dispatch_port

# 全量 lib（~30s+）
cargo test -p deepseek-tui --lib

# 基线（需 .env API key；耗时长）
.\scripts\runtime-longrun-baseline.ps1
```

---

## 6. 已知坑

| 坑 | 处理 |
|----|------|
| 工作区设了 `DEEPSEEK_RUNTIME_DIR` | 污染 `runtime_api` 测试；测试必须显式 `data_dir` 或 unset |
| `final_tool_input` | **必须**经 tui `dispatch`（含 `arg_repair`），勿直接用 core 版做流式工具参数 |
| `EngineToolDispatch` | MCP / LocalShell **不走** adapter；仍 `McpPool` |
| debug `serve` stack overflow | 基线/长跑用 **release** |
| `manager.rs` 拆模块 | 新 `impl RuntimeThreadManager` 若访问私有字段 → `pub(crate)` 字段或留在 `manager.rs` |

---

## 7. 复制给新窗口 Agent 的提示（整段粘贴）

```markdown
继续 DS Pick monorepo 的 **P2 PR4 + A4.6 + R-015** 方案实施。

**必读：**
- `docs/tech/adr/P2_PR4_SESSION_HANDOFF.md`（本对接）
- `docs/tech/adr/P2_MIGRATION_SPIKE.md` §4、§4.1
- `docs/tech/RUNTIME_EVOLUTION_ROADMAP.md` §11–12
- `AGENTS.md` 构建/测试约定

**背景：** PR4 局部已完成 — `deepseek-core::engine` 含 `loop_guard`、`streaming`、`dispatch`、`context`、`EngineToolDispatch`；tui 薄 re-export + `RegistryToolDispatch` 接线；`Engine`/`turn_loop` 仍在 tui（~2k 行 each）。A4.6 已拆 `active`/`monitor`/`routing`/`engine_load`。R-015 基线脚本与首份 RSS 见 `RUNTIME_BASELINE.md`。

**请先做：** `cargo test -p deepseek-tui --lib` 确认绿，再按 spike §4.1 选一刀：
- 优先 A：`approval.rs` 迁 core + 端口化；或
- 优先 B：`turn_loop.rs` 迁 core（补 `core/Cargo.toml` tokio 等），`engine.rs` 削壳。

**约束：** 最小 diff；`CHANGELOG.md` 记用户可见变更；不擅自 commit；保留 canonicalize/路径安全；`final_tool_input` 保留 tui `arg_repair` 路径。

完成后更新 `P2_MIGRATION_SPIKE.md` §4 勾选与 handoff 本文 §3。
```

---

## 8. Git / 工作区说明

对接编写时仓库 **可能有未提交改动**（`crates/core/`、`runtime_threads/`、`web-ui/dist`、`CHANGELOG`、会话 JSON 等）。新窗口应先：

```powershell
git status
git diff --stat
```

勿把 `deepseek-session-*.json`、`.env` 密钥、未请求的 `dist` 批量提交进 PR。`web-ui/dist` 是否纳入变更由维护者策略决定。

---

## 9. 会话记录

- Agent transcript（完整工具记录）：  
  `C:\Users\Administrator\.cursor\projects\f-DeepSeek-TUI-desktop\agent-transcripts\74ca06b4-614c-419a-bbef-238d596319ec\74ca06b4-614c-419a-bbef-238d596319ec.jsonl`

---

*维护：每完成 PR4/A4.6 一切片后更新 §2–§3 与 `P2_MIGRATION_SPIKE.md`，避免下一窗口重复劳动。*
