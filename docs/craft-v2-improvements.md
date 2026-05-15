# CRAFT V2 改进方案

**状态**: 草案  
**基于**: craft-demo-001 实战验证 + 代码评审发现  
**日期**: 2026-05-15

---

## 问题诊断

CRAFT V1（当前已实施的 P0-P4）将软件开发流程拆成了四个角色，但每个角色的**能力边界**还有三个缺口：

| # | 缺口 | 证据 | 后果 |
|---|------|------|------|
| C1 | Reviewer 不能验证编译 | `build_allowed_tools` 中 Review 无 `exec_shell` | 审查不完整——只能做静态检查 |
| C2 | Explorer 的覆盖率没标准 | Demo 中 Explorer 说"读完了"就完了 | 可能漏关键文件，Implementer 基于不完整信息动手 |
| C3 | 黑板的 `rounds` 字段永远是空数组 | `write_blackboard_partition` 写死 `"rounds": json!([])` | fix-loop 的迭代历史丢失，无法回溯 |

另有三个结构性限制暂不纳入本次迭代（见 §4）。

---

## 改进方案

### C1：Reviewer 加只读 shell 能力

**当前**：Review 工具列表 = `list_dir, read_file, grep_files, file_search, note`

**改为**：加 `exec_shell`，但通过系统 prompt 约束为"只用于验证命令"

```
Review 工具列表: list_dir, read_file, grep_files, file_search, exec_shell, note
```

**安全边际**：
- P4 stash 快照在 Implementer 执行前已创建——即使 Reviewer 的 shell 被滥用，代码可恢复
- Reviewer 的系统 prompt 已在 `build_subagent_system_prompt` 中约束其行为为"审查而非修改"
- 不改 `exec_shell` 本身的实现——改动范围仅一行工具列表

**改动量**：

| 文件 | 位置 | 改动 |
|------|------|------|
| `crates/tui/src/tools/subagent/mod.rs` | L3850 | 在 Review 的 vec 中加 `"exec_shell"` |

**验收**：派发 Review agent → 确认它能跑 `cargo check -p xxx 2>&1` 并拿到结果

---

### C2：Explorer 覆盖率要求

**当前**：Explorer 的系统 prompt 只说"分析代码"，没有"你覆盖全了吗"的检查点

**改为**：两层改进

**2a — 系统 prompt 加硬性输出要求**（修改 `build_subagent_system_prompt` 中 Explorer 分支）：

在 Explorer 的任务结束提示中加：

```
Before completing your analysis, append a ## Coverage Report section:

- Files examined: [list every file path you read]
- Files NOT examined that may be relevant: [list paths you suspect are relevant but didn't read, with reasons]
- Confidence: [high / medium / low] — if medium or low, explain what you would need to read to reach high
```

**2b — 黑板 explorer 分区加字段**（修改 `blackboard.rs` 的 `write_blackboard_partition`）：

```json
"explorer": {
  "findings": [...],
  "impact_summary": "...",
  "files_examined": ["path1", "path2", ...],   // 新增
  "coverage_confidence": "high"                  // 新增
}
```

`files_examined` 从 Explorer 的输出中解析（匹配 `## Coverage Report` 段落的文件列表）。

**改动量**：

| 文件 | 位置 | 改动 |
|------|------|------|
| `crates/tui/src/tools/subagent/mod.rs` | `build_subagent_system_prompt` 中 Explorer 分支 | +~15 行 prompt |
| `crates/tui/src/tools/subagent/blackboard.rs` | `write_blackboard_partition` | +~10 行（解析 + 新字段） |

**验收**：派发 Explorer → 确认输出的 `## Coverage Report` 非空 → 确认黑板 `files_examined` 有内容

---

### C3：黑板的 `rounds` 字段记录真实 fix-loop 迭代

**当前**：`write_blackboard_partition` 中死代码：

```rust
"rounds": json!([]), // placeholder — filled by merge logic
```

这个"merge logic"从未实现——`rounds` 永远是空数组。

**改为**：Implementer 每次完成后，追加当前 round 的记录

**数据结构**：

```json
"implementer": {
  "rounds": [
    {
      "round": 1,
      "prompt": "将 take() 替换为 get_or_insert_with",
      "changes": ["crates/desktop/src/commands.rs:987"],
      "reviewer_verdict": "BLOCKER",
      "blockers": ["compilation not verified"]
    },
    {
      "round": 2,
      "prompt": "修复 Reviewer 发现的编译问题",
      "changes": ["crates/desktop/src/commands.rs:987"],
      "reviewer_verdict": "PASS",
      "blockers": []
    }
  ]
}
```

**实现方式**：

`write_blackboard_partition` 已经收 `agent_type` 和 `SubAgentResult`。当 `agent_type == Implementer` 时：
1. 读取现有黑板的 `implementer.rounds` 数组
2. 追加当前 round（从 `SubAgentResult` 提取 changes summary + 上一轮 Reviewer 的 verdict）
3. 写回

**改动量**：

| 文件 | 位置 | 改动 |
|------|------|------|
| `crates/tui/src/tools/subagent/blackboard.rs` | `write_blackboard_partition` | ~30 行 |
| `crates/tui/src/tools/subagent/mod.rs` | `run_subagent_task` 调用处 | 传入 Reviewer verdict（可选——也可在 blackboard 内部读上一分区） |

**验收**：跑 CRAFT 链 → 确认 `.deepseek/blackboards/{task_id}.json` 中 `implementer.rounds` 数组长度 ≥ 1 → 每个 round 含 `prompt`、`changes`、`reviewer_verdict`

---

## 实施顺序

| 顺序 | 改进 | 理由 |
|:--:|------|------|
| 1 | **C1** — Reviewer 加 shell | 一行改动，即时收益——消除"审查不完整"这个最大痛点 |
| 2 | **C2** — Explorer 覆盖率 | 防止 CRAFT 链在第一步就走偏——信息源不完整，后面全歪 |
| 3 | **C3** — rounds 记录 | 可追溯性是闭环的基础——后续 P2 自动 fix-loop 需要读历史 |

总改动量：~65 行 Rust + ~15 行 prompt。

---

## 不在本次迭代的结构性限制

| # | 限制 | 为什么不现在做 |
|---|------|--------------|
| S1 | Dual Judge — Reviewer 和主 Agent 是同一模型 | 需要独立模型实例或规则引擎——架构改动大，先验证单模型闭环是否有效 |
| S2 | 子 Agent 上下文软着陆 | 需要子 Agent 的上下文快照 + 恢复机制——改动 ~200 行，优先级低于 C1-C3 |
| S3 | Explorer 输出的程序化校验（"你真的读了那 5 个文件吗"） | 需要校验层接入 `SubAgentResult`——可以做，但 C2 的 prompt 层要求先验证效果 |

---

## 验证方法

全部三个改进用同一个 demo task 验证——与 `craft-demo-001` 相同的 tracing 添加任务，对比 C1-C3 落地前后的差异：

| 维度 | V1 (已跑) | V2 预期 |
|------|----------|---------|
| Reviewer 跑 `cargo check` | ❌ 被 P3 裁剪阻止 | ✅ 直接执行 |
| Explorer 输出覆盖率报告 | ❌ 无 | ✅ `## Coverage Report` + `files_examined` |
| Blackboard `rounds` | 空数组 | 含 Implementer 每次迭代记录 |
