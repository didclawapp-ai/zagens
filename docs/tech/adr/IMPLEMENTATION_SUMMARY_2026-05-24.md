# 运行时演进 — 实施总结（截至 2026-05-24）

> **SSOT 路线图：** [RUNTIME_EVOLUTION_ROADMAP.md](../RUNTIME_EVOLUTION_ROADMAP.md) v2.0-final + §17  
> **受众：** 维护者、新会话 Agent、桌面/TUI 开发  
> **说明：** 本文据 **仓库现状 + ADR/CHANGELOG**，不重复路线图全文；门控以 §1.6 链为准。

---

## 1. 一句话结论

**Phase 1 harness ✅ → A+ / P2 ✅ → D10 ✅ → 阶段 F §12.4 ✅ → B-L1 CRAFT ✅（2026-05-24 手测）→ 余项：B2 记忆地图 / GAP 表。**

§12.1 **阶段 A 全量完成线** 仍为 **🟡 部分**（A1/A2/A5 有 MVP 与单测，未按 §12.1 五项全勾）；这不阻塞已签收的 P2、D10 与桌面扩容。

**硬约束未变：** DS Pick = Tauri 壳 + `deepseek-tui` sidecar；Agent turn **不**进 WebView；**不**换 app-server 生产 binary。

---

## 2. 门控链实况（§1.6）

```mermaid
flowchart LR
  P1[Phase 1 harness] --> A[A §12.1 L1]
  A --> Ap[A+ §12.2 L2]
  Ap --> P2[P2 §12.3]
  P2 --> D10[D10 解除]
  D10 --> F[F §12.4 桌面 GAP]
  F --> B[B §12.5 CRAFT]
```

| 门控 | 路线图要求 | **实际状态** | 关键证据 |
|------|------------|--------------|----------|
| Phase 1 | DS Pick harness | **✅** | CHANGELOG Phase 1、多窗口计划 |
| **A §12.1** | L1 五项（长跑、可观测、错误、A4、A5） | **🟡 部分** | A4 ✅；A1 MVP/full 有代码与 R-015；A2/A3 有实现+测；**未**标全阶段 A 完成 |
| **A+ §12.2** | 契约 v1、CI 契约测、审批回归 | **✅ 自动化**；手测可复测 | [G2_GATE_ACCEPTANCE.md](./G2_GATE_ACCEPTANCE.md) |
| **P2 §12.3** | turn_loop 在 core；`engine.rs` <300；sidecar 不变 | **✅ L2 终态 + PR6** | [P2_G3_ENGINE_L2_SIGNOFF.md](./P2_G3_ENGINE_L2_SIGNOFF.md)；`engine.rs` ~192 行 |
| **D10** | P2 后解除桌面 freeze | **✅ 已签收** | [P2_D10_UNFREEZE_RECORD.md](./P2_D10_UNFREEZE_RECORD.md) §4 |
| **F §12.4** | F0–F2 + 行为抽样 | **✅ F0–F3 + §12.4 #2**（2026-05-24 手测签收） | [TUI_DS_PICK_GAP.md](../../desktop/TUI_DS_PICK_GAP.md) |
| **B §12.5** | CRAFT + 记忆地图 + GAP 表 | **🟡 部分** — **#1 B-L1 ✅**（2026-05-24）；#2 记忆地图、#3 GAP 未启动 | [G2_PR5_MANUAL_SMOKE_CHECKLIST.md](./G2_PR5_MANUAL_SMOKE_CHECKLIST.md) §10 |

---

## 3. 分阶段交付摘要

### 3.1 阶段 0 — 治理

| 项 | 状态 |
|----|------|
| 路线图 v2.0-final、§4.2 D4–D9 签收 | ✅ |
| app-server 冻结、壳运分离文档 | ✅ |
| D10 freeze 规则（§10.6） | ✅ 已解除（见 §4） |

### 3.2 阶段 A — 底层（L1）

| 子项 | 交付要点 | 状态 |
|------|----------|------|
| **A1** 容量/会话 | `context_partition` 热/冷；`large_output_router` + `artifact_refs`；分区 trim；`history_isomorphism`（user/assistant/thinking）；R-015 RSS ~29 MB + `-Gate` | **🟡 MVP+**，非 §12.1 全勾 |
| **A2** 可观测 | `TurnSummary`、`turn.completed`、tracing span | **🟡** 有实现；§12.1 #2 未正式签收 |
| **A3** 错误 | `error_taxonomy`、HTTP wire envelope、流式重试策略、TUI hint | **🟡** 核心已落地 |
| **A4** 模块化 | `runtime_api` 拆域，主文件 ~505 行 | **✅** |
| **A5** 可测 | `lib` target、mock LLM、回放 fixture 15 步 | **✅**（G2 硬门槛） |
| **A6** 沙箱诚实 | 能力矩阵文档、非 macOS 降级文案 | **✅** |

**债务：** A1.3 落盘路径未全量审计；live `ToolCell` 与 `session.messages` 全同构未做。（`manager.rs` 已拆至 ~572 行。）

### 3.3 阶段 A+ — 契约（L2）

| 项 | 状态 |
|----|------|
| `event_schema_version` + `/health` + SSE | ✅ |
| `sidecar_contract_full_lifecycle`（CI） | ✅ |
| A+.7 审批（含多窗口竞态修复） | ✅ 自动化；弹窗手测可复测 |
| A5.5 回放 15 步 | ✅ |

### 3.4 阶段 P2 — Engine→core（绞杀者）

| PR/切片 | 内容 | 状态 |
|---------|------|------|
| PR0 | Spike、[P2_MIGRATION_SPIKE.md](./P2_MIGRATION_SPIKE.md) | ✅ |
| PR1–2 | 类型、`session`、`working_set` 等入 core | ✅ 局部 |
| PR3 | `TurnEnginePort`、`start_turn` 委托 | ✅ |
| PR4 | `handle_deepseek_turn`、`TurnLoopHost`；`engine.rs` 薄壳；`capacity_flow` 拆子模块 | ✅ |
| PR5 | 双 thread 并行测；`ThreadMessageTurnPort` | ✅ 局部（生产 HTTP 仍 `RuntimeThreadManager`） |
| **PR6a–d** | `tool_parser`、`streaming_phase`、`tool_phase`、`capacity_policy`；tui `tool_plans_exec` + `host_impl/` | ✅ [P2_PR6_TURN_LOOP_L2_MIGRATION_PLAN.md](./P2_PR6_TURN_LOOP_L2_MIGRATION_PLAN.md) |
| G3 | L2 终态 ADR 签收 | ✅ 2026-05-23 |

**架构（当前）：**

```
deepseek-core::handle_deepseek_turn<H: TurnLoopHost>
  ├─ streaming_phase (core)
  ├─ capacity_policy (core)
  └─ tool_phase (core) → host.execute_tool_plans → tui tool_plans_exec + port.rs

deepseek-tui: Engine struct、MCP/LSP/SubAgent、runtime_api、tools/*、RuntimeThreadManager
```

**明确不做（已 ADR）：** 整包 `Engine` struct 进 core；工具实现整体搬迁；换 app-server sidecar。

### 3.5 阶段 F — 桌面融合（D10 后）

| 波次 | 状态 | 备注 |
|------|------|------|
| F0 路由 | ✅ | `route_intent` 全链路单测 |
| F1a Terminal | ✅ | xterm + 增量 progress |
| F1b Diff | ✅ | diff2html + 运行中预览 |
| F2 导出/资源管理器 | ✅ | `export_*_json`、`open_in_shell` |
| F3 a11y | **✅** | Skip link、landmarks、roving tablist、`ModelParamsDialog` dialog；**G2 §8 手测已签**（2026-05-24） |
| F4 内联编辑 | **⏸** | 依赖 L1「改历史」API |

GAP 表：MCP/用量/任务技能/子代理/路由多为 **◐**；定时自动化 UI **暂缓**。

### 3.6 阶段 B — 差异化

| 子轨 | 状态 | 证据 |
|------|------|------|
| **B-L1** CRAFT runtime | **✅ 手测签收** | 黑板 API、角色白名单、fix-loop 提示、`craft.*` SSE — [craft-implementation-issues.md](../../craft-implementation-issues.md) Issue 0–5 |
| **B-L3** AgentPanel CRAFT 卡片 | **✅** | `AgentPanel` + `/v1/blackboards`；G2 §10.7 |
| **B2** 记忆地图 | **❌ 未启动** | §12.5 #2 |
| **B3** TUI 体验 | **⏸** | 非阻塞 |

---

## 4. 治理签收索引

| 文档 | 含义 | 日期 |
|------|------|------|
| [P2_G3_ENGINE_L2_SIGNOFF.md](./P2_G3_ENGINE_L2_SIGNOFF.md) | P2 L2 终态 / §12.3 架构 | 2026-05-23 |
| [G2_GATE_ACCEPTANCE.md](./G2_GATE_ACCEPTANCE.md) | A+ 自动化门控 | 2026-05-23 |
| [P2_D10_UNFREEZE_RECORD.md](./P2_D10_UNFREEZE_RECORD.md) | D10 解除 freeze | Jason **2026-05-24** |
| [G2_PR5_MANUAL_SMOKE_CHECKLIST.md](./G2_PR5_MANUAL_SMOKE_CHECKLIST.md) | 手测勾选（含 **§10 B-L1 CRAFT**） | F + B-L1 已签 **2026-05-24** |

---

## 5. 建议后续优先序（与 §17.3 对齐，2026-05-24 代码审计）

1. **B2 记忆地图** — §12.5 #2（greenfield，见 `docs/topic-memory-rust-plan.md`）  
2. **CRAFT 集成手测** — 自动化单测已加（`instructions_paths` / resident 硬锁 / LSP）；手测清单 [G2 §11](./G2_PR5_MANUAL_SMOKE_CHECKLIST.md) 待签  
3. **B3 TUI 拆分** — `main.rs` CLI 模块化（不阻塞门控）  
4. **R-015** — 全量 `-Gate` 重跑（可选）  
5. **远期** — `Engine` 整 struct 入 core；`StateStore` vs JSONL；`streaming_phase` 拆子模块；B3 `main.rs` CLI 拆分

---

## 6. 回归命令（新会话首选）

```powershell
cd F:\DeepSeek-TUI-desktop
cargo test -p deepseek-core --lib capacity_policy
cargo test -p deepseek-tui --lib history_isomorphism
cargo test -p deepseek-tui config::tests::instructions_paths --lib
cargo test -p deepseek-tui tools::subagent::tests::resident_file --lib
cargo test -p deepseek-tui core::engine::tests::build_tool_context_wires_lsp --lib
cargo test -p deepseek-tui --lib capacity_escalation
cargo test -p deepseek-tui --test protocol_recovery
cargo test -p deepseek-tui --lib sidecar_contract_full_lifecycle
cd crates/desktop/web-ui && npm run test:f3 && npm run build
```

---

## 7. 与 CHANGELOG 关系

未发布变更见根目录 **[CHANGELOG.md](../../CHANGELOG.md) `[Unreleased]`** — 本总结不替代逐条 changelog，仅作阶段归档视图。

---

*维护：重大门控（G2/G3/D10）或 §12.x 状态变化时更新本文 §2–§5。*
