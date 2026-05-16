# 工具链改进方案：基于全量代码审查中暴露的问题

**日期**：2026-05-17  
**来源**：对 DS Pick / deepseek 仓库全量安全审查过程中实际遇到的工具障碍

---

## 问题清单与根因分析

| # | 问题 | 严重度 | 根因 |
|---|------|--------|------|
| 1 | `file_search` 返回 `.git/objects/` 等二进制路径，淹没有效结果 | 高 | 未设置排除模式；对比 `grep_files` 已有默认排除 |
| 2 | `grep_files` 每次返回无用的 `symbol_index_hits`，且始终指向 `schema_migration.rs:83` | 中 | 符号索引查询总是执行且结果与查询无关时仍输出 |
| 3 | `agent_spawn` 120s per-step 硬超时，审查任务中 4/5 子代理失败 | 高 | `STEP_API_TIMEOUT` 硬编码，不可配置 |
| 4 | `read_file` 不返回结构化的 `total_lines`，依赖截断消息中的模糊提示 | 中 | 行数仅在截断时以自然语言形式出现 |
| 5 | 子代理无中间进度事件，父线程在 120s 内完全黑盒等待 | 中 | 子代理独立会话，仅最终 `subagent.done` sentinel |
| 6 | `symbol_index_hits` 行号对宏展开代码不准确且每次 grep 都输出 | 低 | `syn` span 固有局限；无按需查询机制 |

---

## 改进方案

### 改进 1：`file_search` 添加默认排除目录

**影响文件**：`crates/tui/src/tools/file_search.rs`

**现状**（第 119-121 行）：
```rust
let mut builder = WalkBuilder::new(base_path);
builder.hidden(false).follow_links(true).require_git(false);
let walker = builder.build();
```

`WalkBuilder` 来自 `ignore` crate，支持 `.gitignore` 规则和自定义排除，但当前未使用。

**方案**：添加与 `grep_files` 一致的默认排除列表，并尊重 workspace 的 `.gitignore`：

```rust
let mut builder = WalkBuilder::new(base_path);
builder
    .hidden(false)
    .follow_links(true)
    .require_git(false)
    .git_ignore(true)            // 新增：尊重 .gitignore
    .git_global(true)            // 新增：尊重全局 gitignore
    .filter_entry(move |entry| { // 新增：硬排除常见非源码目录
        let name = entry.file_name().to_string_lossy();
        !matches!(name.as_ref(),
            "target" | "node_modules" | ".git" | "dist" | "build"
            | "__pycache__" | ".venv" | "venv" | ".turbo" | ".next"
        )
    });
```

**效果**：`file_search .rs` 不再返回 `.git/objects/`、`target/` 下的路径。

---

### 改进 2：`grep_files` 按需启用 `symbol_index_hits`

**影响文件**：`crates/tui/src/tools/search.rs`

**现状**（第 227-264 行）：每次 `grep_files` 都调用 `lookup_symbol_hits()`，无论用户是否需要。返回的 `symbol_index_hits` 常包含与查询无关的条目（如始终指向 `schema_migration.rs:83`），且 `symbol_index_summary` 无条件填充。

**方案**：

**步骤 A** — 增加 `symbol_index` 参数（默认 `false`）

在 input schema 中新增可选布尔参数：

```json
"symbol_index": {
    "type": "boolean",
    "description": "Also query the symbol index for definitions matching the pattern (default: false). Symbol line numbers may drift for macro-expanded code."
}
```

**步骤 B** — 条件执行符号查询

```rust
// 第 227 行附近，改为：
let symbol_index_enabled = optional_bool(&input, "symbol_index", false);
let (symbol_hits, symbol_status) = if symbol_index_enabled {
    let hits = lookup_symbol_hits(&context.workspace, &pattern_str);
    let status = crate::symbol_index::index_status(&context.workspace);
    (hits, status)
} else {
    (Vec::new(), String::new())
};
```

**步骤 C** — 仅输出与匹配文件相关的符号命中

当 `symbol_index` 开启时，过滤掉不在当前 grep 匹配文件列表中的条目：

```rust
let matched_files: HashSet<&str> = results.iter()
    .map(|r| r.file.as_str())
    .collect();
let filtered_hits: Vec<_> = symbol_hits.into_iter()
    .filter(|h| h.get("file")
        .and_then(|f| f.as_str())
        .is_some_and(|f| matched_files.contains(f)))
    .collect();
```

**效果**：默认 grep 输出清洁，无 `symbol_index_hits` 噪音。有需要时显式传 `symbol_index: true`。

---

### 改进 3：`agent_spawn` 支持 `step_timeout_ms` 参数

**影响文件**：
- `crates/tui/src/tools/subagent/mod.rs`（`SubAgentRuntime` 构造）
- `crates/tui/src/tools/subagent/blackboard.rs`（可能的参数传递路径）
- 工具定义层的 `agent_spawn` / `delegate_to_agent` input schema

**现状**（第 71-73 行）：
```rust
const STEP_API_TIMEOUT: Duration = Duration::from_secs(120);
```
此常量在 `SubAgentRuntime` 中使用，不可覆盖。

**方案**：

**步骤 A** — `SubAgentRuntime` 支持可配置超时

```rust
pub struct SubAgentRuntime {
    // ... 现有字段 ...
    step_timeout: Duration,  // 新增：可配置
}

impl SubAgentRuntime {
    pub fn new(...) -> Self {
        Self {
            // ...
            step_timeout: STEP_API_TIMEOUT,
        }
    }

    /// 设置每个 LLM 步骤的超时
    pub fn with_step_timeout(mut self, timeout: Duration) -> Self {
        self.step_timeout = timeout;
        self
    }
}
```

在子代理循环中将 `STEP_API_TIMEOUT` 替换为 `self.step_timeout`。

**步骤 B** — `agent_spawn` 工具暴露参数

在 `AgentSpawnTool::input_schema()` 和 `DelegateToAgentTool::input_schema()` 中增加：

```json
"step_timeout_ms": {
    "type": "integer",
    "description": "Per-step API timeout in milliseconds (default: 120000, max: 600000). Increase for review/audit workloads.",
    "minimum": 10000,
    "maximum": 600000
}
```

**步骤 C** — 在 `AgentSpawnTool::execute()` 中读取并传递

```rust
let step_timeout = optional_u64(&input, "step_timeout_ms", 120_000)
    .clamp(10_000, 600_000);
let runtime = runtime.with_step_timeout(Duration::from_millis(step_timeout));
```

**效果**：审查类子代理可传 `step_timeout_ms: 300000`（5 分钟），避免 4/5 超时。

---

### 改进 4：`read_file` 返回结构化的 `total_lines`

**影响文件**：`crates/tui/src/tools/file.rs`

**现状**：总行数仅在截断消息中以字符串形式出现（第 178-186 行）：
```
"... (第 1-2000 行，共 5332 行; 下一窗口设 ...)"
```
调用方（LLM）必须从自然语言中解析行数。

**方案**：在 `ToolResult` 的 `metadata` 中注入 `total_lines`。

`ReadFileTool::execute` 返回前：

```rust
if let Some(total) = total_lines_known {
    result = result.with_metadata(json!({
        "total_lines": total,
        "start_line": start_line,
        "end_line": start_line + collected.len() as u64 - 1,
    }));
}
```

**效果**：LLM 可直接从结构化 JSON 获取 `total_lines` 字段决定是否续读，无需解析截断字符串。

---

### 改进 5：子代理中间进度事件

**影响文件**：`crates/tui/src/tools/subagent/mod.rs`、`crates/tui/src/core/events.rs`

**现状**：子代理运行期间，父线程只能等待 `subagent.done` sentinel。无中间状态。

**方案**：子代理每完成一个工具调用后，通过已有的 `tx_event` 通道向父线程推送 `Event::SubAgentProgress`。

**步骤 A** — 新增事件类型（`events.rs`）

```rust
pub enum Event {
    // ... 现有变体 ...
    /// 子代理单步进度（工具调用完成或 API 调用完成）
    SubAgentProgress {
        agent_id: String,
        step: u32,
        tool_name: Option<String>,
        message: String,
    },
}
```

**步骤 B** — 子代理循环中发射（`subagent/mod.rs`）

在 `SubAgent` 的 turn 循环中，每次工具调用完成或 API 调用完成时：

```rust
if let Some(ref tx) = self.parent_event_tx {
    let _ = tx.send(Event::SubAgentProgress {
        agent_id: self.id.clone(),
        step: self.step_count,
        tool_name: Some(tool_name.to_string()),
        message: format!("completed {tool_name}"),
    }).await;
}
```

**效果**：父线程可实时获知子代理进度——每个文件审查完成时收到一次事件。超时不再是黑盒，父线程可判断「正在推进但慢」还是「卡在某个文件上」。

---

### 改进 6：`symbol_index_hits` 行号偏差自动修正提示

**影响文件**：`crates/tui/src/tools/search.rs`、`.deepseek/pick-rules.md`（项目规则）

**现状**：`symbol_index_note` 已提示 "Line numbers from syn spans; may drift for macro-expanded code"，但这条信息只出现在 grep 输出中。DS Pick 的 `pick-rules.md`（第 6b 节）虽已记载此规则，但模型未必始终遵循。

**方案**：当 `symbol_index` 开启且命中包含 `impl_fn` 类型时，额外附加提示：

```rust
if symbol_hits.iter().any(|h| h.get("symbol") == Some(&json!("impl_fn"))) {
    extra.insert("symbol_index_warning".into(), json!(
        "Some hits are 'impl_fn' — these come from #[derive] expansions and line \
         numbers may be off by 5-20 lines. Use read_file with a wider range."
    ));
}
```

**效果**：减少模型盲信偏移行号导致的无效 `read_file` 调用。

---

## 实施优先级

| 优先级 | 改进 | 代价 | 影响 |
|--------|------|------|------|
| **P0** | 改进 1：`file_search` 排除 | 5 行代码 | 消除审查中最直接的噪音源 |
| **P0** | 改进 2：`grep_files` 按需符号索引 | 20 行 | 每次 grep 输出缩减 30-50% |
| **P1** | 改进 3：`agent_spawn` 可配超时 | 30 行 + schema | 审查/审计类工作流不再超时 |
| **P1** | 改进 4：`read_file` 结构化行数 | 10 行 | 消除解析截断字符串的需求 |
| **P2** | 改进 5：子代理进度事件 | 40 行 + 事件定义 | 透明化子代理状态 |
| **P2** | 改进 6：`impl_fn` 行号警告 | 8 行 | 减少无效 `read_file` 调用 |

P0 两项合计约 25 行改动，可在一个 PR 内完成。

---

## 不做的事

- **不修改 shell 执行层**：`exec_shell` 在 Windows cmd.exe 下的行为是系统环境差异，不属于代码缺陷。改进方案超出了 DS Pick 项目范围。
- **不增加子代理并发数上限**：当前 10 个已足够（`[subagents].max_concurrent` 可配置，默认 10）。问题在于单个代理的超时，而非并发数。
