# HARNESS — Agent Harness 归并提案（Proposal）

**Status:** Proposed (2026-05-26)
**Owner:** 架构 owner（待签收）
**Source vision:** [`docs/Agent+Harness组合式编程方案.md`](../../Agent+Harness组合式编程方案.md) v1.2
**Related:** [`ARCHITECTURE_ASSESSMENT_2026-05-25.md`](./ARCHITECTURE_ASSESSMENT_2026-05-25.md) §1 / §5.1 / §7.1 · [`B2_INJECTION_ARBITRATION.md`](./B2_INJECTION_ARBITRATION.md) · [`D6_RUNTIME_SERVER.md`](./D6_RUNTIME_SERVER.md) · [`D7_PERSISTENCE_UNIFICATION.md`](./D7_PERSISTENCE_UNIFICATION.md) · [`BACKLOG_LANDLOCK_ENFORCE.md`](./BACKLOG_LANDLOCK_ENFORCE.md)

---

## 0. TL;DR（评审者先读这段）

| 问题 | 提案立场 |
|------|---------|
| `Agent+Harness组合式编程方案.md` v1.2 方向是否正确？ | **正确**（§1 问题定义、§5 黑板三身份、§7 Context Reset、§8 可视化三角） |
| 是否作为「新方案」立项？ | **不**。仓库已有 7 个分散但成熟的子系统覆盖方案 60–70% 职责；当作「Zagens v0.5+ Harness 主题归并路线」 |
| 是否在冻结期启动实施？ | **不**。Phase 0（本 ADR + 名词映射）是文档动作；Phase 1+ 搭车 D6/D7/D8/D13，**不**单独占主线 |
| §11–§12 数学基础是否实现？ | **绝大部分降级或删除**（见 §6）。代价/收益不成比例，且参数无运行数据可估 |
| 是否新增 Engine 字段 / 新持久化轨 / 新 `/v1/*` 路径？ | **全部 No**（合规 §7.1 红线） |

**核心动作（如本 ADR 被接受）：**
1. 文档锁定名词映射表（§3），止血"方案 vs 现状"的概念漂移
2. Phase 1–3 全部挂在 D6/D7/D8/D13 的 PR 链上，**不**新立主线
3. 进化引擎、四模型协同数学基础移入「v1.0 之后探索方向」清单

---

## 1. Context

### 1.1 上游方案

`docs/Agent+Harness组合式编程方案.md` v1.2（1670 行）提出十二章组合式 Harness 体系：黑板 / 任务图 / 决策分级（L1–L4）/ 行为监控 / Context Reset / 笔记 / 组合式 Harness / 进化引擎 + 数学基础（粗糙集 / 贝叶斯 / 可能性论 / SPRT / 指数衰减 / 多目标优化）。**该文档是远景设计稿，未与现有 crate 形态对齐。**

### 1.2 仓库已有承载

下表是方案各概念在仓库中的「事实承载」（探索结果，medium 深度）：

| 方案概念 | 已有承载（文件 + 大致 LOC） | 成熟度 |
|---------|----------------------------|--------|
| 任务图 / 子任务 | `tools/{plan, todo, tasks}.rs` + `task_manager.rs` (~1880) + `tools/subagent/mod.rs` + `cycle_manager.rs` `StructuredState` | 雏形：线性 checklist + 后台队列，**无 DAG** |
| 黑板 | `scratchpad/{mod,schema,coverage,summary,auditor}.rs` + `tools/subagent/blackboard.rs` + `runtime_api/blackboards.rs` (~40) | 雏形：**两套并行**（audit scratchpad + CRAFT JSON 黑板） |
| 决策日志 | `audit.rs` + `runtime_threads/persist.rs` events + `cycle_manager.rs` `<carry_forward>` | 雏形：**三处分散**，无统一 append-only view |
| 笔记 | `topic-memory` crate + `tui/topic_memory.rs` (~359) + `skills/` + `tools/remember.rs` + `~/.deepseek/memory.md` | 部分：**4 套并存**，无统一检索注入 |
| Context Reset | `compaction.rs` (~2772) + `seam_manager.rs` (~802) + `cycle_manager.rs` + `core::engine::hosts::seam::SeamHost` (10 方法) | **已成熟**：仓库最厚子系统之一 |
| 决策分级 | `execpolicy`（AskForApproval: UnlessTrusted / OnFailure / Always）+ `core::ApprovalMode`（Auto/Suggest/Never）+ `command_safety.rs` 风险 stakes | 部分：**有三档审批**，无显式 L1–L4 命名 |
| 行为监控 | `core/engine/cycle_hooks.rs` + `capacity_flow/{checkpoints,interventions}.rs` + `scratchpad_state.rs`（readonly 熔断）+ `coherence.rs`（用户体验阶梯） | 部分：**有 cycle/coherence 干预**，无"工具连续失败 N 次"专用计数 |
| 组合式 Harness | `execpolicy` + `sandbox` + `hooks` + `network_policy` + `command_safety` | 部分：**5 套独立 policy 子系统**，未按任务类型动态装配 |
| 可视化 | `web-ui` `AuditScratchpadPanel.tsx` + `TopicMemoryPanel.tsx` + `lib/craftBlackboard.ts` | 部分：UI 雏形齐全，等待 backend 归一 |
| 进化引擎 | — | **完全缺失** |

### 1.3 冻结纪律约束

参照 [`ARCHITECTURE_ASSESSMENT_2026-05-25.md`](./ARCHITECTURE_ASSESSMENT_2026-05-25.md)：

- §1 定型 checklist 仍 **7/10**；冻结窗口预计 10–14 周
- §5.1 主线已签收：**D6 → D9/D10 → D7 → D8 → D1 → P2**
- §7.1 红线（与本提案直接相关）：
  - ⛔ 禁止给 `deepseek_core::engine::Engine` 加新字段（M-series 35 字段封口）
  - ⛔ 禁止新增 `/v1/*` 端点，除非配套补 OpenAPI schema 草案
  - ⛔ 禁止 `desktop` crate `use deepseek_core` / `use deepseek_tui`

**任何"全量实施 v1.2 方案"的 PR 都会同时触碰这三条红线。** 因此本提案的核心是「**归并 + 搭车 + 砍数学**」三件事，不是新建。

---

## 2. Decision

**接受 `Agent+Harness组合式编程方案.md` 的问题定义与三角架构（Harness / 黑板 / 可视化），拒绝其 v1.2 实施路线。** 改为按本 ADR §3 名词映射 + §5 Phase 路线推进。

### 2.1 核心原则

1. **优先归并，禁止平行**：方案每个概念必须先在 §3 表里找到现有承载，归并失败才允许 0→1。
2. **零 Engine 字段新增**：所有新组件以 `deepseek_core::engine::hosts::*` trait + `EnginePlatformExt` 形式接入（与 M3–M5 的 8 个 host 同形态）。
3. **零新持久化轨**：黑板 / 任务图 / 决策日志 全部复用 D7 之后的统一 SQLite + JSONL，作为 **derived view** 而非新表。
4. **零新 HTTP 路径**（短期）：扩容 `/v1/blackboards` 与 `/v1/threads/{id}/...` 现有路径；新概念在 D8 落地后通过 OpenAPI 一次性曝光。
5. **数学基础整体降级**（§6）。

---

## 3. 名词映射表（SSOT，本 ADR 的灵魂）

**任何 PR / 文档 / UI 文案使用以下方案概念时，必须用「仓库现有承载」描述。** 不允许同时使用方案名词与现有名词指代同一物。

| 方案 v1.2 概念 | 仓库归并目标 | 归并形式 | 备注 |
|---------------|------------|---------|------|
| **黑板 — 区域1 任务目标** | `runtime_threads` thread metadata + `cycle_manager::StructuredState.objective` | derived view | 不可变；目标变更走 `Op::Steer` |
| **黑板 — 区域2 任务图** | `tools/plan` `SharedPlanState` ⊕ `tools/todo` ⊕ `tools/tasks` SQLite | derived DAG view | 三者输入合流，**plan/todo/tasks 是工具**，task graph 是只读视图 |
| **黑板 — 区域3 当前上下文** | `scratchpad_state` + `cycle_manager` 当前 cycle 状态 | derived | 频繁更新由 cycle/capacity 已有路径维护 |
| **黑板 — 区域4 决策日志** | `runtime_threads` events 过滤（`event_kind ∈ {approval, policy_decision, replan, cycle_advance, intervention}`）+ `audit.log` | derived view（**不新建表**） | append-only 由现有 broadcast 天然满足 |
| **黑板 — 区域5 发现与踩坑** | `scratchpad/schema.rs` `Inventory` + `NoteLine` + `tools/remember.rs` 输出 | 直接复用 | scratchpad 已是该形态 |
| **黑板 — 区域6 Harness 状态** | `execpolicy` + `sandbox` + `network_policy` + `hooks` 当前快照 | derived view（D13 Capability Manifest 后） | 不存，按需 compose |
| **任务图追踪器** | `cycle_manager` + `scratchpad_flow::record_tool_outcome` + 新增 `TaskGraphHost`（只读 derive） | trait host | 不写新存储 |
| **决策分级器** | `execpolicy::AskForApproval` 三档 + `core::ApprovalMode` + `command_safety` stakes | 直接对齐命名 | 见 §4 |
| **行为监控器** | `capacity_flow/{checkpoints,interventions}` + `cycle_hooks` + 新增 `BehaviorMonitorHost`（**仅"工具连续失败 N 次"计数**） | 复用 + 1 个新 host | 删除 SPRT |
| **强制续写** | `core/engine/turn_loop/host_impl/no_tool_uses.rs`（已存在！） | 直接复用 | 命名对齐 |
| **目标重注入** | `cycle_hooks::refresh_system_prompt` + `<carry_forward>` briefing | 已存在 | 按 cycle 边界触发，非按 token 数 |
| **Context Reset / 交接** | `compaction.rs` + `seam_manager.rs` + `cycle_manager.rs` 三套已有路径 | **直接复用** | 方案 §7 章降级为「使用规范」 |
| **笔记系统** | `topic-memory` crate（pheromone 图）+ `skills` + `remember` + `memory.md` | 直接复用 + 文档统一检索注入次序 | 与 B2.1 仲裁链对齐（[`B2_INJECTION_ARBITRATION.md`](./B2_INJECTION_ARBITRATION.md)） |
| **审查管线（安全/架构/性能/风格）** | `execpolicy` ⊕ `sandbox` ⊕ `hooks` ⊕ `network_policy` ⊕ `command_safety` | 合并入 D13 Capability Manifest | 不立新 crate |
| **组合式 Harness 装配器** | D13 Capability Manifest（backlog 已存） | 等 D13 启动 | 不在 Phase 0–3 内 |
| **进化引擎** | — | **延期 v1.0 之后** | 见 §7 |
| **UI 可视化面板** | `AuditScratchpadPanel` → 扩容为 `HarnessPanel`（集合：任务图 / 决策日志 tab / 黑板 / Harness 状态） | rename + 扩容 | 不新建顶层面板 |

### 3.1 假朋友（必须警惕的"同名不同义"）

| 仓库名词 | 方案名词中**不**对应 |
|---------|---------------------|
| `cycle` / `cycle_hooks` / `cycle_manager` | **不是** loop detection；是 **context refresh / 换脑** |
| `Seam L1/L2/L3`（密度层级） | **不是** L1–L4 决策分级 |
| `tui/tests/eval_harness.rs` | **不是** Agent Harness；是离线 eval |
| `TaskManager`（~1880 LOC） | **不是** 任务图；是后台 job 队列 |
| `CoherenceState` | **不是** "内部一致性"；是 UI 会话健康文案 |
| `record_tool_outcome` | **不是** 决策审计；是 scratchpad nudge 计数 |

---

## 4. 决策分级对齐（L1–L4 → 已有三档）

方案 §4.2 / §11 的 L1–L4 分级，**不**新建分级器。直接对齐到现有审批栈：

| 方案 Level | 含义 | 仓库对齐 | 配置位 |
|:----------:|------|---------|--------|
| **L1** | 实现细节，不记录 | 不进任何审批；**不记录**（方案要求"不记录"，与现状一致） | — |
| **L2** | 技术选择，自主 + 记录原因 | `AskForApproval::UnlessTrusted` + `ApprovalMode::Auto` + 事件流自然记录 | `execpolicy` 规则表 |
| **L3** | 架构决策，暂停 + 等待确认 | `AskForApproval::OnFailure` 或工具自报 `stakes = High` → 走 `await_tool_approval` | `command_safety` + 工具元数据 |
| **L4** | 业务规则，必须询问 | `AskForApproval::Always` + 强制人工 approve | `execpolicy` deny-list / 工具 `requires_approval=true` |

**收益：**
- 不写新代码即可声称"已支持 L1–L4 决策分级"
- 项目级/任务级 override 走已有 `execpolicy` 规则表（无需新机制）
- "可能性等级（极低/低/中/高/极高）"降级为 UI approval 提示的语义标签（在 `command_safety.rs` `Stakes` 上做映射）

---

## 5. Phase 路线（搭车 D-series 拐点，不新立主线）

### Phase 0 — 本 ADR + 名词锁定（NOW，1–2 周，**仅文档**）

| 交付 | 触发红线 | §1 影响 |
|------|---------|---------|
| 本 ADR 评审 + 签收 | 否 | 不变 |
| `docs/Agent+Harness组合式编程方案.md` 顶部加 banner，指向本 ADR 名词映射 | 否 | 不变 |
| `RUNTIME_ARCHITECTURE.md` §10 加入「Harness 主题」段落，引用本 ADR | 否 | 不变 |
| 把方案 §11 / §12 数学基础章节内容**搬出主文档**为附录 `APPENDIX_HARNESS_MATH.md`，标注"未来探索" | 否 | 不变 |

### Phase 1 — 借势 D7（持久化统一），4–6 周，**schema 设计为主**

| 交付 | 形式 | 风险 |
|------|------|------|
| 在 D7 PR 链（特别是 C2 文档 PR）追加 `harness_views.sql`：基于统一 SQLite 给出 task graph / decision log / blackboard 三个**只读视图**的 SQL 草案 | spike + SQL | 低 |
| audit scratchpad ↔ CRAFT blackboard schema 对齐：定义"unified blackboard v1 4 分区"JSON schema（目标 / 任务图 / 当前 / 笔记+踩坑），**不**改现有写入路径 | schema 文件 | 低 |
| 新增 `TaskGraphHost` / `BlackboardReadHost` trait（**只读** trait，0 字段写入 Engine）的 spike RFC | spike doc | 低 |
| 验收：D7 C6 勾选时，本提案 task graph / blackboard / decision log 三视图随之可用 | — | — |

### Phase 2 — 借势 D8（OpenAPI/TS 生成），3–4 周

| 交付 | 形式 | 风险 |
|------|------|------|
| `runtime_api/blackboards.rs`（当前 ~40 LOC）扩容为统一黑板 view：`GET /v1/threads/{id}/harness/blackboard` | 端点扩容（**非新增 `/v1/*` 一级路径**） | 中 |
| `GET /v1/threads/{id}/harness/decisions` — decision log derived view | 端点扩容 | 中 |
| `GET /v1/threads/{id}/harness/task-graph` | 端点扩容 | 中 |
| `GET /v1/threads/{id}/harness/state` — Harness 模块快照（execpolicy + sandbox + hooks compose 结果） | 端点扩容 | 中 |
| 全部经 D8 OpenAPI 自动生成 TS 类型，前端无手写 interface | 自动生成 | 低 |
| `AuditScratchpadPanel` rename → `HarnessPanel`，tabs 加 "任务图 / 决策 / 黑板 / 状态" | UI 重构 | 中 |

### Phase 3 — §1 = 10/10 之后（6–12 月），P2 阶段

| 交付 | 依赖 | 备注 |
|------|------|------|
| 行为监控器 `BehaviorMonitorHost`（仅"工具连续失败 N 次 → 触发 escalation"） | 无 | 简单计数器，配置阈值，无 SPRT |
| 组合式 Harness 装配器 | D13 Capability Manifest | 与 D13 同步设计 |
| 进化引擎（如果有数据） | 至少 6 月运行数据 + 评审 | 默认延期 |
| 数学基础四模型实现 | 评审通过 | 默认不做 |

---

## 6. 数学基础降级清单

方案 §11–§12 共 11 个数学模型。降级处置如下：

| 模型 | 原章节 | 处置 | 替代 |
|------|--------|------|------|
| 指数衰减 + 唤起修正 | §12.1 | **删除实现**，保留 mental model | 按 cycle/seam 阈值硬触发 |
| 注入频率最优化 T* ≈ √(2c/λ) | §12.2 | **删除** | 按 cycle 边界 + capacity_flow 阈值 |
| 粗糙集三区域分类器 | §11.1 | **降级为 TOML 规则表** | `execpolicy` 已是这种形态 |
| 贝叶斯决策（E[代价] 比较） | §11.2 | **删除** | 工具自报 stakes + 配置阈值 |
| 可能性理论 Π 等级 | §11.3 | **降级为 UI 文案标签** | `command_safety::Stakes` 上做枚举映射 |
| SPRT 卡住检测 | §11.4 | **降级为简单计数器** | `BehaviorMonitorHost` 内置 N=3/5/8 三档可配 |
| 黑板 token 分配（约束优化） | §12.3 | **降级为固定预算** | `B2_INJECTION_ARBITRATION` 已有优先级链 |
| 笔记多因子排序（Rel × Fresh × Freq） | §12.4 | **复用 topic-memory pheromone 图** | 已存在 |
| Context Reset 阈值 u*（最优停止） | §12.5 | **删除推导**，保留 70–80% 阈值说明 | `cycle_manager` 768K 阈值已生产验证 |
| 任务拆分信息熵约束 | §12.6 | **删除** | LLM 自拆 + 人审 |
| 进化引擎 FDR + 多臂老虎机 | §12.7 | **延期 v1.0+** | 无数据可估 |
| Harness 装配多目标优化 | §12.8 | **降级为优先级 if-else**（方案自己给了简化版） | D13 时再评 |

**降级理由（共同）：** 这些模型的所有参数（λ、A_crit、校准因子、阈值_A/_B、H_max/H_min、precision CI、FDR α）都需要**长期运行数据**才能估计。在数据采集 pipeline 不存在的情况下先实现框架，等于用想象常数跑半年——边际收益不会超过 TOML 配置表。

---

## 7. Non-goals（明确不做）

1. **不**实现进化引擎（v1.0 之前）
2. **不**实现四模型协同数学基础（粗糙集 + 贝叶斯 + 可能性论 + SPRT）的形式化推理引擎
3. **不**新建第四套持久化轨（黑板/任务图/决策日志 全部 derived view）
4. **不**给 `deepseek_core::engine::Engine` 加字段
5. **不**新建顶层 `/v1/*` 路径（仅在 `/v1/blackboards` 与 `/v1/threads/{id}/...` 下扩容）
6. **不**新建独立 `crates/harness/` crate（host trait 入 `deepseek_core::engine::hosts`，实现按子系统就近放）
7. **不**在 Phase 0–2 引入"任务图 DAG"概念（保持线性 plan/todo 即可，DAG 是 Phase 3 议题）
8. **不**做 desktop crate ↔ core 的直连（依然 HTTP/IPC）
9. **不**改 `B2_INJECTION_ARBITRATION` 已锁定的 5 行注入优先级链
10. **不**与 ratatui freeze 路径合流（方案的可视化仅对 Zagens web-ui）

---

## 8. Code anchors（评审检索用）

| 关注点 | 文件 |
|--------|------|
| audit scratchpad SSOT | [`crates/tui/src/scratchpad/{mod,schema,coverage,summary,auditor}.rs`](../../../crates/tui/src/scratchpad/) |
| CRAFT blackboard | [`crates/tui/src/tools/subagent/blackboard.rs`](../../../crates/tui/src/tools/subagent/blackboard.rs) · [`crates/tui/src/runtime_api/blackboards.rs`](../../../crates/tui/src/runtime_api/blackboards.rs) |
| plan / todo / tasks 工具 | [`crates/tui/src/tools/{plan,todo,tasks}.rs`](../../../crates/tui/src/tools/) |
| task manager（后台 job 队列） | [`crates/tui/src/task_manager.rs`](../../../crates/tui/src/task_manager.rs) |
| cycle / context refresh | [`crates/tui/src/cycle_manager.rs`](../../../crates/tui/src/cycle_manager.rs) · [`crates/tui/src/core/engine/cycle_hooks.rs`](../../../crates/tui/src/core/engine/cycle_hooks.rs) |
| Context Reset（compaction + seam） | [`crates/tui/src/compaction.rs`](../../../crates/tui/src/compaction.rs) · [`crates/tui/src/seam_manager.rs`](../../../crates/tui/src/seam_manager.rs) · [`crates/core/src/engine/hosts/seam.rs`](../../../crates/core/src/engine/hosts/seam.rs) |
| 决策审批栈 | [`crates/core/src/engine/{approval,op_loop}.rs`](../../../crates/core/src/engine/) · [`crates/tui/src/tui/approval.rs`](../../../crates/tui/src/tui/approval.rs) · [`crates/execpolicy/src/lib.rs`](../../../crates/execpolicy/src/lib.rs) · [`crates/tui/src/command_safety.rs`](../../../crates/tui/src/command_safety.rs) |
| audit log | [`crates/tui/src/audit.rs`](../../../crates/tui/src/audit.rs) |
| 行为干预 | [`crates/tui/src/core/engine/capacity_flow/{checkpoints,interventions}.rs`](../../../crates/tui/src/core/engine/capacity_flow/) · [`crates/core/src/engine/scratchpad_state.rs`](../../../crates/core/src/engine/scratchpad_state.rs) |
| 笔记 / 记忆 | [`crates/topic-memory/`](../../../crates/topic-memory/) · [`crates/tui/src/topic_memory.rs`](../../../crates/tui/src/topic_memory.rs) · [`crates/tui/src/skills/`](../../../crates/tui/src/skills/) · [`crates/tui/src/tools/remember.rs`](../../../crates/tui/src/tools/remember.rs) |
| Capability / Policy | [`crates/execpolicy/`](../../../crates/execpolicy/) · [`crates/tui/src/sandbox/`](../../../crates/tui/src/sandbox/) · [`crates/tui/src/network_policy.rs`](../../../crates/tui/src/network_policy.rs) · [`crates/hooks/`](../../../crates/hooks/) |
| UI 面板 | [`crates/desktop/web-ui/src/components/AuditScratchpadPanel.tsx`](../../../crates/desktop/web-ui/src/components/AuditScratchpadPanel.tsx) · `TopicMemoryPanel.tsx` · [`crates/desktop/web-ui/src/lib/craftBlackboard.ts`](../../../crates/desktop/web-ui/src/lib/craftBlackboard.ts) |

---

## 9. Acceptance

### Phase 0（本 ADR）
- [ ] 架构 owner 评审并签收本 ADR
- [ ] 上游方案文档加 banner 指向本 ADR
- [ ] `APPENDIX_HARNESS_MATH.md` 文档移交完成
- [ ] `ARCHITECTURE_ASSESSMENT_2026-05-25.md` §10 重评时点表加一行"Harness 提案签收"

### Phase 1（搭车 D7）
- [ ] `harness_views.sql` 草案进入 D7 C2/C5 之间
- [ ] unified blackboard v1 4 分区 JSON schema 合入 `docs/tech/schemas/`
- [ ] `TaskGraphHost` / `BlackboardReadHost` spike RFC 合入

### Phase 2（搭车 D8）
- [ ] `/v1/threads/{id}/harness/{blackboard,decisions,task-graph,state}` 四个端点经 OpenAPI 暴露
- [ ] `AuditScratchpadPanel` rename 为 `HarnessPanel` 并扩容 tabs
- [ ] 端到端：单线程任务可在 HarnessPanel 看见完整 4 视图

### Phase 3（§1 = 10/10 之后）
- [ ] 评审：进化引擎 vs 数学基础 vs 行为监控 N 计数器 三选一启动
- [ ] D13 Capability Manifest 接入"组合式 Harness 装配器"语义

---

## 10. Change control

- **任何 PR 引入方案 v1.2 概念词时**，必须在 PR 描述对照 §3 名词映射，否则维护者可要求重写。
- **任何"在 Engine struct 加字段以承载 Harness 状态"的 PR**，必须先升级本 ADR（评审同 M-series 等级）。
- **任何"实现方案 §11–§12 数学模型"的 PR**，必须先在本 ADR §6 表里把对应行从"删除/降级"改为"实现"，并附运行数据支撑。
- **任何方案文档（`Agent+Harness组合式编程方案.md`）的 v1.3+ 更新**，必须同步刷新本 ADR §3 映射表。
- 本 ADR 在 §1 = 10/10 之后再评估是否升级为 Accepted 主线 ADR 或归档为 historical proposal。

---

## 11. 评审检查清单（给架构 owner）

- [ ] §3 名词映射表是否完整覆盖方案 v1.2 全部一级概念？
- [ ] Phase 1–3 是否真的"零新主线"，全部搭车 D6/D7/D8/D13？
- [ ] §6 降级清单是否激进过度（删除/降级过多）？
- [ ] §7 Non-goals 是否需要再增列（例如"不引入新 supervisor 协议"）？
- [ ] 是否需要把本 ADR 中的"Harness Panel UI 设计"拆为单独的 desktop UX ADR？
- [ ] §1 第 9 项（OpenAPI/TS 自动生成）在 Phase 2 是硬依赖；若 D8 延期，Phase 2 是否同步延期？

---

> **本 ADR 不替代 `Agent+Harness组合式编程方案.md`，而是为其规定落地的物理边界与时序。** 方案是远景，本 ADR 是路线。
