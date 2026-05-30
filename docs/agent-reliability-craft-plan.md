# Agent 可靠性演进：CRAFT 向初步方案

> **状态：** 草案（根据产品讨论整理，便于后续迭代为实施清单）。  
> **范围：** DeepSeek-TUI / Zagens 共用 runtime；不替代现有 [子智能体文档](SUBAGENTS.md)，在其上增量演进。

## 命名备忘：CRAFT 缩写（SSOT）

拼写固定为 **CRAFT**（非 CARFT）。五个字母概括 **机制**，不是 Explorer / Implementer / Reviewer / Verifier 四个角色名的首字母缩写。路线图 **§9.1 B1** 见 [RUNTIME_EVOLUTION_ROADMAP.md](tech/RUNTIME_EVOLUTION_ROADMAP.md)。

| 字母 | 含义 | 对应 B1 步骤 |
|------|------|----------------|
| **C** | **Closed-loop**（闭环） | B1.4 fix-loop、`turn_loop` |
| **R** | **Review** | B1.2 `verdict` / `blockers`、Reviewer |
| **A** | **Agent**（多角色 `agent_spawn`） | B1.1 角色白名单、B1.3 spawn |
| **F** | **Fix-loop** | B1.4 程序化修复轮 |
| **T** | **Traceable**（黑板 + `task_id`） | B1.2–B1.3 黑板 |

**边界：** 全库审计子代理 **Auditor** + scratchpad 见 [audit-scratchpad-design.md](desktop/audit-scratchpad-design.md)，可与 CRAFT 黑板互补，但 **Auditor 不是上表中的 A**。长程代码任务（生成 / 修复 / 重构防早停）见 [harness/LONG_HORIZON_CODE_TASKS.md](harness/LONG_HORIZON_CODE_TASKS.md)（**LHT**），与 CRAFT 审查闭环分轨。

## 1. 目标与非目标

### 1.1 目标

把「单次生成看起来像对」转变为**可追溯、可验证、可多轮收口**的产出，系统化降低两类问题：

1. **脱离实际的幻觉**：API/数据流臆测、未对照仓库事实的补丁。  
2. **流程断层**：探索发现未传到实现者；审查发现问题无自动修复环；测试失败上下文未结构化交给修复方。

### 1.2 非目标（本草案不承诺一次到位）

- 完整复刻任一论文/商业产品的多智能体品牌实现。  
- 替代人类对需求与风险的最终签字。  
- 保证「确定性」字面意义（神经网络仍不完美）；此处 **确定性指：门禁脚本与协议带来的可重复验收路径**。

## 2. 背景：为何单靠大上下文不够

DeepSeek V4 等模型的长上下文缓解「装不下」，但不自动保证「会精读整张图」。易出现：

- **行动偏好**：先看片段就开写，未建立读写链。  
- **信息衰减**：上下文再长，仍可能弱化中间关键约束。  

工程对策不靠「再加一句别胡编」，而靠：**工具接地、门禁验收、结构化共享状态、角色边界**。

## 3. 与 Zagens / 当前架构的关系

- **桌面（Zagens）**：Web UI → HTTP/SSE → 与 CLI 同源 `serve --http` runtime（见 [docs/desktop/README.md](desktop/README.md)）。  
- **主 agent + 子 agent**：[`agent_spawn` 等编排](SUBAGENTS.md)，已有 `explore` / `implementer` / `review` / `verifier` / `custom` 等 **角色语调（role taxonomy）**。  
- **CRAFT 式演进**：在上述 **同一套派发能力**上，增加 **结构化黑板、程序化闭环、硬性工具裁剪、可执行规范**，而非推翻重做。

### 3.1 与「记忆地图」（topic graph）的配合

CRAFT 解决的是 **单次任务可追溯与多角色交接**（黑板、门禁、闭环）。与本路线中 **下一阶段** 的「记忆地图」互补：后者从对话中维护 **topic 认知图**（衰减、关联、可生成注入用 Markdown），独立实现见开源库 [**topic-memory-graph**](https://github.com/didclawapp-ai/topic-memory-graph)。

**已定序：** 先持续推进 CRAFT 落地，再将记忆地图 **系统化接入**（避免两条线同时争抢 prompt 空间却无合并策略）。并排开发时可约定：黑板写 **任务态事实与裁决**，记忆地图写 **跨轮话题骨架**；二者注入上下文时需明确 **先后顺序、字数上限与脱敏**。路线图、集成注意与 **记忆地图中长期潜力备忘**见 [docs/tui/UNDERLYING_ITERATION_REFERENCE.md §2.2–§2.3](tui/UNDERLYING_ITERATION_REFERENCE.md)。

### 3.2 并行工具调度与子代理写路径（现状核对）

> **核对日期：** 2026-05-17。对照 Zagens / 主 agent 在会话中的自述与当前 `crates/tui` runtime 实现。用于纠正 prompt 中的过时说法，并指导 §11 / P4 的优先级。

#### 3.2.1 结论摘要

| 主题 | 模型常见说法 | 代码事实 |
|------|--------------|----------|
| 主代理同轮并行 `edit_file` | 「现在就做 / 习惯问题」 | **❌ 不支持**：调度器要求整批 `read_only && supports_parallel`（见 `dispatch.rs` `should_parallelize_tool_batch`）；`edit_file` 为 `WritesFiles` 且 `supports_parallel` 默认为 false |
| 主代理同轮并行只读工具 | 较少强调 | **✅ 支持**：`read_file`、`grep_files`、`list_dir` 等同批可并行 |
| 多文件一次写入 | `apply_patch` | **✅ 单工具内多文件**，仍属写工具，**不能**与同批其它写操作并行 |
| 子代理能否写盘 | 「理论上可以、实际受限」 | **分角色**：`explore` / `review` 在 `build_allowed_tools` 中被硬裁剪（无 `edit_file`）；`implementer` 等默认 **继承父代理完整工具面**（v0.6.6+，`allowed_tools: None`） |
| 子代理能否跑编译/测试 | 「❌ 不能编译」 | **⚠️ 过严**：若 `allow_shell` 且未裁剪，可通过 `exec_shell` / `run_tests` 等执行；非引擎级禁止 |
| 子代理 LSP | 「写完看不到诊断」 | **⚠️ 半对**：无 engine 级 `run_post_edit_lsp_hook` + `flush_pending_lsp_diagnostics`；子代理 `ToolContext` 未接 `lsp_manager`，故 `edit_file` 内嵌的 `lsp_diagnostics_for_paths` 通常为空；可 **主动** 调用 `diagnostics` 工具 |
| 并行 spawn 多子代理写同一仓库 | 后写覆盖、无 merge | **✅ 成立**；且子代理 **一步内** 多个 tool call 亦为 **串行** for 循环（`subagent/mod.rs`） |
| 文件所有权 | 需要 `claim_file` | **仅有软约束**：`resident_file` + `RESIDENT_LEASES` 在 spawn 时 **warning**，不阻止并发写 |
| 补丁队列 / 冲突检测 | 需要 `apply_patches` 原子合并 | **未实现**：`apply_patch` / `edit_file` 均直接落盘 |

**一句话：** 并行子代理 **写代码** 的主要缺口在 **工具链与调度**（锁、LSP 回路、集成回合），不是「再加一个并行开关」；主代理 **也不能** 靠同轮多个 `edit_file` 绕过调度器。

#### 3.2.2 主代理：工具批调度

入口：`crates/tui/src/core/engine/dispatch.rs` — `should_parallelize_tool_batch` 要求计划中 **每一项** 同时满足：

- `read_only`
- `supports_parallel`
- 非 `approval_required`、非 interactive

`edit_file` / `write_file` / `apply_patch` 标记为 `WritesFiles`，`supports_parallel` 默认 **false**（`tools/spec.rs`；`file.rs` 测试断言 write 不可并行）。因此 **同轮多个写工具会串行执行**。

**可行模式（今日）：**

| 模式 | 说明 |
|------|------|
| 同轮并行只读 | 一次请求发多个 `read_file` / `grep_files` 等 |
| 单次 `apply_patch` | `changes` 数组一次改多文件（仍为一个写工具调用） |
| 多轮串行写 | 每轮一个或少量写工具，由主 agent 根据上轮结果决策 |
| 并行 `agent_spawn` | 多个子代理 **同时** 跑（受 `max_concurrent` 限制），适合 **只读** 探索；并行 **写** 仍有踩脚风险 |

#### 3.2.3 子代理：工具面、执行与 LSP

**工具继承（v0.6.6+）：** `SubAgentToolRegistry` 默认 `allowed_tools: None` → 与父 Agent 模式相同的完整 registry（`subagent/mod.rs` `build_allowed_tools`）。例外：

- `SubAgentType::Explore` / `Review` → 显式窄列表（无 `edit_file` / `write_file`），对应 CRAFT P3「硬裁剪」已部分落地。
- `Custom` → 必须显式 `allowed_tools` 非空。

**执行模型：** 子代理自有 turn 循环（`run_subagent_task`），**不** 走主 engine 的 `turn_loop` 并行派发；一步内多个 tool call **顺序** `execute`，连只读也不会在子代理内部并行。

**LSP 双路径（主 vs 子）：**

| 路径 | 主 agent | 子 agent |
|------|----------|----------|
| Tool 结果内嵌 | `lsp_diagnostics_for_paths(context, …)` — 仅当 `context.lsp_manager` 有值 | 子代理 `build_tool_context()` **未** `with_lsp_manager`，通常为空 |
| Engine post-edit | `run_post_edit_lsp_hook` → `pending_lsp_blocks` → 下一轮前 `flush_pending_lsp_diagnostics`（`lsp_hooks.rs` / `turn_loop.rs`） | **无** |
| 主动查询 | 可调 `diagnostics` 工具 | 同上（若工具面未裁剪） |

主 agent 的 LSP 实际主要依赖 **engine 路径**（`Engine` 持有 `lsp_manager`，与 `ToolContext` 分离）。子代理若要「编辑后自动见诊断」，需在子代理 `ToolContext` 接线 `lsp_manager`，并在子代理 turn 末复用 flush 逻辑（或等价地在 tool 结果中保证 diag 块非空）。

#### 3.2.4 并行写与所有权（缺口 ↔ CRAFT）

与 §5.3.2 黑板「顺序写入」互补：**黑板解决角色间事实传递**；**文件级所有权**解决多子代理 / 多写者同时改盘。

| 能力 | 现状 | 建议优先级 |
|------|------|------------|
| 硬文件锁 / `claim_file` | `resident_file` lease 仅 warning | **P1**（spawn 拒绝或排队） |
| 子代理 LSP 回路 | 未接 `lsp_manager` | **P0**（低成本，见 §11.4） |
| 子代理只产出 diff、主代理单点 apply | 无 | **P2**（集成回合 + `cargo check`） |
| `git worktree` + Integrator | 未做 | **P4 RFC**（§11.1 第 5 项） |

#### 3.2.5 方案对照（纠正模型自述）

| 方案 | 可行性（当前代码） | 适合场景 |
|------|-------------------|----------|
| 主代理同轮并行 `edit_file` | ❌ 调度器不允许 | — |
| 主代理同轮并行只读 + 单轮 `apply_patch` 多文件 | ✅ | 互不依赖的多文件小改 |
| 子代理只读（`explore`）+ 主代理写 | ✅ 成熟 | 大面积调查后统一修改 |
| 子代理 `implementer` 串行写 + 主代理集成 | ⚠️ 可用但无锁、无自动 LSP | 中等规模；需主 agent 编排与验证 |
| 多子代理并行写同一 repo | ⚠️ 高风险 | 仅当文件分区明确且接受覆盖风险；待 P1/P2 工具链 |

#### 3.2.6 与 CRAFT 阶段的关系

- **P3 工具硬裁剪**：`explore` / `review` 无写路径 — **已在 runtime 落地**（`build_allowed_tools`）；其它类型仍全继承，需靠 spawn 时 `allowed_tools` 或 prompt 约束。
- **P4 环境隔离**：`git worktree` / Integrator — 对应并行写的 **环境沙箱** 层（§5.2），与 §3.2.4 表一致。
- **勿误导模型**：系统提示中应避免「同轮并行多个 `edit_file`」；应引导 **只读并行**、**单次 patch**、或 **spawn 分工 + 主代理集成**。

## 4. 「精度天花板」归纳（编程场景）

| 断点 | 表现 | CRAFT 向对策（概念） |
|------|------|----------------------|
| 软约束权限 | Prompt 宣称只读仍可能越界写 | **按角色的工具允许表**（运行时裁剪；`custom` 角色是基础） |
| 无横向传递 | Explorer 的发现依赖主 agent 复述 | **共享黑板**：结构化写入，.spawn 时注入实现者上下文 |
| 单次反馈 | Review 完不自动拉回修复 | **Review→Revise 协议**（程序化或固定模板触发下一轮） |
| 测试与修复脱节 | Verifier 只报 fail | **Verifier 结构化产物**：日志摘要、疑似根因字段写入黑板 |
| 规范不可执行 | 「要 PEP8」仅自然语言 | **可执行规范**：契约/脚本必须通过才算「提交」 |

## 5. CRAFT 式架构要点（对齐讨论稿）

以下为 **目标蓝图**；第六节按阶段拆解为可落地 increments。

### 5.1 角色（可与现有 role 对齐）

| 角色 | 职责提要 | 与现有子 agent |
|------|----------|----------------|
| Specifier（可选） | 需求 → 契约/检查项（OpenAPI 片段、YAML、验收脚本入口） | 可由 `plan` + 产物约定扩展，或独立 prompt |
| Explorer | 只读探索、影响面、风险 | 对应 `explore` |
| Implementer | 按契约与黑板约束最小修改 | 对应 `implementer` |
| Reviewer | 对照契约与清单，**可拦 BLOCKER** | 对应 `review` |
| Verifier | 跑测试/诊断，输出可消费结构（两层：事实 + 推测） | 对应 `verifier` |
| Integrator（后期） | 受控合并、文档/Changelog | 当前主要由人 + Git；可作明确阶段 gate |

**Verifier 输出分层**（`verifier` 角色上下文有限——只有日志与 diff，没有完整代码上下文，根因假设可能不准确）：

| 层次 | 字段 | 含义 | 示例 |
|------|------|------|------|
| **observed** | `failures[].observed` | 测试日志摘录 + 失败断言的实际值/预期值（**事实，不推测**） | `"test_login line 85: expected token.uid == 42, got token.uid == None"` |
| **hypothesis** | `failures[].hypothesis` | 基于日志关键词的推测（**明确标注为推测，附带置信度标量**） | `{"guess": "auth/login.rs:42 未设置 user_id", "confidence": 0.4}` |

Implementer 读 `hypothesis` 但不盲从——仍需自己读源代码确认。`observed` 字段作为确定性输入直接指导调试。

> **置信度评分方法论（待验证）**：P0 阶段使用 `confidence_method: "llm_self_report"`——由 LLM 自评置信度。LLM 对自身不确定性缺乏元认知，此数值仅供辅助参考。P1 后可根据实际数据引入启发式评分（如"基于栈跟踪定位=0.5，基于代码搜索找到具体行=0.7"）。

### 5.2 三层隔离（成熟度分阶段）

| 层级 | 机制 | Phase 1 | 远期 |
|------|------|---------|------|
| 工具沙箱 | 角色级 `allowed_tools` | 强化 `explore`/`review` 无写路径；`custom` 白名单 | 与 registry 深度绑定，文档化审计 |
| 环境沙箱 | 独立工作区副本 | 可选：同 repo 内谨慎开发 | `git worktree` / 副本目录 + Integrator 合并 |
| 数据沙箱 | 黑板/密钥可见性 | 黑板键前缀 + 不注入密钥给 explore | 显式密级与 redact |

### 5.3 共享黑板（Blackboard）

- **MVP**：任务目录下单一 JSON 文件，带 `schema_version`、`task_id`、按角色分区的键。  
- **写入约定**：Explorer 写 `findings[]`，Verifier 写 `failures[]` + `hypothesis`，Reviewer 写 `verdict` + `blockers[]`。  
- **消费**：`agent_spawn` Implementer 时，runtime 或编排层将相关片段 **注入 system 增补或首条 user**，减少「主 agent 转述丢失」。

#### 5.3.1 黑板 JSON Schema（草案）

```jsonc
// 文件路径：.deepseek/blackboards/{task_id}.json
{
  "schema_version": 1,
  "task_id": "task-20260513-001",
  "created_at": "2026-05-13T10:00:00Z",
  "updated_at": "2026-05-13T10:15:00Z",
  "explorer": {
    "findings": [
      {
        "file": "crates/tui/src/auth/login.rs",
        "concern": "token 生成使用标准库 RNG，未引入 CSPRNG",
        "severity": "high",
        "suggestion": "改用 rand::rngs::OsRng"
      }
    ],
    "impact_summary": "仅影响 auth 模块；下游 consumer 在 commands/login.rs"
  },
  "implementer": {
    "rounds": [
      {
        "version": 1,
        "status": "done",
        "changes": [
          {"file": "crates/tui/src/auth/login.rs", "intent": "替换 RNG 为 OsRng", "diff_range": { "start": 40, "end": 55 }}
        ],
        "notes": "同步更新了 Cargo.toml 移除旧 rand 依赖"
      }
    ],
    "current_round": 1
  },
  "reviewer": {
    "rounds": [
      {
        "version": 1,
        "verdict": "BLOCKER",
        "reviewed_round": 1,
        "blockers": [
          {
            "id": "B1",
            "file": "crates/tui/src/auth/login.rs",
            "line": 42,
            "description": "OsRng 未 seed，在 WASM 目标不可用",
            "rule": "CROSS_PLATFORM",
            "severity": "BLOCKER"
          }
        ],
        "passes": []
      }
    ],
    "current_round": 1
  },
  "verifier": {
    "failures": [
      {
        "test": "test_login_success",
        "exit_code": 1,
        "observed": "line 85: expected token.uid == 42, got None",
        "hypothesis": {
          "guess": "第 42 行忘记给 token 设置 user_id 字段",
          "confidence": 0.4,
          "confidence_method": "llm_self_report"
        }
      }
    ],
    "summary": "1/5 测试失败",
    "diagnostics_hint": "cargo test -p auth -- --nocapture 2>&1 | tail -40"
  }
}
```

**设计要点**：

- **`diff_range` 为结构化对象而非字符串**：`{ "start": 40, "end": 55 }` 取代 `"L40-L55"`。Reviewer 可直接用数值构造 `read_file(path, start_line=40, limit=16)`，无需字符串解析。
- **`rounds[]` 保留多轮历史**：Implementer 和 Reviewer 的分区改用 `rounds[]` 数组而非直接覆写。当闭环触发第二轮 Implementer 时，Reviewer 可对比 `rounds[0]` 和 `rounds[1]` 的 diff（模拟人类 code review 的"审查修改后的版本"模式）。未增加并发复杂度（角色仍是串行写），仅改了存储结构。
- **`reviewed_round` 字段**：Reviewer 的每一轮审查明确标注审查的是 Implementer 的哪个 round 版本，避免多轮闭环后审查目标混乱。
- **`confidence_method: "llm_self_report"`**：标注置信度来源，便于后续替换为启发式评分时做对比。

#### 5.3.2 黑板并发策略

多子 Agent 不能同时写同一文件。朴素策略（MVP）：

| 规则 | 说明 |
|------|------|
| **一任务一黑板** | 文件路径包含 `task_id`，不同任务天然隔离 |
| **顺序写入** | 角色严格串行：Explorer → Implementer → Reviewer → Verifier → (循环)。不存在两个角色同时写黑板的场景 |
| **角色追加** | 每个角色覆盖自己的分区键（如 `explorer` 对象整体覆写），不触碰其他角色的分区 |
| **读快照** | 读操作只在 **spawn 时刻做一次**：runtime 在 spawn 子 Agent 前读取全量黑板，注入其上下文。运行期间不重读磁盘，避免每步 I/O 延迟 |

选择"spawn 时刻读快照"而非"每步实时读"的权衡：Implementer 看不到 Explorer 中途的增量发现，但在角色串行架构下 Explorer 已完成才会 spawn Implementer，因此信息天然完整。

#### 5.3.3 黑板生命周期

| 阶段 | 行为 |
|------|------|
| **创建** | 任务发起时（主 Agent 调用 `task_create` 或首次 `agent_spawn`），runtime 在 `.deepseek/blackboards/{task_id}.json` 创建初始骨架 |
| **更新** | 每个角色完成时，runtime 将该角色的结构化输出写入对应分区 |
| **读取** | spawn 子 Agent 时注入黑板内容到 system/user 上下文 |
| **归档** | 任务标记为 `COMPLETED` 或 `ABANDONED` 后，黑板文件保留在 `.deepseek/blackboards/` 下（低存储开销 JSON，便于事后审计）；可选 `--ttl` 自动清理 |
| **升级** | 闭环超过 N 次（默认 3）时，黑板全量快照随升级事件上报主会话 |

### 5.4 内置闭环（Fix-Loop）

#### 5.4.1 闭环协议执行位置（三选项分析）

关键决策：Review→Revise / Test→Fix 的循环逻辑写在哪一层？

| 选项 | 位置 | 做法 | 好处 | 坏处 |
|------|------|------|------|------|
| **1** | 子 Agent 内部 | Reviewer 发 verdict 信号后保持 Running，等待 Implementer 在同一会话中回复 | 闭环透明，无需外部编排 | 子 Agent 必须保持 Running 等待，占用并发槽位；子 Agent 会话上下文膨胀 |
| **2（P2 选用）** | 主 Agent 轮次间 | 子 Agent 返回结构化数据 → 主 Agent 读取 `verdict` / `failures[]` 字段 → 生成新的 `agent_spawn`（不改 Rust 代码，改主 Agent 系统提示词） | 实现最简单，零架构改动；主 Agent 转述的是结构化 JSON（损失远小于自然语言）；消费端逻辑在主 Agent prompt 中 | 主 Agent 仍是中介，但成本可接受；需确保主 Agent 理解优先从 `structured_verdict` 字段而非自然语言文本做决策 |
| **3（远期）** | Runtime spawn 后钩子 | `agent_spawn` 返回时，后钩子检查 verdict → 满足条件则自动再 spawn | 真正的自动化 | 需要引入 spawn 后钩子系统，架构改动大；需定义钩子失败/超时策略 |

**决策**：P2 选用**选项 2**（主 Agent 读结构化数据做分支）。P2 的消费端逻辑全部落在**主 Agent 系统提示词**中，无需修改 Rust runtime。主 Agent 的 prompt 增加一节：

```
When a sub-agent result carries a structured_verdict:
- If verdict == "BLOCKER", spawn a new Implementer with the blocker items as fix context. Do not ask the user.
- If verdict == "MAJOR", include the items in the next Implementer spawn but allow the fix loop to proceed.
- If verdict == "PASS", continue to the next role.
```

代价最小且能验证闭环逻辑是否正确。后续根据真实使用反馈再评估选项 3。

#### 5.4.2 闭环协议细节

1. **Review → Revise**：`verdict` 含 `BLOCKER` 时，**不结束任务**；主 Agent 解析结构化 `blockers[]` 字段 → 生成新的 Implementer 子任务（`agent_spawn` 携带 `blockers[]` 作为修复目标）。优先**程序解析 verdict**（读取 JSON 字段），其次用强约束模板要求 Reviewer 在特定 Markdown 围栏内输出。

2. **Test → Diagnose → Fix**：Verifier 失败 → 黑板写入 `failures[]`（含 `observed` + `hypothesis`）→ 主 Agent 生成 Implementer 子任务，注入诊断块 → Implementer 修复 → 重跑测试链。**失败 N 次（默认 3）升级主会话**，附带黑板全量快照 + 最后 N 轮的 diff 摘要，要求人工介入或主 Agent 重新评估方案。

3. **升级协议**：N 次闭环后仍未通过时，主 Agent 收到升级事件（含黑板 JSON + 失败摘要）。主 Agent 可选择：(a) 调整策略后重新派发，(b) 缩小任务范围（拆成更小子任务），(c) 挂起并通知用户。

### 5.5 可执行规范（Executable specs）

- Implementer **提交定义**：通过固定脚本（如 `cargo fmt --check`、`cargo clippy`、`cargo test -p ...`、项目既有的 pre-commit）。  
- **失败则禁止**将状态标为 `DONE` 或禁止进入 Reviewer/Verifier 下一阶段（由编排状态机定义）。  
- Specifier 产物可包含「契约测试」命令行，由 Verifier 阶段执行。

### 5.6 P0 落地草图：结构化 Verdict 端到端

P0 的核心工作：让子 Agent（尤其是 Reviewer）输出可被程序化消费的 JSON，不再依赖自然语言 grep。

#### 5.6.1 数据结构（改 `crates/tui/src/tools/subagent/mod.rs`）

在 `SubAgentResult` 中新增可选字段 `structured_verdict`：

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StructuredVerdict {
    pub verdict: VerdictLevel,       // "PASS" | "BLOCKER" | "MAJOR" | "FAIL"
    pub items: Vec<VerdictItem>,
    pub summary: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum VerdictLevel {
    #[serde(rename = "PASS")]
    Pass,
    #[serde(rename = "BLOCKER")]
    Blocker,
    #[serde(rename = "MAJOR")]
    Major,
    #[serde(rename = "FAIL")]
    Fail,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VerdictItem {
    pub severity: String,            // "BLOCKER" | "MAJOR" | "MINOR"
    pub file: String,
    pub line: Option<u32>,
    pub description: String,
    pub rule: Option<String>,        // 引用规范/规则 id，如 "TOKEN_INSECURE_RNG"
    pub suggestion: Option<String>,
}
```

#### 5.6.2 解析策略：JSON 围栏

LLM 直接输出裸 JSON 不可靠（容易夹带解释性文字）。采用 **`<!-- craft-verdict -->` 标记围栏** 或 **Markdown 代码块标注 `json`**：

```
<!-- craft-verdict -->
{"verdict": "BLOCKER", "items": [{"severity": "BLOCKER", "file": "auth/login.rs", "line": 42, "description": "OsRng 在 WASM 不可用", "rule": "CROSS_PLATFORM"}]}
```

Runtime 解析逻辑（`subagent/mod.rs` 中 `parse_structured_verdict` 函数）：

1. 在子 Agent 最终输出中搜索 `<!-- craft-verdict -->` 标记  
2. 取标记后第一个 `{` 到最后一个 `}` 的 JSON 块  
3. `serde_json::from_str` 反序列化  
4. 解析失败 → 字段保持 `None`，回退到自然语言解析（不阻塞流程）

#### 5.6.3 Reviewer 提示词增补

在 Reviewer 的系统提示词末尾追加：

```
## Output Format

After your review analysis, output a structured verdict in a JSON fence:

<!-- craft-verdict -->
{
  "verdict": "PASS" | "BLOCKER" | "MAJOR" | "FAIL",
  "items": [
    {
      "severity": "BLOCKER" | "MAJOR" | "MINOR",
      "file": "path/relative/to/repo/root",
      "line": <u32 or null>,
      "description": "what is wrong",
      "rule": "RULE_ID or null",
      "suggestion": "how to fix or null"
    }
  ],
  "summary": "one-line summary or null"
}

- "BLOCKER": must fix before merge (security, data loss, build break)
- "MAJOR": should fix (correctness, perf regression)
- "MINOR": nice to fix (style, nit)
- "PASS": no issues found
- "FAIL": review could not complete (env issue, missing context)
```

#### 5.6.4 消费端：两层解耦

P0 阶段只做**数据的结构化产出与解析**——让 `SubAgentResult` 携带 `structured_verdict` 字段。P2 阶段再让主 Agent 基于该字段做闭环分支。两层解耦避免 P0 改动过大。

**P0 改动（Rust 层——数据产出）**：在 `SubAgentResult` 中新增 `structured_verdict: Option<StructuredVerdict>` 字段。`parse_structured_verdict()` 解析子 Agent 输出中的 `<!-- craft-verdict -->` 围栏 JSON，解析成功就携带，失败就是 `None`。不做任何自动重派逻辑。P0 验收：调用 `agent_result` 查询 Reviewer，结果 JSON 中包含 `structured_verdict` 字典。

**P2 改动（Prompt 层——消费决策）**：主 Agent 系统提示词中增加结构化裁决的读取指令（见 5.4.1 决策块中的 prompt 片段）。主 Agent 看到 `structured_verdict.verdict == "BLOCKER"` → 调用 `agent_spawn` 派发新 Implementer，携带 `blockers[]` 作为修复目标。所有消费端逻辑落在 prompt 中，零 Rust 改动。

P0 阶段 `structured_verdict` 字段首先在 Reviewer 角色落地；Verifier 的结构化输出（`failures[]` + `observed`/`hypothesis`）作为 Reviewer 的并行改动同步推进。

改动量估算：`subagent/mod.rs` 约 70 行（`StructuredVerdict` / `VerdictItem` / `VerdictLevel` 结构体 + `parse_structured_verdict()` + `SubAgentResult` 字段），Reviewer 提示词约 25 行，消费端分支约 20 行。总计 ~120 行，可在单次 PR 完成。

### 5.7 P1 落地草图：文件型黑板端到端

P1 的核心工作：引入 `.deepseek/blackboards/{task_id}.json`，让 Explorer 的发现通过黑板传递给 Implementer，消除「主 Agent 口头转述」的信息损失。

#### 5.7.1 数据流与工程决策（P0 落地后结论）

| 工程问题 | 决策 | 依据 |
|----------|------|------|
| `task_id` 如何流入 spawn 链？ | `SpawnRequest` 加 `task_id: Option<String>`，`agent_spawn` 增加同名参数。与现有 `cwd` / `resident_file` 模式一致 | `parse_spawn_request` 已有 `cwd` 解析模式可复用 |
| 黑板文件何时创建？ | 首次 `agent_spawn` 携带 `task_id` 时，在 `run_subagent_task` 入口创建骨架（如果文件尚不存在）。`task_create` 也可预创建 | 避免主 Agent 额外的显式创建步骤 |
| 黑板何时写入？ | `run_subagent_task` 调用 `manager.update_from_result()` 之后，从 `SubAgentResult` 提取产出写入黑板对应分区 | 钩子点：`mod.rs` L2638 |
| 黑板注入位置？ | `build_assignment_prompt()` 增加 `blackboard_section` 参数，在 Task 行之前插入 Markdown 引用块 | 注入点：`mod.rs` L3623 |
| Explorer 结构化输出？ | P1 前期容忍 Explorer 无 `structured_verdict`，黑板 `explorer` 分区保持空。P1 后期通过 `EXPLORE_AGENT_PROMPT` 追加简易结构化约定（与 Reviewer 同模式） | 不阻塞 MVP；渐进覆盖 |

#### 5.7.2 代码改动清单

**Step 1：`SpawnRequest` 加 `task_id` 字段**（`mod.rs` L484）

```rust
struct SpawnRequest {
    // … 现有字段 …
    resident_file: Option<String>,
    /// Optional task id for blackboard association.
    /// When set, the child reads/writes `.deepseek/blackboards/{task_id}.json`.
    task_id: Option<String>,
}
```

**Step 2：`parse_spawn_request` 解析 `task_id`**（`mod.rs` L3351）

```rust
    let task_id = input
        .get("task_id")
        .and_then(|v| v.as_str())
        .map(str::to_string)
        .filter(|s| !s.trim().is_empty());

    Ok(SpawnRequest {
        // … 现有字段 …
        resident_file,
        task_id,
    })
```

**Step 3：`agent_spawn` schema 增加参数**（`mod.rs` L1610 附近）

```json
"task_id": {
    "type": "string",
    "description": "Optional task id for blackboard association"
}
```

**Step 4：`SubAgentTask` 加 `task_id`**（`mod.rs` L2603）

```rust
struct SubAgentTask {
    // … 现有字段 …
    task_id: Option<String>,
}
```

**Step 5：`build_assignment_prompt` 增加黑板注入**（`mod.rs` L3623）

```rust
fn build_assignment_prompt(
    prompt: &str,
    assignment: &SubAgentAssignment,
    agent_type: &SubAgentType,
    blackboard_section: Option<&str>,  // ← 新增参数
) -> String {
    let role = assignment.role.as_deref().unwrap_or("default");
    let header = format!(
        "Assignment metadata:\n- objective: {}\n- role: {}\n- resolved_type: {}",
        assignment.objective, role, agent_type.as_str()
    );
    if let Some(bb) = blackboard_section {
        format!("{header}\n\n## Blackboard\n{bb}\n\nTask:\n{prompt}")
    } else {
        format!("{header}\n\nTask:\n{prompt}")
    }
}
```

**Step 6：`run_subagent` 入口读取黑板**（`mod.rs` L2747）

```rust
async fn run_subagent(/* … */ task_id: Option<String>, /* … */) -> Result<SubAgentResult> {
    // Read blackboard at spawn time (snapshot — no live reload)
    let blackboard_section = task_id.as_deref()
        .and_then(|tid| read_blackboard_section(tid, &agent_type));
    
    let system_prompt = build_subagent_system_prompt(&agent_type, &assignment);
    // …
    let mut messages = vec![Message {
        role: "user".to_string(),
        content: vec![ContentBlock::Text {
            text: build_assignment_prompt(&prompt, &assignment, &agent_type, blackboard_section.as_deref()),
            cache_control: None,
        }],
    }];
```

**Step 7：`run_subagent_task` 末尾写黑板**（`mod.rs` L2638）

```rust
    let mut manager = task.manager_handle.write().await;
    match &result {
        Ok(res) => manager.update_from_result(&task.agent_id, res.clone()),
        Err(err) => manager.update_failed(&task.agent_id, err.to_string()),
    }

    // P1: write structured output to blackboard
    if let (Some(tid), Ok(ref res)) = (task.task_id.as_deref(), &result) {
        let _ = write_blackboard_partition(tid, &task.agent_type, res);
    }
```

**Step 8：黑板读写模块**（新增 `blackboard.rs`，~120 行）

```rust
// crates/tui/src/tools/subagent/blackboard.rs

use anyhow::Result;
use serde_json::Value;
use std::path::PathBuf;

/// Read the blackboard file and format the relevant section as Markdown
/// for injection into the child's assignment prompt.
pub fn read_blackboard_section(task_id: &str, agent_type: &SubAgentType) -> Option<String> {
    let path = blackboard_path(task_id);
    let raw = std::fs::read_to_string(&path).ok()?;
    let board: Value = serde_json::from_str(&raw).ok()?;

    // Different roles get different subsets
    let section = match agent_type {
        SubAgentType::Implementer => {
            // Implementer needs explorer findings + reviewer blockers
            format_explorer_findings(&board) + "\n" + &format_reviewer_blockers(&board)
        }
        SubAgentType::Reviewer => {
            // Reviewer needs implementer changes (to know what to review)
            format_implementer_changes(&board)
        }
        SubAgentType::Verifier => {
            // Verifier needs implementer changes (to know what to test)
            format_implementer_changes(&board)
        }
        _ => String::new(),
    };
    // …
}

/// Write one role's partition to the blackboard.
pub fn write_blackboard_partition(
    task_id: &str,
    agent_type: &SubAgentType,
    result: &SubAgentResult,
) -> Result<()> {
    // Read existing board (or create empty)
    // Update the partition key matching agent_type
    // Write back atomically (write temp → rename)
    // …
}
```

#### 5.7.3 改动量估算

| 改动 | 文件 | 行数 |
|------|------|------|
| `SpawnRequest` + `parse_spawn_request` | `subagent/mod.rs` | ~15 行 |
| `agent_spawn` schema | `subagent/mod.rs` | ~7 行 |
| `SubAgentTask` + `run_subagent` 入口 | `subagent/mod.rs` | ~20 行 |
| `build_assignment_prompt` | `subagent/mod.rs` | ~15 行 |
| `run_subagent_task` 写钩子 | `subagent/mod.rs` | ~8 行 |
| 黑板模块 | `subagent/blackboard.rs`（新文件） | ~120 行 |
| Explorer 提示词增补（后期） | `subagent/mod.rs` | ~15 行 |
| **总计** | | **~200 行** |

#### 5.7.4 验收标准

- 主 Agent 调用 `agent_spawn(type="explore", task_id="bugfix-001")` → Explorer 完成 → 黑板 `explorer` 分区写入 `findings[]`
- 主 Agent 调用 `agent_spawn(type="implementer", task_id="bugfix-001")` → Implementer 首条 user message 包含 `## Blackboard` 区块，内容为 Explorer 的结构化发现（`file` + `concern` + `severity`）
- 无 `task_id` 时黑板路径完全不触及，`blackboard_section = None`，行为与 P0 无差异

### 5.8 P2 落地草图：Fix-Loop 闭环协议

P2 是**纯 prompt 层改动**——在主 Agent 系统提示词中增加结构化裁决读取指令，不改 Rust 代码。利用 P0 的 `structured_verdict` 字段和 P1 的 `task_id` 黑板传播，实现自动 Review→Revise 和 Test→Fix 闭环。

#### 5.8.1 改动位置

单文件单处：`crates/tui/src/prompts/base.md`，在「Sub-agent completion sentinel」的 Integration protocol 之后，追加「CRAFT P2 fix-loop protocol」节。

#### 5.8.2 Prompt 增补内容

```
**CRAFT P2 fix-loop protocol:**

When you retrieve a sub-agent's result (via `agent_result` or the sentinel
summary), check whether the result payload carries a `structured_verdict`
object. If it does, follow this protocol:

1. Read `structured_verdict.verdict` — "PASS" | "BLOCKER" | "MAJOR" | "FAIL".

2. If "BLOCKER":
   - Do NOT ask the user. Do NOT mark the task complete.
   - Call agent_spawn(type="implementer", task_id="<same-task-id>") with each
     blocker item (file + line + description + suggestion).
   - After Implementer finishes, spawn Reviewer again.
   - Track Review→Revise cycles. After 3 without PASS → escalate to user.

3. If "FAIL" (Verifier):
   - Call agent_spawn(type="implementer", task_id="<same-task-id>") with
     observed failures + hypothesis from the Verifier output.
   - After Implementer finishes, spawn Verifier again.
   - Same escalation: 3 Test→Fix cycles without passing → escalate.

4. If "MAJOR":
   - Spawn Implementer with major items as context. Allow loop to proceed.

5. If "PASS":
   - Continue to next role. No fix-loop needed.

Always use the same `task_id` across fix-loop spawns so the blackboard (P1)
propagates structured context between agents.
```

#### 5.8.3 改动量

- `crates/tui/src/prompts/base.md`：**~25 行**
- **零 Rust 改动**

#### 5.8.4 依赖关系

P2 依赖 P0（`structured_verdict` 字段出现在 `agent_result` JSON 载荷中）和 P1（`task_id` 实现黑板上下文传播）。两个前提均已交付。

#### 5.8.5 验收标准

- 主 Agent 调用 `agent_result` → 看到 `structured_verdict.verdict == "BLOCKER"` → 在同 turn 内调用 `agent_spawn(type="implementer", task_id="<same>")` 携带 `blockers[]`
- 同一 task_id 下，第二轮 Reviewer 返回 PASS 时主 Agent 继续下一步而非再次 spawn
- 3 次闭环仍未 PASS 时，主 Agent 通知用户具体持久 blocker 列表

## 6. 分阶段实施路线（建议顺序）

与讨论一致：**先门禁与状态，再黑板，再自动重派，最后工作区副本与 Integrator**。

| 阶段 | 内容 | 关键改动点 | 验收 |
|------|------|-----------|------|
| ✅ **P0** | Implementer/Reviewer/Verifier 输出**结构化 JSON 区块**（`<!-- craft-verdict -->` 围栏），`SubAgentResult` 新增 `structured_verdict` 字段 | `subagent/mod.rs`：`StructuredVerdict` 结构体 + `parse_structured_verdict()`；Reviewer prompt 增补 JSON 输出约定 | 手动 spawn reviewer → 解析 `result.structured_verdict` 得到 `VerdictLevel::Blocker`；无结构化输出时回退不崩溃 |
| ✅ **P1** | 引入**文件型黑板**（`.deepseek/blackboards/{task_id}.json`）。`SpawnRequest` 加 `task_id`，`parse_spawn_request` 解析，`agent_spawn` schema 增加参数。`build_assignment_prompt` 注入黑板区块。`run_subagent_task` 末尾写黑板分区。新增 `blackboard.rs` | [P1 落地草图详见 5.7](#57-p1-落地草图文件型黑板端到端)。改动量 ~200 行。详见 5.7.2 八步改动清单 | 同上：Explorer 写入 → Implementer 首条 message 含 `## Blackboard` 区块。无 `task_id` 时行为与 P0 完全一致 |
| ✅ **P2** | **Review→Revise / Test→Fix** 由主 Agent 根据 `structured_verdict` **自动再 spawn**（选项 2——纯 prompt 层，零 Rust 改动）；N 次失败后升级主会话 | `prompts/base.md`：在 Sub-agent completion sentinel 段落后追加 CRAFT fix-loop protocol 节（~25 行） | Review 返回 `{"verdict":"BLOCKER"}` → 主 Agent 自动调用 `agent_spawn(type="implementer", task_id="<same>")` 携带 `blockers[]` → 第二轮 Review 返回 PASS |
| ✅ **P3** | **硬工具裁剪**：对 `explore`/`review` 在注册表层去掉写文件（`write_file`/`edit_file`/`apply_patch`）与危险 shell。与当前「子 Agent 默认全继承父 registry」的差异需在 PR 中显式改过，否则评审会误以为已具备硬裁剪 | `registry.rs`：角色级工具过滤（`SubAgentToolRegistry::new()` 根据 agent_type 分支裁剪 `allowed_tools`）；`build_allowed_tools()` 内默认列表改为显式只读子集。与 [SUBAGENTS.md](SUBAGENTS.md) 中现有 `allowed_tools()` 的废弃注释一致 | 误调用写文件工具在框架层不可达；`explore` 调用 `write_file` 时返回 `"tool not allowed for this role"` |
| ✅ **P4** | **Implementer pre-flight git stash 快照**（安全网）。`run_subagent_task` 入口，若 agent_type 为 Implementer，执行 `git stash push --include-untracked -m "craft-auto-{agent_id}"`。不自动恢复——stash 是手动回退辅助，非自动化机制 | `subagent/mod.rs`：`run_subagent_task` 入口加 ~13 行 | `git stash list` 可见 `craft-auto-*` 条目。git 不可用或无变更时静默跳过，不阻塞任务 |

> **P4 原始方案（工作区副本 + Integrator）降级为独立 RFC。** P0-P3 已将需要隔离的场景压缩到极低概率，完整 worktree 方案的工程代价（~500 行）与当前边际收益不成比例。

## 7. 工程提示与交付格式（低风险增效）

独立于 CRAFT：**细化工程提示词** 要求助手按「变更文件 → 条目化意图与关键点」输出（你已验证的规范摘要），便于人工快速对照 diff，降低「读着像对」没被审查的风险。

建议在产品侧可选提供 **「变更摘要」模板**（不强制改写模型），例如在 Zagens / 会话约定中固定小节标题。

## 8. Zagens / 运行时注意事项

- 桌面端不改变「单引擎真理」：**编排增强应优先落在 `serve --http` 与 thread/engine 一层**，前端消费事件与黑板路径即可对齐 TUI。  
- 与安全规则一致：**路径规范化、密钥不入黑板明文**、沿用现有 workspace / trust / approval。

## 9. 风险与权衡

| 风险 | 说明 | 缓解 |
|------|------|------|
| **成本与延迟** | 多角色串行 + 多轮验证增加了 token 与时间。典型 CRAFT 流程（Explorer→Implementer→Reviewer→Implementer→Verifier→Implementer→Verifier）token 消耗可能是当前单次流程的 **3-5 倍**。每次 Verifier 涉及 `cargo test --workspace` 可能分钟级编译延迟 | P0–P2 先验证闭环逻辑是否正确；Verifier 阶段引入**增量测试策略**：若只改了 `crates/auth`，仅跑 `cargo test -p auth`，不跑全工作区 |
| **Flaky 测试** | 易导致误循环；需在 Verifier 层标记 flaky/retry 策略 | Verifier 的 `observed` 字段携带完整日志；对同一测试连续 2 次失败才标记为 failure |
| **过度工程** | P0–P2 已到大部分收益；P4 按真实痛点启动 | P4 独立为 RFC，不阻塞前序阶段 |
| **解析脆弱** | LLM 输出需 schema 或 JSON 围栏，避免单靠自由文本正则 | `<!-- craft-verdict -->` 围栏 + `serde_json` 解析；解析失败回退到自然语言（graceful degradation） |
| **Verifier 根因不准确** | Verifier 只有日志与 diff，假设可能误导 Implementer | 明确分 `observed`（事实）和 `hypothesis`（推测+置信度）；Implementer 读但自己验证源码 |
| **单 Agent 隐性成本被低估** | 当前单 Agent 在长会话中反复试错，很多轮次消耗在"理解现状"上，且缺乏角色分工导致 token 浪费；CRAFT 虽串行但每个角色带精准上下文入场，可能减少无效轮次 | P1 跑通后执行 **A/B 对比**：① 选 3 个典型任务（简单 bug 修复 / 中等重构 / 跨模块变更）；② CRAFT 完整流程各跑 3 次，单 Agent 各跑 3 次；③ 对比维度：总 token 消耗、到第一次正确输出的轮次、人工评估正确率。结果驱动 P2 的投入决策 |

## 10. 相关文档

- [SUBAGENTS.md](SUBAGENTS.md) — 角色与工具表面  
- [RUNTIME_API.md](RUNTIME_API.md) — HTTP/SSE 契约  
- [MODES.md](MODES.md) — Plan / Agent / YOLO 与审批  
- [docs/desktop/DESKTOP_IMPLEMENTATION_PLAN.md](desktop/DESKTOP_IMPLEMENTATION_PLAN.md) — 桌面与 sidecar  
- [docs/tui/PROMPT_ANALYSIS.md](tui/PROMPT_ANALYSIS.md) — 提示与委托策略  
- [docs/LOCALIZATION.md](LOCALIZATION.md) — TUI 侧 locale（桌面 i18n 可日后对齐）

---

## 附录 A：行业「目标 / 闭环」形态与 CRAFT 的母题对齐

> **说明：** 本节整理自公开测评、推文与图解类二手材料，用于**对齐概念**；不替代各产品官方文档，亦不构成对其工程实现的担保。

多款编程 Agent 产品在相近时期集中强调 **`/goal`、Ralph Loop、completion condition** 等能力，与本文 **Verifier + 结构化裁决 + Fix-Loop** 同属一条母题：**完成条件必须可核验，且不能仅依赖执行模型自证「做完了」**。

| 侧重点（市面叙述） | 典型做法（概念层） | 与本文 CRAFT 的映射 |
|--------------------|---------------------|---------------------|
| **Codex**：持久化、`update_goal`、断点续跑、上下文将尽时「软着陆」 | 目标作为**会话外仍存在的对象**，进度可写回本地层 | 可与 **P1 黑板 `task_id` + 会话持久化** 结合扩展为「显式 Goal 记录与恢复」 |
| **Hermes**：看板 / SQLite、多 Worker 进程、心跳与僵尸回收、**嘴上说完成要先过验证**、`/rollback` 与快照 | **调度隔离 + 不烂尾 + 可回滚** | 对齐 **P3 工具硬裁剪**、Verifier **observed 事实门禁**；P4 stash 快照是轻量后悔药；**完整 worktree + 调度器** 仍可待独立 RFC |
| **Claude Code**：`/goal` 条件循环、**独立小模型当验收官**（与执行模型分离）、`--resume` / 非交互 `-p`、`Agent View` | **Dual judge** + CLI/CI 友好 | 本文 P2 以 **`structured_verdict` + prompt 分支**起步；远期可加 **Cheap model / 规则引擎二次验收**，降低仅依赖主模型读 JSON 的脆弱性 |

**结论：** 「/goal」类能力与 CRAFT **不是两套哲学**——包装不同，核心都是 **门禁 + 闭环 +（可选）独立验收**。本仓库落点重在 **开源 runtime、`agent_spawn` 角色 taxonomy、黑板渐进落地**。

---

## 11. 后续改进方向

以下按 **与 CRAFT 直接相关 → 产品与生态 → 证据与演进** 排列，可作 issue/里程碑 backlog。

### 11.1 与 CRAFT 直接衔接

1. **端到端验证与 A/B** — 对比「单 Agent 长会话」与「CRAFT 角色链 / 黑板」：首轮正确率、闭环次数、token 与墙钟时间、人工介入次数。（亦见 §9 风险表。）
2. **结构化输出质量数据** — 统计 `structured_verdict` 解析成功率、Reviewer 假阳性/假阴性、Verifier `hypothesis` 相对最终根因的命中率；驱动提示词或小验收模型迭代。
3. **程序化闭环（Rust 钩子，可选）** — 若 P2 prompt 在实践中易被主模型跳过，评估 **spawn/turn 结束回调**（§5.4.1 选项 3）：仅处理 `BLOCKER` + `task_id` 再派发，减小对提示词依赖。
4. **Goal 显性化（产品概念）** — UI/会话层展示「当前目标」，与 `task_id`/黑板绑定；远期支持恢复会话后继续同一 goal，贴近市面 `--resume` 叙事，底层仍复用 thread/session/blackboard。
5. **P4 完整版 RFC（按需）** — `git worktree` / 副本工作区 + Integrator 合并闸门；仅在强隔离或多并行需求明确时再开。

### 11.4 并行写与 LSP（§3.2 落地项）

与 [§3.2](#32-并行工具调度与子代理写路径现状核对) 对照，可独立于黑板 P1 推进的短项：

| 优先级 | 项 | 改动要点 | 验收 |
|--------|-----|----------|------|
| **P0** | 子代理 LSP | `build_tool_context` / `SubAgentRuntime::child_runtime` 传递 `with_lsp_manager(engine.lsp_manager.clone())`；可选在子代理 step 末 flush diag | `implementer` 子代理 `edit_file` 后 tool 结果或下一轮可见 `<diagnostics>` |
| **P1** | `resident_file` 硬锁 | lease 冲突时拒绝 spawn 或排队，而非仅 JSON warning | 两路 spawn 同 `resident_file` 时第二路失败或可观测等待 |
| **P2** | 集成回合 | 子代理返回 unified diff / patch 文本；主代理单轮 `apply_patch` + `exec_shell` 验证 | 无并行落盘；冲突在主代理可见 |
| **P4** | worktree | 见 §11.1 第 5 项 | 子代理写隔离副本，Integrator 合并 |

### 11.2 工作区约束与用户体验

6. **工作区规则产品化** — 在既有 `instructions = [...]`（见 `config.example.toml`）之上，可增加 **约定文件自动装载**（如 `PROJECT_RULES.md`）、Zagens **只读展示已加载规则**。`.cursor/rules` 不会自动进 runtime，需显式映入 `instructions`。
7. **Zagens** — Web UI **i18n**、**更新/升级**（从「关于 + 跳转 Release」到 Tauri updater）、对 **黑板 / task** 状态的轻量可视（仍遵守 §8：编排优先落在 `serve --http` 层）。

### 11.3 文档、安全与近期动作

8. **短文档蒸馏** — 将附录 A 与 §7「变更摘要」收成 contributor/用户向一页速读。
9. **安全与供应链** — 自动化更新若涉及二进制拉取，须对齐 [SECURITY.md](SECURITY.md) 与 `.cursor/rules/security-trust.mdc`。
10. **即刻可做** — 跑通全链 CRAFT、收集围栏缺失/JSON 截断样本、按上表拆解带验收标准的 issue。

### 11.5 两个「金矿」backlog（2026-05-30 设计对话）— ⬜ 规划中

来源：[`desktop/DEV_NOTES.md` §2026-05-30 抗幻觉工程哲学](desktop/DEV_NOTES.md)「人类工程机制 ↔ harness」映射表中**尚未映射**的两条。按人类工程方法论，二者都属「抗幻觉上游收益最高」，但**0.8 引擎分离之后再做**（现阶段先把幻觉率与稳定性踩平，勿同时调多个动目标）。

| # | 金矿 | 对应 CRAFT 字母 | 缺口 / 动机 | 落点 | 验收草案 |
|---|------|------|------|------|----------|
| ① | **设计评审前置（design review）** | **R**（Review 前移） | 现 CRAFT 重「事后审代码」；人类工程最省钱的审查是「动手前审方案」——错误设计实现得再漂亮也白干。抓幻觉要趁它**还是计划里一句话**，而非已写进 N 个文件 | LHT plan / 任务图阶段加一道「方案审」闸门（见 [harness/LONG_HORIZON_CODE_TASKS.md](harness/LONG_HORIZON_CODE_TASKS.md)）；可复用 Reviewer 角色，输入从 diff 换成 plan/任务图 | 任务图派生后、首个 Implementer 动手前，产出一份对「目标↔任务图」的结构化 verdict（PASS/BLOCKER），BLOCKER 阻止进入实现 |
| ② | **可追溯矩阵（traceability）** | **T**（Traceable，落实命名里已有的字母） | 现已绑「实现↔验证」（`[verify:]`），差锚回「目标/需求」。堵死 DEMO3 那种「验收项在分解时悄悄变形」（runnable acceptance 塌缩成 create-only） | checklist item / 任务图节点增加回指「原始目标/需求」的 trace 字段，形成 `目标 ↔ 实现 ↔ 验证` 三元绑定；黑板 `task_id` 已是天然锚 | 任一清单项完成可反查「它满足了哪条原始要求」；目标未被任何 verify 项覆盖时可检出告警 |

> 与「单模型自审共谋盲区」配套结论（同源对话）：**S3 程序化校验优先级应高于 S1 双模型 Judge**——让不会幻觉的工具（编译器/测试/`read_file` 真实调用记录）当终审，而非引第二个模型。见 [`craft-v2-improvements.md` §4 S1/S3](craft-v2-improvements.md)。

> **首个落地抓手（2026-05-30）：** 二者已在「**全新项目并行代码生成**」方案中找到具体落点 —— ①设计评审前置 = 并行分发前的 **P0.5 契约固化闸门**；②可追溯矩阵 = 集成前 **P1.5 符合性审核**天然产出的 `契约条款 ↔ 模块 ↔ 审核` 绑定表。详见 [`harness/PARALLEL_FRESH_GENERATION.md`](harness/PARALLEL_FRESH_GENERATION.md)。

---

**文档修订记录（摘要）：** 新增 §11.5（两个金矿 backlog：设计评审前置 / 可追溯矩阵，源自 DEV_NOTES 2026-05-30）；增补 §3.2（并行调度与子代理写路径现状核对）、§11.4（LSP/锁/集成回合落地表）；既往：附录 A、§11；§10 增加 LOCALIZATION 链接。
