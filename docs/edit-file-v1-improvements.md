# edit_file 改进方案 V1

**日期**: 2026-05-17
**基于**: V0 全部实施完毕 + V5 实施会话实际工具调用日志
**原则**: 向后兼容，不改现有参数语义，新增 `operation` 模式

---

## V0 回顾

V0 的 5 项改进（E1–E5）全部已实施，效果确认：

| # | 改进 | 状态 | 实测 |
|---|------|------|------|
| E1 | `\r\n` ↔ `\n` 换行符自适应 | ✅ | CRLF 文件多行编辑一次成功 |
| E2 | `start_line` / `end_line` 行范围限定 | ✅ | 精确定位，无误伤 |
| E3 | 诊断性错误信息（NOT_FOUND + HINT） | ✅ | 模型不再尝试 PowerShell 旁路 |
| E4 | count > 1 时 `[AMBIGUOUS]` 警告 + `replace_mode` | ✅ | 3 处命中 → 返回含行号的警告 |
| E5 | 成功响应含 unified diff + 命中行号 | ✅ | 模型无需额外 `read_file` 验证 |

这五项解决了**搜索替换本身的可靠性**。但 V0 没有触及一个更深层的问题：**不是所有编辑操作都适合用 search/replace 表达**。

---

## V1 问题诊断

回顾 V5 实施会话中跟 `edit_file` 相关的摩擦：

| 场景 | 操作本质 | 被迫使用的 search/replace 方式 | 结果 |
|------|---------|-------------------------------|------|
| V5-1：在 `main.rs:804` 后加一行 `warmup_if_needed` | **插入** | 搜 `"let workspace = resolve_workspace..."`，replace 为 `原行 + "\n    warmup..."` | 搜索字符串命中多行 → 误伤 |
| V5-7：给 `SymbolEntry` struct 加 `calls` 字段 | **插入** | 搜最后一个已有字段，replace 为 `原字段 + ",\n    calls: Vec<String>"` | 成功，但构造多行搜索字符串繁琐 |
| V5-7：struct 构造点补 `calls: vec![]` | **替换指定行** | 搜 `SymbolEntry {` 附近的唯一文本 | 每个构造点需要不同的搜索字符串 |

**根因**：`edit_file` 只有一个 operation —— search/replace。而常见的编辑意图有三种：

| 意图 | 自然表达 | 当前被迫表达 |
|------|---------|-------------|
| 在某行后插入新内容 | "在第 804 行后插入一行" | 搜第 804 行的文本，replace 为 自身+新行 |
| 删除若干行 | "删除第 30–35 行" | 搜那几行的完整文本，replace 为空字符串 |
| 替换指定行 | "第 3849 行改为 X" | 搜第 3849 行的文本，replace 为 X |

强制用 search/replace 来表达这些意图，就是 V5-1 中 6 次失败的深层原因 — 不是换行符问题（V0 E1 已解决），而是**搜索字符串本身就是构造出来的，不一定精确匹配**。

---

## 改进方案（4 项，~130 行）

### V1-1：新增 `operation` 参数 — 支持 4 种操作模式（P0）

**问题**：`edit_file` 只有 search/replace 一种操作。插入、删除、行替换都需要模型构造人工搜索字符串，增加出错概率。

**方案**：新增 `operation` 可选参数，默认值为 `"search_replace"`（保持向后兼容）。新增三种模式：

```json
"operation": {
    "type": "string",
    "enum": ["search_replace", "insert_after", "delete_lines", "replace_line"],
    "description": "操作模式。默认 'search_replace'（搜索替换）。其他模式不需要 search 参数。"
}
```

**模式语义**：

| operation | 所需参数 | 行为 |
|-----------|---------|------|
| `search_replace`（默认） | `search` + `replace` | 搜索并替换（V0 行为，不变）|
| `insert_after` | `after_line` + `text` | 在 `after_line` 行之后插入 `text` |
| `delete_lines` | `start_line` + `end_line` | 删除 `start_line` 到 `end_line` 行（含两端）|
| `replace_line` | `line` + `text` | 将 `line` 行的内容替换为 `text` |

**execute 分发逻辑**（伪代码）：

```rust
async fn execute(&self, input: Value, context: &ToolContext) -> Result<ToolResult, ToolError> {
    let operation = optional_str(&input, "operation").unwrap_or("search_replace");
    match operation {
        "search_replace" => execute_search_replace(input, context).await,
        "insert_after"   => execute_insert_after(input, context).await,
        "delete_lines"    => execute_delete_lines(input, context).await,
        "replace_line"    => execute_replace_line(input, context).await,
        other => Err(ToolError::invalid_input(format!("unknown operation: {other}"))),
    }
}
```

**`insert_after` 实现要点**：

```rust
async fn execute_insert_after(input: Value, context: &ToolContext) -> Result<ToolResult, ToolError> {
    let path_str = required_str(&input, "path")?;
    let text = required_str(&input, "text")?;
    let after_line = required_u64(&input, "after_line")? as usize;

    let file_path = context.resolve_path(path_str)?;
    let contents = fs::read_to_string(&file_path).map_err(...)?;
    let file_le = if contents.contains("\r\n") { "\r\n" } else { "\n" };

    let lines: Vec<&str> = contents.lines().collect();
    if after_line > lines.len() {
        return Err(ToolError::execution_failed(format!(
            "[OUT_OF_RANGE] after_line={after_line} 超过文件总行数 {} ({})",
            lines.len(), file_path.display()
        )));
    }

    let text_normalized = normalize_line_endings(text, file_le);
    let mut new_lines: Vec<String> = lines.iter().take(after_line)
        .map(|s| s.to_string()).collect();
    for t in text_normalized.lines() {
        new_lines.push(t.to_string());
    }
    for l in &lines[after_line..] {
        new_lines.push(l.to_string());
    }
    let updated = new_lines.join(file_le);

    fs::write(&file_path, &updated)...?;
    let diff = make_unified_diff(&file_path.display().to_string(), &contents, &updated);
    let summary = format!("inserted {} line(s) after line {after_line} in {}",
        text_normalized.lines().count(), file_path.display());
    // LSP diagnostics appended as in search_replace
}
```

**`delete_lines` 实现**：读取 → 按行切分 → 删除 `[start, end]` → 写回 → diff。

**`replace_line` 实现**：读取 → 按行切分 → 替换 `line` 行 → 写回 → diff。

**改动量**：file.rs ~100 行（execute 重构为分发器 + 3 个新函数 + schema 扩展）。  
**效果**：V5-1 的"在第 804 行后插入"变为 `operation: "insert_after", after_line: 804, text: "crate::symbol_index::warmup_if_needed(&workspace);"` — 无需构造搜索字符串，零误伤风险。  
**验收**：在 5000 行 CRLF 文件中用 `insert_after` 在指定行插入 → 一次成功，diff 正确，行号无偏移。

---

### V1-2：`insert_after` 支持文件边界（P1）

**问题**：`after_line` 为 0 时无法在文件**开头**插入；`after_line` = 文件总行数时无法在**末尾**追加（当前拒绝）。

**方案**：扩展 `after_line` 语义：

| after_line | 行为 |
|-----------|------|
| `0` | 在文件最开始（第 1 行之前）插入 |
| `1..=N` | 在第 N 行之后插入 |
| `N`（= 文件总行数）| 在文件末尾追加 |

实现中 `lines.iter().take(0)` 是空迭代器 → 空行集合 → `text` 成为文件开头，逻辑自然成立。末尾追加同理。

**改动量**：file.rs ~5 行（移除 `after_line > lines.len()` 的错误检查，改为 `after_line > lines.len()` 才报错；`after_line == 0` 无需特殊处理）。  
**验收**：空文件 `after_line: 0` → 正常插入；5000 行文件 `after_line: 5000` → 末尾追加。

---

### V1-3：search_replace 失败时建议替代操作（P1）

**问题**：当 search_replace 的 `search` 未找到时（V0 E3 的 `[NOT_FOUND]`），模型不知道可以用 `replace_line` 绕过，于是进入盲猜循环。

**方案**：在 `[NOT_FOUND]` 错误末尾附加操作建议：

```rust
let alt = if start_line > 0 {
    format!("\n💡 如果知道确切行号，可直接使用 operation: \"replace_line\" 并指定 line: <行号>，无需 search 字符串。")
} else {
    String::new()
};
return Err(ToolError::execution_failed(format!(
    "[NOT_FOUND] search string not found in {}. {hint}{alt}",
    file_path.display()
)));
```

**改动量**：file.rs ~5 行。  
**效果**：模型收到 `[NOT_FOUND]` 后，如果知道行号（多数情况都知道 — 从 `read_file` / `grep_files` / `symbol_index` 得到），可以直接改用 `replace_line`，不回退重试 search。

---

### V1-4：紧凑变更格式（P2）

**问题**：当前 `make_unified_diff` 使用 `similar::TextDiff` 的 unified diff —— 3 行上下文 + `--- a/` / `+++ b/` 标头 + `@@` 区块标头。对 1–2 行改动，元数据占比 >50%，模型需要扫描标头才能找到实际改动。

**方案**：在 diff 下方附加紧凑行：`  - 旧行内容\n  + 新行内容`。只在 `search` 和 `replace` 总行数 ≤ 5 时附加。

```
// 辅助函数
fn make_compact_change(old: &str, new: &str) -> String {
    let mut out = String::new();
    for line in old.lines() { out.push_str(&format!("  - {line}\n")); }
    for line in new.lines() { out.push_str(&format!("  + {line}\n")); }
    out
}
```

**改动量**：file.rs ~20 行（新辅助函数 + execute 末尾条件追加）。  
**效果**：小改动时 diff 下方直接看到 `  - old\n  + new`，无需解析 unified diff 标头。  
**验收**：替换 2 行 → 返回正文含 `  - old line\n  + new line`。

---

## 实施顺序

| 批次 | 项 | 改动量 | 收益 |
|:----:|-----|--------|------|
| **A** | V1-1（4 种 operation 模式） | ~100 行 | 插入/删除/替换行不再需要构造搜索字符串 — 消除 V5-1 类问题的根源 |
| **B** | V1-2（文件边界）+ V1-3（失败建议替代） | ~10 行 | 覆盖边缘 case；搜索失败时引导模型用行号操作 |
| **C** | V1-4（紧凑 diff） | ~20 行 | 小改动时减少 diff 阅读开销 |

**总改动量**：~130 行，全部在 `file.rs` 的 `EditFileTool` 实现内，不影响其他工具。

---

## V0 → V1 演进路线

| 版本 | 解决什么 | 核心方法 |
|------|---------|---------|
| V0 | 搜索替换的**可靠性** | 换行符自适应、行范围限定、多匹配警告、诊断错误、unified diff |
| V1 | 编辑意图的**表达力** | 新增 `insert_after` / `delete_lines` / `replace_line` 三种行操作模式 |

V0 保证了 search/replace **不出错**。V1 让模型在**不需要 search/replace** 时，不被迫用 search/replace — 这才是 V5-1 类问题的根本解决。

---

## 向后兼容

- `operation` 可选，默认 `"search_replace"`
- 不传 `operation` 时行为与 V0 完全一致
- 现有 `search` / `replace` / `start_line` / `end_line` / `replace_mode` 参数语义不变
- 现有测试不需要修改

---

## 测试用例

```rust
#[tokio::test]
async fn edit_file_insert_after_mid_file() { /* 3 行文件第 2 行后插入 → 4 行 */ }

#[tokio::test]
async fn edit_file_insert_after_beginning() { /* after_line: 0 → 文件开头插入 */ }

#[tokio::test]
async fn edit_file_insert_after_end() { /* after_line = N → 末尾追加 */ }

#[tokio::test]
async fn edit_file_insert_after_out_of_range() { /* after_line > N → [OUT_OF_RANGE] */ }

#[tokio::test]
async fn edit_file_delete_lines_range() { /* 删除第 2–3 行 → 只剩第 1 行 */ }

#[tokio::test]
async fn edit_file_delete_lines_single() { /* start == end → 删 1 行 */ }

#[tokio::test]
async fn edit_file_replace_line() { /* 替换第 2 行 → 内容变化 */ }

#[tokio::test]
async fn edit_file_unknown_operation() { /* operation: "magic" → error */ }

#[tokio::test]
async fn edit_file_backward_compat_no_operation() { /* 不传 operation → search_replace 路径 */ }
```
