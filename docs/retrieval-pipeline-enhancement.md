# 检索管道增强方案：减少模型幻觉的结构化改进

> **目标**：在不变更 LLM 模型本身的前提下，提高 `grep_files` / `read_file` 等工具返回信息的精准度，减少信息检索阶段的噪音和漏检，从而降低模型幻觉密度。
>
> **原则**：不改工具接口、不引入外部 API 依赖、不改 prompt、不依赖 GPU / ONNX / 嵌入模型。所有增强落在工具**后端检索逻辑**内。

---

## 1. 问题诊断

### 1.1 当前检索管道的三个瓶颈

```
用户问题
  ↓
grep_files（纯正则匹配）→ 匹配行无相关性排序 → 几百个结果中模型自行挑
  ↓
read_file（全文读取或大范围读取）→ 上下文窗口快速膨胀
  ↓
后续推理依赖之前的 read_file 记忆 → 信息衰减 → 幻觉
```

| 瓶颈 | 表现 | 根因 |
|------|------|------|
| A. 符号定位靠正则 | 问"`StructuredVerdict` 在哪定义的"→ 264 个文件中搜正则 | 没有符号→文件→行号的索引 |
| B. 搜索结果未排序 | `grep_files("task_id")` 返回 100 个结果，相关定义在第 40 个 | 纯文件系统遍历顺序，无相关性评分 |
| C. 大文件全量读取 | `mod.rs` 4221 行只读前 200 行，后半部分的定义靠猜 | 没有文件结构摘要，不知道"跳过前 3900 行后才有目标定义" |

### 1.2 与 Cursor 的差距本质

Cursor 的嵌入索引（embeddings-based semantic retrieval）解决的是瓶颈 A 和 B——用语义向量找到相关片段，天然排除了词法相同但语义无关的匹配。本方案在**不引入嵌入模型**的前提下，用"符号索引 + BM25 排序 + 文件结构摘要"三条轻量措施，在相同的约束内达到近似效果。

---

## 2. 三阶段方案

| 阶段 | 内容 | 收益 | 成本 |
|------|------|------|------|
| **P1** | 符号索引 — `syn` 提取所有 `pub` 符号 → `symbols.json` | 消除"定义在哪"类幻觉 | ~200 行 |
| **P2** | BM25 排序 — `grep_files` 内部相关性重新排序 | 减少无关文件读取 | ~60 行 |
| **P3** | 结构摘要 — 大文件预先提取公开类型/函数签名 | 减少大文件全文读取 | ~150 行 |

---

## 3. P1：符号索引（`symbols.json`）

### 3.1 目标

启动时扫描 workspace 的所有 `.rs` 文件，提取 `pub fn`、`pub struct`、`pub enum`、`pub trait`、`pub mod` 的签名和行号，写入 `.deepseek/symbols.json`。模型调用 `grep_files` 之前，可以先查索引精确跳转。

### 3.2 索引格式

```json
{
  "schema_version": 1,
  "generated_at": "2026-05-15T10:00:00Z",
  "files": {
    "crates/tui/src/tools/subagent/mod.rs": {
      "symbols": [
        {"kind": "struct", "name": "StructuredVerdict", "line": 4146},
        {"kind": "enum",   "name": "VerdictLevel",     "line": 4160},
        {"kind": "fn",     "name": "parse_structured_verdict", "line": 4189},
        {"kind": "struct", "name": "SubAgentResult",    "line": 409}
      ]
    },
    "crates/tui/src/runtime_api.rs": {
      "symbols": [
        {"kind": "struct", "name": "RuntimeApiState", "line": 54},
        {"kind": "fn",     "name": "build_router",    "line": 419},
        {"kind": "fn",     "name": "get_task",        "line": 2041}
      ]
    }
  }
}
```

### 3.3 实现方案

**依赖**：`syn` crate（Rust 语法解析，纯 Rust，无 C 依赖）。加入 `crates/tui/Cargo.toml`：

```toml
syn = { version = "2", features = ["full", "parsing"] }
```

**代码位置**：`crates/tui/src/symbol_index.rs`（新文件，~150 行）

```rust
//! Build and query a symbol index for the workspace.
//!
//! The index is a JSON file at `.deepseek/symbols.json` mapping
//! file paths → list of (kind, name, line).
//!
//! Rebuilt on every `serve --http` start (incremental rebuild with
//! mtime checks is a future optimization). Missing/unparseable
//! files are skipped silently — the index is best-effort.

use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SymbolEntry {
    pub kind: String,    // "struct" | "enum" | "fn" | "trait" | "mod"
    pub name: String,
    pub line: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileSymbols {
    pub symbols: Vec<SymbolEntry>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SymbolIndex {
    pub schema_version: u32,
    pub generated_at: String,
    pub files: BTreeMap<String, FileSymbols>,
}

/// Build the index by walking the workspace and parsing every `.rs` file
/// with `syn`. Skipped files (parse errors, non-UTF-8) are omitted without
/// error — the index is best-effort.
pub fn build_index(workspace: &Path) -> SymbolIndex {
    let mut files = BTreeMap::new();

    for entry in walk_rs_files(workspace) {
        let rel = entry.strip_prefix(workspace).unwrap_or(&entry);
        let rel_str = rel.to_string_lossy().replace('\\', "/");

        if let Some(symbols) = extract_symbols(&entry) {
            if !symbols.is_empty() {
                files.insert(rel_str, FileSymbols { symbols });
            }
        }
    }

    SymbolIndex {
        schema_version: 1,
        generated_at: chrono::Utc::now().to_rfc3339(),
        files,
    }
}

/// Query the index for a symbol name (case-insensitive prefix match).
/// Returns `(file_path, line_number)` pairs sorted by exact-match priority.
pub fn query_symbol(index: &SymbolIndex, name: &str) -> Vec<(&str, usize)> {
    let name_lower = name.to_lowercase();
    let mut results: Vec<(&str, usize, bool)> = Vec::new();

    for (file, file_syms) in &index.files {
        for sym in &file_syms.symbols {
            let sym_lower = sym.name.to_lowercase();
            if sym_lower == name_lower {
                results.push((file.as_str(), sym.line, true));
            } else if sym_lower.contains(&name_lower) {
                results.push((file.as_str(), sym.line, false));
            }
        }
    }

    results.sort_by(|a, b| b.2.cmp(&a.2)); // exact matches first
    results.into_iter().map(|(f, l, _)| (f, l)).collect()
}

fn walk_rs_files(root: &Path) -> Vec<PathBuf> {
    let mut files = Vec::new();
    walk_rs_files_impl(root, &mut files);
    files
}

fn walk_rs_files_impl(dir: &Path, out: &mut Vec<PathBuf>) {
    if dir.file_name().map_or(false, |n| n == "target" || n == "node_modules") {
        return;
    }
    if let Ok(entries) = std::fs::read_dir(dir) {
        for entry in entries.flatten() {
            let p = entry.path();
            if p.is_dir() {
                walk_rs_files_impl(&p, out);
            } else if p.extension().map_or(false, |e| e == "rs") {
                out.push(p);
            }
        }
    }
}

fn extract_symbols(path: &Path) -> Option<Vec<SymbolEntry>> {
    let src = std::fs::read_to_string(path).ok()?;
    let file = syn::parse_file(&src).ok()?;
    let mut symbols = Vec::new();

    for item in &file.items {
        match item {
            syn::Item::Fn(f) => {
                symbols.push(SymbolEntry {
                    kind: "fn".into(),
                    name: f.sig.ident.to_string(),
                    line: line_of(&src, f.sig.ident.span()),
                });
            }
            syn::Item::Struct(s) => {
                symbols.push(SymbolEntry {
                    kind: "struct".into(),
                    name: s.ident.to_string(),
                    line: line_of(&src, s.ident.span()),
                });
            }
            syn::Item::Enum(e) => {
                symbols.push(SymbolEntry {
                    kind: "enum".into(),
                    name: e.ident.to_string(),
                    line: line_of(&src, e.ident.span()),
                });
            }
            syn::Item::Trait(t) => {
                symbols.push(SymbolEntry {
                    kind: "trait".into(),
                    name: t.ident.to_string(),
                    line: line_of(&src, t.ident.span()),
                });
            }
            syn::Item::Mod(m) => {
                symbols.push(SymbolEntry {
                    kind: "mod".into(),
                    name: m.ident.to_string(),
                    line: line_of(&src, m.ident.span()),
                });
            }
            _ => {}
        }
    }

    Some(symbols)
}

/// Convert a proc_macro2 Span into a 1-based line number.
fn line_of(src: &str, span: proc_macro2::Span) -> usize {
    let byte_offset = span.start().line - 1; // proc_macro2 lines are 1-based
    // Fallback: count newlines up to the span's byte position.
    // For simplicity, use the span's line directly (requires
    // `syn` with `extra-traits` feature for .start().line).
    // Minimal implementation: count from source.
    src[..span.start().byte].chars().filter(|&c| c == '\n').count() + 1
}
```

> **行号精确度说明**：`proc_macro2::Span::start().line` 需要 `syn` 的 `extra-traits` feature 才能稳定工作。当前草稿用 `byte` 偏移 + 换行计数兜底，确保不依赖不稳定 API。

### 3.4 集成点

**启动时构建**（`crates/tui/src/main.rs` 或 `serve --http` 入口）：

```rust
// After workspace is determined:
let index = symbol_index::build_index(&workspace);
let index_path = workspace.join(".deepseek").join("symbols.json");
if let Some(parent) = index_path.parent() {
    let _ = std::fs::create_dir_all(parent);
}
let _ = std::fs::write(&index_path, serde_json::to_string_pretty(&index).unwrap_or_default());
```

**运行时查询**：新增工具 `symbol_search`（或 `grep_files` 内集成索引优先查询）。优先方案：**不新增工具**——在 `grep_files` 的返回结果中，当检测到查询词匹配索引中的精确符号名时，在结果头部追加 `## Symbol Index` 引用块：

```
## Symbol Index
- `StructuredVerdict` (struct) → crates/tui/src/tools/subagent/mod.rs:4146
- `VerdictLevel` (enum) → crates/tui/src/tools/subagent/mod.rs:4160

## Grep Results
...
```

### 3.5 工作区感知

与 Issue 0 同理——`build_index(workspace)` 用显式 workspace 路径，不依赖 `current_dir()`。Zagens 多工作区场景下，每个 Composer 工作区有独立的 `symbols.json`。切换工作区时重建索引（或 Tauri 命令传入 `workspaceRoot`）。

### 3.6 改动量

| 文件 | 行数 |
|------|------|
| `crates/tui/Cargo.toml` | +1（`syn` dependency） |
| `crates/tui/src/symbol_index.rs` | ~160 |
| `crates/tui/src/main.rs`（启动构建） | ~10 |
| `crates/tui/src/tools/search.rs`（集成优先查询） | ~25 |
| **总计** | **~200** |

---

## 4. P2：BM25 搜索结果排序

### 4.1 目标

替换 `grep_files` 中"文件系统遍历顺序 → 截断 100 条"的结果排列，改为 **BM25 相关性评分排序**。使模型最先看到最相关的匹配，减少因"相关结果在第 40 位"导致的不必要 `read_file`。

### 4.2 算法简述

BM25 是一个纯词频统计的排序算法——不需要模型、不需要 GPU、不需要外部依赖。公式：

```
BM25(D, Q) = Σ IDF(qᵢ) · (f(qᵢ,D) · (k₁+1)) / (f(qᵢ,D) + k₁·(1-b + b·|D|/avgdl))

其中：
- f(qᵢ,D) = 词 qᵢ 在文档 D 中的词频
- |D| = 文档长度（行数或字节数）
- avgdl = 所有文档的平均长度
- IDF(qᵢ) = log((N - n(qᵢ) + 0.5) / (n(qᵢ) + 0.5) + 1)
- k₁ = 1.2, b = 0.75（标准参数）
```

对于 `grep_files`：查询词从正则 pattern 中提取（去掉正则元字符后分词），文档 = 每个匹配文件的行集合，BM25 分数 = 匹配行的 BM25 分数加总。

### 4.3 实现方案

**代码位置**：`crates/tui/src/tools/search.rs` 内新增一个私有模块 `bm25`（~60 行）。

```rust
// ── BM25 scoring for grep_files result ranking ──────────────

/// Extract query terms from a regex pattern by stripping regex
/// metacharacters and splitting on word boundaries.
fn extract_query_terms(pattern: &str) -> Vec<String> {
    let cleaned = pattern
        .replace(['.', '*', '+', '?', '(', ')', '[', ']', '{', '}', '^', '$', '|', '\\'], " ");
    cleaned
        .split_whitespace()
        .map(|s| s.to_lowercase())
        .filter(|s| s.len() >= 2)  // skip single-char noise
        .collect()
}

/// BM25 score for a single document (file) against query terms.
struct Bm25Scorer {
    /// IDF per term: term → idf value
    idf: std::collections::HashMap<String, f64>,
    avgdl: f64,
    k1: f64,
    b: f64,
}

impl Bm25Scorer {
    fn new<'a>(
        docs: impl Iterator<Item = (&'a str, usize)>, // (file_path, line_count)
        terms: &[String],
    ) -> Self {
        let docs: Vec<_> = docs.collect();
        let n = docs.len() as f64;
        let avgdl = if n > 0.0 {
            docs.iter().map(|(_, len)| *len as f64).sum::<f64>() / n
        } else {
            1.0
        };

        let mut idf = std::collections::HashMap::new();
        for term in terms {
            let doc_count = docs.iter()
                .filter(|(path, _)| file_contains_term(path, term))
                .count() as f64;
            let idf_val = ((n - doc_count + 0.5) / (doc_count + 0.5) + 1.0).ln();
            idf.insert(term.clone(), idf_val);
        }

        Self { idf, avgdl, k1: 1.2, b: 0.75 }
    }

    fn score(&self, file_path: &str, term_matches: &[(String, usize)]) -> f64 {
        let doc_len = file_line_count(file_path) as f64;
        let mut total = 0.0;
        for (term, count) in term_matches {
            if let Some(&idf) = self.idf.get(term) {
                let tf = *count as f64;
                let numerator = tf * (self.k1 + 1.0);
                let denominator = tf + self.k1 * (1.0 - self.b + self.b * doc_len / self.avgdl);
                total += idf * numerator / denominator;
            }
        }
        total
    }
}
```

### 4.4 集成点

`grep_files` 的 `execute()` 方法中，现有逻辑是：

1. Regex 匹配 → 收集 `Vec<GrepMatch>`
2. 按文件分组 → 文件系统遍历序
3. 截断到 `max_results`

改动：在步骤 2 和 3 之间插入 BM25 重排序：

```rust
// After collecting matches, before truncation:
let terms = extract_query_terms(&pattern_str);
if !terms.is_empty() {
    let scorer = Bm25Scorer::new(
        matches.iter().map(|m| (m.file.as_str(), file_line_count(&full_path))),
        &terms,
    );
    // Group matches by file, compute BM25 score per file
    let mut file_scores: Vec<(&String, f64)> = file_matches.keys()
        .map(|file| {
            let file_terms: Vec<(String, usize)> = /* count term occurrences in this file's matches */;
            (file, scorer.score(file, &file_terms))
        })
        .collect();
    file_scores.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
    // Reorder results by file score
}
```

### 4.5 改动量

| 文件 | 行数 |
|------|------|
| `crates/tui/src/tools/search.rs` | +60 |
| **总计** | **~60** |

---

## 5. P3：大文件结构摘要

### 5.1 目标

对于 >500 行的文件，在 `read_file` 首次读取时附加一个**结构摘要头**（列出文件中定义的所有公开类型/函数/impl 块 + 行号）。模型可以从摘要中得知"目标定义在第 3900-4100 行"，然后用 `start_line` / `limit` 精确跳转，而非全文读取。

### 5.2 摘要格式

在 `read_file` 返回内容的头部插入：

```
## File Summary (crates/tui/src/tools/subagent/mod.rs)
| Line    | Kind   | Name                        |
|---------|--------|-----------------------------|
| 409     | struct | SubAgentResult              |
| 433     | field  | structured_verdict          |
| 1101    | fn     | build_allowed_tools         |
| 3725    | struct | SubAgentToolRegistry        |
| 4146    | struct | StructuredVerdict           |
| 4189    | fn     | parse_structured_verdict    |
```

### 5.3 实现方案

**方案**：P1 的 `symbol_index.rs` 已构建了完整的文件 → 符号映射。P3 **直接复用该索引**：

```rust
/// Format a markdown summary table for a file from the symbol index.
/// Returns `None` when the file has <500 lines or isn't in the index.
pub fn format_file_summary(index: &SymbolIndex, file_path: &str, total_lines: usize) -> Option<String> {
    if total_lines < 500 {
        return None; // small files don't need summaries
    }
    let file_syms = index.files.get(file_path)?;
    if file_syms.symbols.is_empty() {
        return None;
    }

    let mut table = format!(
        "## File Summary ({})\n| Line | Kind | Name |\n|------|------|------|\n",
        file_path
    );
    for sym in &file_syms.symbols {
        table.push_str(&format!("| {} | {} | `{}` |\n", sym.line, sym.kind, sym.name));
    }
    Some(table)
}
```

`read_file` 的 `execute()` 方法中，在打开文件后、读取内容前，查索引：

```rust
// In ReadFileTool::execute, after resolving total_lines:
if let Ok(index_raw) = std::fs::read_to_string(workspace.join(".deepseek/symbols.json")) {
    if let Ok(index) = serde_json::from_str::<SymbolIndex>(&index_raw) {
        if let Some(summary) = symbol_index::format_file_summary(&index, &rel_path, total_lines) {
            // Prepend summary to content
            content = format!("{summary}\n\n---\n\n{content}");
        }
    }
}
```

### 5.4 改动量

| 文件 | 行数 |
|------|------|
| `crates/tui/src/symbol_index.rs` | +25（`format_file_summary`） |
| `crates/tui/src/tools/file.rs` | +15（`read_file` 集成） |
| **总计** | **~40**（不包括 P1 的 ~200 行基础） |

---

## 6. 实施顺序与依赖

```
P1: 符号索引 ──── 所有后续阶段的基础
  ├─ P3: 结构摘要（直接复用 P1 索引）
  └─ P2: BM25 排序（独立，可与 P3 并行）

总改动量: ~300 行 Rust + 1 个新 crate 依赖（syn）
```

| 阶段 | 一周内可完成 | 需配合 |
|------|-------------|--------|
| P1 | ✅ | `cargo check` 通过 + 索引文件格式验证 |
| P2 | ✅ | 手工对比重排序前后的 `grep_files` 结果 |
| P3 | ✅ | 依赖 P1 先构建索引 |

---

## 7. 验收标准

### P1 验收

```bash
cargo run -- serve --http
# 启动后：
cat .deepseek/symbols.json | jq '.files["crates/tui/src/tools/subagent/mod.rs"].symbols[] | select(.name == "StructuredVerdict")'
# → {"kind": "struct", "name": "StructuredVerdict", "line": 4146}
```

### P2 验收

```bash
# 在模拟工具调用中：
grep_files(pattern="agent_spawn", include=["*.rs"])
# 结果前 3 条包含 crates/tui/src/tools/subagent/mod.rs 的 fn agent_spawn 签名行
# （而非 config.rs 或 runtime_api.rs 中的调用点排在前面）
```

### P3 验收

```bash
read_file(path="crates/tui/src/tools/subagent/mod.rs")
# 返回内容头部包含 ## File Summary 表格，列出所有公开符号
# 正文从指定 start_line 开始，摘要不重复
```

---

## 8. 不做的和为什么

| 不做的事 | 理由 |
|----------|------|
| 嵌入模型 / 向量数据库 | 引入 Python/ONNX 依赖，破坏"纯 Rust runtime"约束 |
| 新增 `symbol_search` 工具 | 维持现有工具接口不变，降低 prompt 适配成本 |
| 增量索引更新（mtime watch） | 启动时全量构建 <2 秒（280 个 .rs 文件），增量收益不大 |
| TypeScript 符号索引 | 本项目 .ts/.tsx 文件仅 ~50 个，手动 grep 可接受；可后续加 `swc` 解析 |

---

**关联文档**：[agent-reliability-craft-plan.md](agent-reliability-craft-plan.md) · [craft-implementation-issues.md](craft-implementation-issues.md)
