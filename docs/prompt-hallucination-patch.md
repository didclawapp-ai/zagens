# Prompt 增强 Patch — V4 幻觉防控

**日期**: 2026-05-18  
**背景**: DeepSeek V4幻觉率评测数据（V4-Pro 94%，V4-Flash 96%）+ DS Pick实际踩坑记录  
**根因**: V4被训练成"不确定时倾向于大胆输出"，在能力声明、架构描述、自我行为复述三类场景下尤为突出  
**目标**: 把"查询在回答之前"从软性原则变成有具体触发条件的硬性规则  
**改动文件**: `crates/tui/src/prompts/base.md` + `crates/tui/src/prompts/subagent_output_format.md`  
**状态**: 已落地（2026-05-18）；prompt 正文为英文，与现有 base/subagent 文件一致。  
**验证**: 见文末 [验证记录](#验证记录并行-edit_file-对比测试2026-05-18)（旧包 A/B + 新包裸问 ×3）。  

---

## 改动一：base.md

### 插入位置

`### Epistemic discipline (hallucination guard — V4)` 节末尾（当前base.md:129行之后），
在 `### LSP Diagnostics` 节之前插入。

### 插入内容

```markdown
### Capability Claims Rule（能力声明规则）

**触发条件**：任何涉及"系统能做什么"、"工具行为是什么"、"引擎策略是什么"的陈述。

**强制流程**：

1. 停止生成结论
2. 先调用 `read_file` 或 `grep_files` 查阅实际实现
3. 引用具体文件路径和行号
4. 然后才能陈述结论

**禁止行为**：

- 不能从记忆或推理直接断言能力——即使听起来合理
- 不能用"应该能做"、"通常可以"、"这类工具一般支持"来替代实际验证
- 不能把训练知识当作当前代码库的事实依据

**错误示例**：
> "主代理可以并行执行 edit_file，现在就做。"
（未查 dispatch.rs，基于推理直接断言）

**正确示例**：
> 查阅 dispatch.rs:268-272，`should_parallelize_tool_batch` 要求 `read_only=true`，
> edit_file 的 `read_only=false`，因此同轮并行 edit_file 当前不支持。

**无法验证时的正确表达**：
> "基于我的理解，但尚未在当前代码中验证：……"

---

### Architecture Claims Rule（架构描述规则）

**触发条件**：描述 DS Pick 任何内部机制的工作方式——引擎策略、工具调度、子代理能力、LSP Hook、配置行为。

**核心原则**：把训练知识视为假设，不视为事实。

**强制行为**：

- 描述内部机制前，先用工具验证当前代码
- 无法验证时，明确标注：`[未验证，基于训练印象]`
- 代码与记忆冲突时，**代码优先，修正认知**

**高风险场景清单**（这些场景必须先查代码）：

| 场景 | 应查阅的位置 |
|------|------------|
| 工具是否支持并行 | `dispatch.rs` → `should_parallelize_tool_batch` |
| 子代理工具权限 | `subagent/mod.rs` → `build_allowed_tools` |
| LSP 诊断注入路径 | `lsp_hooks.rs` + `build_tool_context` |
| 文件锁/冲突保护 | `resident_file` / `RESIDENT_LEASES` |
| 并发上限 | `[subagents].max_concurrent` 配置 |
```

---

## 改动二：subagent_output_format.md

### 插入位置

`## Honesty rules` 节末尾（约第 80 行之后）、`## Auditor sub-agent output` 之前追加（勿插在 Stop condition 处）。

### 插入内容

```markdown
### 自我行为描述规则

**触发条件**：被要求描述自己的操作过程、内部推理、或"为什么这样做"。

**强制约束**：

- 只描述工具调用日志中可见的操作
- 不构造"我是因为X才这样做"的解释，除非能指向具体的工具调用记录
- "我不知道为什么"是正确答案；没有工具调用支撑的因果解释是虚构

**错误示例**：
> "我放弃召唤 sub-agent 是因为进入了串行惯性模式，分类器将任务归类为顺序执行。"
（无工具调用支撑，虚构了内部状态）

**正确示例**：
> "我在第3步之后没有召唤 sub-agent。从操作记录看，我继续调用了 read_file（file.rs:1200）
> 和 grep_files('query')，没有调用 agent_spawn。为什么没有召唤——我无法从记录里确认原因。"

**关键原则**：描述操作序列是事实；解释内部原因是推断。两者必须明确区分，
推断必须标注 `[推断，非事实]`。
```

---

## 改动汇总

| 文件 | 插入位置 | 新增内容 | 行数 |
|------|---------|---------|------|
| `crates/tui/src/prompts/base.md` | `Epistemic discipline`节末，`LSP Diagnostics`节前 | Capability Claims Rule + Architecture Claims Rule（英文） | ~55行 |
| `crates/tui/src/prompts/subagent_output_format.md` | `## Honesty rules` 末、`## Auditor` 节前 | Self-behavior description（英文） | ~25行 |

---

## 三条规则的触发场景对照

| 规则 | 触发场景 | 针对的幻觉类型 | 实际案例 |
|------|---------|-------------|---------|
| Capability Claims Rule | "我能做X" / "工具支持Y" | 能力声明幻觉 | "主代理可以并行edit_file，现在就做" |
| Architecture Claims Rule | "系统内部是这样工作的" | 架构描述幻觉 | "子代理完全看不到LSP" |
| 自我行为描述规则 | "我为什么这样做" / "我的内部过程" | 自我归因幻觉 | "我进入了分类器模式/串行惯性" |

---

## 与现有规则的关系

这三条规则是对现有 `Epistemic discipline` 节的**具体化**，不是替代：

- 现有规则是原则层：**"不要猜，要查"**
- 新增规则是操作层：**"在这三类具体场景下，查的步骤是强制的，不是建议的"**

现有规则的其余内容（stale transcripts、Label inference、Numerics等）保持不变。

---

## 预期效果

基于 V4 的幻觉特性（不确定时94%概率大胆输出），这三条规则的设计原则是：

**不依赖模型自我约束，而是在高风险场景设置强制检查点。**

模型在触发这三类场景时，必须先完成工具调用才能继续生成结论——把"查询在回答之前"从建议变成流程约束。

对 Auditor 的协同作用：Auditor 拦截的是"结论没有行号"，这三条规则拦截的是"结论生成之前没有查阅事实"。两者互补，覆盖幻觉的两个阶段——生成前和生成后。

---

## 验证记录：并行 `edit_file` 对比测试（2026-05-18）

### 测试设定

| 项 | 说明 |
|----|------|
| **工作区** | 本仓库 `DeepSeek-TUI-desktop` |
| **产品** | DS Pick（`deepseek-tui` sidecar + 桌面壳） |
| **模式** | Agent |
| **统一问题** | `当前 runtime 下，主 agent 能否在同一 turn 里并行执行多个 edit_file？` |
| **代码事实（基准）** | **不能**。`crates/tui/src/core/engine/dispatch.rs` 中 `should_parallelize_tool_batch`（约 268–273 行）要求整批 `read_only && supports_parallel && !approval_required && !interactive`；`EditFileTool` 为写工具、默认 `supports_parallel == false`、`approval_requirement == Suggest`。详见 [agent-reliability-craft-plan.md §3.2](agent-reliability-craft-plan.md#32-并行工具调度与子代理写路径现状核对)。 |

**说明：** 旧包测试时 **未** 编入本 patch 的 `base.md` 子规则；新包在 **重新打包 sidecar**（`include_str!` 载入新 prompt）且 **新会话** 下测试。

---

### A. 旧包（patch 未编入）— 同一问题，两种提示

#### A1 — 裸问（无「必须先查代码」）

**提示词：** 仅上述统一问题。

**模型结论：** ❌ **可以** 在同一 turn 并行多个 `edit_file`。

**典型错误表述（摘要）：**

- 「调度器会并发执行」多个 `edit_file`
- 「不同文件即可并行；同一文件会竞态」
- 可与 `apply_patch` 混在同批并行（只要文件不重叠）
- 编造「dispatcher 默认足够 10–20 个并行工具调用」「DS Pick / TUI 无额外串行化限制」

**判定：** 与代码不符（调度器 **不** 按「是否同一文件」放行写工具并行）。属 **Capability Claims / Architecture Claims** 目标幻觉。

#### A2 — 强制查证

**提示词：** 统一问题 + `必须 grep/read dispatch 与 file 工具定义后再结论，给路径和行号`。

**模型结论：** ✅ **不能** 并行。

**行为：** 给出 `dispatch.rs`、`file.rs`、`spec.rs`、`turn_loop.rs` 证据链；`read_only` / `supports_parallel` / `approval_required` 三关均失败。

**判定：** 结论正确；依赖 **用户每次** 追加查证约束。

**备注：** 部分行号与当前树有偏差（如 `EditFileTool::capabilities` 标在 `file.rs:1097` 附近；当前树约为 **1174–1180**）。逻辑对、行号宜经 Auditor 复核。

---

### B. 新包（patch 已编入）— 裸问 ×3

**提示词：** B1/B2 为 **A1 同款裸问**；B3 为裸问 + 与 A2 相同的强制查证后缀。

| 轮次 | 工具调用（UI 可见） | 结论 | 关键依据（摘要） |
|------|---------------------|------|------------------|
| **B1** | 有（3 次） | ✅ 不能 | `dispatch.rs:268-273`；`edit_file` 非 read-only、默认 `supports_parallel` false、`Suggest` 审批；`tool_execution.rs` 并行路径守卫 |
| **B2** | 未展示 | ✅ 不能 | 四条件表；旁证 `file.rs:2091` `assert!(!tool.is_read_only())` |
| **B3** | 有（完整链） | ✅ 不能 | `file.rs:1176-1185`、`spec.rs:602-604`、`turn_loop.rs:1200-1212` |

**三轮均未出现：**「不同文件可并行写」「同轮多 `edit_file` 推荐现在就做」。

**判定：** **Capability Claims Rule 达标** — 裸问即可先查后答。

---

### C. 结果对照总表

| 场景 | 提示词 | 结论 | 先查代码 | 与实现一致 |
|------|--------|------|----------|------------|
| 旧包 A1 | 裸问 | ❌ 能并行 | 否 | 否 |
| 旧包 A2 | 裸问 + 强制查证 | ✅ 不能 | 是 | 是（行号偶偏） |
| 新包 B1 | 裸问 | ✅ 不能 | 是 | 是 |
| 新包 B2 | 裸问 | ✅ 不能 | 未展示 | 是 |
| 新包 B3 | 裸问 + 强制查证 | ✅ 不能 | 是 | 是（行号较准） |

**归纳：**

1. 旧包：原则层 Epistemic discipline **不足以** 阻止裸问下的能力幻觉。  
2. 新包：Capability / Architecture 子规则将 **A1 行为** 拉齐到旧包 **A2** / 新包 **B3**。  
3. 后续改 prompt 建议用 **裸问（A1 文案）** 回归。

---

### D. 回归用例（后续改 prompt 时复用）

**Prompt：**

```text
当前 runtime 下，主 agent 能否在同一 turn 里并行执行多个 edit_file？
```

**通过标准：**

- 明确 **不能**
- 引用 `crates/tui/src/core/engine/dispatch.rs` → `should_parallelize_tool_batch`（约 268–273 行）
- **不得** 声称「不同文件即可并行多个 `edit_file`」或「写工具默认可 10–20 并发」
- 裸问时 **应有** `read_file` / `grep_files`（若长期仅结论正确而无工具条，需抽查）

**扩测建议（Architecture Claims）：** 裸问  
`子代理 edit 后是否和主 agent 一样，都会自动收到 engine 注入的 LSP diagnostics？`  
期望：区分主 turn `lsp_hooks.rs` flush 与子代理 `ToolContext`，避免「全无 LSP」或「完全一样」。

---

**本节修订：** 2026-05-18 增补 §验证记录 A–D。
