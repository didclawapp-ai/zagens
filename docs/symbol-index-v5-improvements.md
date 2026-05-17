# 符号索引 V5 改进方案

**日期**: 2026-05-17  
**基于**: V4 已实施版本（schema_version: 3，48个TS/TSX文件已索引，启动warmup生效）  
**原则**: 轻量、无新依赖、向后兼容，不搞重量型  
**前置阅读**: `symbol-index-v4-improvements.md`（V4实施记录）

---

## V4 已实施状态回顾

| V4项 | 内容 | 状态 |
|------|------|:----:|
| V4-1 | TypeScript/TSX 正则解析器（48个文件已索引） | ✅ |
| V4-2 | Rust trait 方法补漏（kind = "trait_fn"） | ✅ |
| V4-3 | 嵌套 mod 递归走完整深度 | ✅ |
| bugfix | ensure_symbol_index 移到 symbol_index_enabled 门之前 | ✅ |
| bugfix | warmup_if_needed 启动时后台构建（main.rs:753） | ✅ |

当前索引规模：schema_version 3，覆盖 ~271 个 Rust 源文件 + 48 个 TS/TSX 文件。

---

## V5 问题诊断

来源：V4实施会话（DeepSeek V4自查）+ 使用痛点分析

| # | 问题 | 影响 |
|---|------|------|
| H1 | CLI模式无warmup，冷启动首次grep卡住 | deepseek -p 场景体验差 |
| H2 | index_status() 每次grep全仓遍历mtime | ~271文件×每次grep×回合内多次调用，IO浪费 |
| H3 | kind_filter参数存在但没有外部入口 | 模型无法区分fn/interface/struct，查询噪音大 |
| H4 | grep命中后模型还需额外read_file确认上下文 | 多一次工具调用，多消耗token和回合数 |
| H5 | 函数调用关系缺失，只有flat符号列表 | 模型查函数时不知道依赖链，需要自己读函数体 |
| H6 | Rust符号和TS符号完全独立，Tauri bridge无关联 | 查一个命令名需要分两次查才能拿到两侧行号 |
| H7 | 符号变更无追踪，Implementer改动靠模型自述 | 改动摘要是模型叙述而非事实记录，有幻觉风险 |
| H8 | 命中结果无置信度，精确匹配和模糊匹配格式相同 | 模型无法判断是否需要read_file再确认 |

---

## V5 改动（8项，~175行）

按批次分三轮实施。

---

## 批次 I：即时收益（~40行）

### V5-1：CLI warmup（1行）

**问题**：`warmup_if_needed()` 只在 `serve --http` 分支调用（`main.rs:753`），`deepseek -p "..."` 和TUI交互模式冷启动时首次 `grep_files` 会等待后台构建完成。

**方案**：在 `main.rs` 非serve分支加一行，放在读取config之后、进入引擎之前：

```rust
// main.rs — CLI分支入口（具体行号在实施时read_file确认）
crate::symbol_index::warmup_if_needed(&workspace);
```

**改动量**：+1行  
**验收**：`deepseek -p "查一下loadWorkspaceFileIntoPreview"` → 首次grep立即返回命中，无等待

---

### V5-2：match_score 置信度字段（~10行）

**问题**：`symbol_index_hits` 返回的命中列表中，精确匹配和模糊匹配格式完全相同，模型无法区分可信度，保守起见每次都再去 `read_file` 确认。

**方案**：在 `SymbolHit` 输出结构加 `match_score` 字段：

```json
{
  "symbol_index_hits": [
    {
      "name": "loadWorkspaceFileIntoPreview",
      "file": "crates/desktop/web-ui/src/openWorkspaceFile.ts",
      "line": 28,
      "kind": "fn",
      "match_score": 1.0
    }
  ]
}
```

| 匹配类型 | score |
|---------|-------|
| 精确匹配（name完全一致） | 1.0 |
| 前缀匹配 | 0.8 |
| 子串/模糊匹配 | 0.5 |

在 `query_symbol_with_mode()` 的返回处注入score，序列化时带出。

**改动量**：+~10行（`SymbolHit` struct加字段 + `query_symbol_with_mode` 注入 + 序列化）  
**验收**：`grep_files("loadWorkspaceFileIntoPreview")` → hits中 `match_score: 1.0`；`grep_files("loadWork")` → hits中 `match_score: 0.8`

---

### V5-3：kind_filter 外部入口（~15行）

**问题**：`query_symbol_with_mode` 已有 `kind_filter: Option<&str>` 参数，但 `lookup_symbol_hits`（`search.rs:648`）调用时始终传 `None`，外部无法按符号类型过滤。

**方案**：在 `grep_files` 的JSON schema加可选参数 `symbol_kind`，透传给 `query_symbol_with_mode`：

```json
{
  "query": "PreviewState",
  "symbol_index": true,
  "symbol_kind": "interface"
}
```

支持的值：`"fn"` / `"struct"` / `"enum"` / `"interface"` / `"type"` / `"trait"` / `"trait_fn"` / `"impl_fn"` / `"class"` / `"method"`

`symbol_kind` 不传时行为与现在完全一致。

**改动量**：+~15行（schema解析 + lookup_symbol_hits透传 + pick-rules.md说明）  
**验收**：`grep_files("PreviewState", symbol_kind: "interface")` → 只返回interface类型命中，不返回同名fn/const

---

## 批次 II：查询质量（~65行）

### V5-4：mtime 指纹缓存（~30行）

**问题**：`index_status()` 每次调用都执行 `walk_source_files()` 遍历全仓（当前~319个源文件），每次grep触发两轮全量文件系统遍历。回合内3-4次grep = 数百次 `fs::metadata()` 调用。

**方案**：新增轻量指纹缓存文件 `.deepseek/.symbols_fingerprint`，记录最后一次重建时源文件列表的聚合哈希：

```
// .deepseek/.symbols_fingerprint 格式（纯文本，单行）
schema:3|count:319|hash:a3f7c921
```

`index_status()` 新逻辑：

```
1. symbols.json 不存在 → Missing（不变）
2. 读 .symbols_fingerprint，
   快速计算当前源文件列表哈希（只stat不读内容）→
   一致 → Fresh（跳过全量遍历）
   不一致 → Stale（触发重建）
3. fingerprint文件不存在 → 回退到现有全量mtime比较（兼容旧索引）
```

`build_index()` 完成时写入fingerprint文件。哈希算法用 `sha2`（已有依赖）对所有源文件路径+mtime拼接后hash。

**改动量**：`symbol_index.rs` +~30行（新增 `write_fingerprint()` + `read_fingerprint()` + 改造 `index_status()`）  
**验收**：连续两次 `grep_files` → 第二次不触发 `walk_source_files()`（可通过tracing日志验证）

---

### V5-5：grep命中附带文件符号摘要（~25行）

**问题**：`grep_files symbol_index: true` 命中后，模型通常还需要再调一次 `read_file` 才能看到目标函数的完整上下文。两次工具调用消耗双倍token。

**方案**：当 `symbol_index: true` 且命中文件数 ≤ 3 时，对每个命中文件附加该文件的符号摘要：

```json
{
  "symbol_index_hits": [...],
  "symbol_index_file_summaries": {
    "crates/desktop/web-ui/src/openWorkspaceFile.ts": {
      "symbols": [
        {"name": "normalizeWorkspaceRelPath", "kind": "fn", "line": 11},
        {"name": "loadWorkspaceFileIntoPreview", "kind": "fn", "line": 28}
      ]
    }
  }
}
```

模型看到摘要后可以直接用 `read_file(path, start_line=hit.line-5, limit=60)` 精准读取，跳过盲读整个文件的步骤。命中文件数 > 3 时不附加（避免响应体积膨胀）。

复用现有 `format_file_summary()` 逻辑（`symbol_index.rs:267`），不重复造轮子。

**改动量**：`search.rs` +~25行  
**验收**：`grep_files("loadWorkspaceFileIntoPreview", symbol_index: true)` → 响应含 `symbol_index_file_summaries`，摘要列出该文件所有符号

---

### V5-6：Tauri bridge 跨语言符号关联（~10行索引构建 + ~20行查询展示）

**问题**：Rust侧的 `#[tauri::command] fn read_workspace_binary_at_root` 和TS侧的 `invoke('read_workspace_binary_at_root')` 是同一个Tauri命令，但在索引里是完全独立的两条记录，模型需要分两次查。

这个问题对Auditor的`tauri-bridge.md`规则影响最直接——Auditor核查跨边界发现时，现在需要分两次查才能拿到两侧行号。

**方案**：索引构建完成后，做一次符号名匹配——把Rust侧 `kind = "fn"` 且有 `#[tauri::command]` 属性的符号，与TS侧 `invoke('xxx')` 调用的函数名做关联，写入 `bridge_pairs` 字段：

```json
{
  "bridge_pairs": [
    {
      "command": "read_workspace_binary_at_root",
      "rust": {"file": "crates/desktop/src/commands.rs", "line": 142},
      "ts":   {"file": "crates/desktop/web-ui/src/openWorkspaceFile.ts", "line": 47}
    }
  ]
}
```

查询时，搜一个命令名直接返回两侧行号。

**改动量**：
| 文件 | 改动量 |
|------|--------|
| `symbol_index.rs` | 新增 `build_bridge_pairs()` ~20行（构建时做名称匹配） |
| `symbol_index.rs` | `SymbolIndex` struct加 `bridge_pairs` 字段 +2行 |
| `search.rs` | 查询时检查bridge_pairs并附加到响应 +~8行 |

**验收**：`grep_files("read_workspace_binary_at_root", symbol_index: true)` → 响应含 `bridge_pairs` 命中，同时返回Rust侧和TS侧行号

---

## 批次 III：可追溯性（~70行）

### V5-7：轻量调用关系索引（~40行）

**问题**：索引只有flat符号列表，没有调用关系。模型查一个函数时，不知道它调用了哪些其他函数，只能自己读函数体分析。

**方案**：不做完整调用图（重量型），只做单向"这个函数体内出现了哪些已知符号名"——在构建索引时，对每个函数体的文本做一次符号名扫描。

```json
{
  "name": "loadWorkspaceFileIntoPreview",
  "kind": "fn",
  "line": 28,
  "calls": ["normalizeWorkspaceRelPath", "detectFileType", "isBinaryFileType", "invoke"]
}
```

实现方式：索引构建第二遍——第一遍建好符号列表后，第二遍对每个函数体文本（通过行号范围截取）扫描已知符号名的出现。

Rust侧用 `syn` 的 `ExprCall`/`ExprMethodCall` 节点，比正则更准确。TS侧用正则扫描已知符号名。

`calls` 只记录出现在当前索引里的符号名，不记录外部库调用（避免噪音）。

**改动量**：
| 文件 | 改动量 |
|------|--------|
| `symbol_index.rs` | `SymbolEntry` struct加 `calls: Vec<String>` 字段 +2行 |
| `symbol_index.rs` | Rust侧 `extract_calls_from_fn()` ~20行 |
| `symbol_index.rs` | TS侧 `extract_calls_from_ts_fn()` ~15行（正则扫描） |
| `symbol_index.rs` | `build_index()` 第二遍调用 +5行 |

**验收**：`grep_files("loadWorkspaceFileIntoPreview", symbol_index: true)` → hits中含 `calls: ["normalizeWorkspaceRelPath", "detectFileType", "isBinaryFileType"]`

---

### V5-8：符号变更追踪（~30行）

**问题**：索引每次重建只记录当前状态，没有变更历史。CRAFT链路中Implementer完成后，改动摘要依赖模型自述，有幻觉风险。

**方案**：重建时对比新旧索引，输出 `changes` 字段写入 `.deepseek/.symbols_changes.json`：

```json
{
  "rebuilt_at": "2026-05-17T10:30:00Z",
  "added":    ["NewComponent", "useNewHook"],
  "removed":  ["OldHelper"],
  "modified": ["loadWorkspaceFileIntoPreview"]
}
```

判断 `modified` 的依据：符号名相同但 `line` 变化超过2行，或 `calls` 列表有变化。

CRAFT的Implementer完成后，黑板写入时可以直接读这个文件，而不是让模型总结"我改了什么"——**事实记录替代模型叙述**，与Auditor的设计哲学一致。

`changes` 文件只保留最近一次重建的diff，不做历史堆积（轻量）。

**改动量**：
| 文件 | 改动量 |
|------|--------|
| `symbol_index.rs` | 新增 `diff_indexes()` ~20行 |
| `symbol_index.rs` | `build_index()` 完成时写入changes文件 +5行 |
| `blackboard.rs` | Implementer分区写入时读取changes文件 +5行 |

**验收**：修改一个TS函数后触发重建 → `.deepseek/.symbols_changes.json` 含该函数名在 `modified` 数组中

---

## 实施顺序总览

| 批次 | 项 | 改动量 | 收益 |
|:----:|-----|--------|------|
| **I** | V5-1 CLI warmup | 1行 | CLI冷启动不再卡首次grep |
| **I** | V5-2 match_score | ~10行 | 模型知道命中可信度，减少不必要read_file |
| **I** | V5-3 kind_filter入口 | ~15行 | 查询精度提升，过滤同名不同类型符号 |
| **II** | V5-4 mtime指纹缓存 | ~30行 | 消除每次grep的全仓遍历 |
| **II** | V5-5 grep附带文件摘要 | ~25行 | 减少一次read_file调用 |
| **II** | V5-6 Tauri bridge关联 | ~30行 | 跨语言查询一次命中两侧行号，强化Auditor核查 |
| **III** | V5-7 轻量调用关系 | ~40行 | 查函数时知道依赖链 |
| **III** | V5-8 符号变更追踪 | ~30行 | Implementer改动有事实记录，替代模型叙述 |
| **合计** | | **~181行** | |

---

## 对比：V4 vs V5

| 维度 | V4 | V5 |
|------|----|----|
| 核心关注 | 索引**覆盖什么**（TS支持、trait方法、嵌套mod） | 索引**怎么用**（精度、速度、关联、可追溯） |
| 主要改动层 | 解析引擎（新语言支持） | 查询层 + 构建后处理层 |
| 新增能力 | TS/TSX符号、trait_fn、深层mod | 调用关系、bridge关联、变更追踪、置信度 |
| 与CRAFT关系 | 独立 | V5-6直接强化Auditor的tauri-bridge核查；V5-8为blackboard提供事实改动记录 |

---

## 注意事项

**V5-7调用关系的边界**：`calls` 只记录出现在当前项目索引里的符号名，外部crate的函数调用（如 `tokio::spawn`、`serde_json::from_str`）不记录，避免噪音膨胀索引体积。

**V5-6 bridge关联的识别方式**：Rust侧通过 `#[tauri::command]` 属性识别，TS侧通过 `invoke('xxx')` 正则识别。命令名字符串匹配，大小写敏感。如果命令名用变量传递（`invoke(cmdName)`）则无法关联，属于已知限制。

**V5-8变更追踪的轻量边界**：只保留最近一次diff，不做历史堆积。如果需要多次迭代历史，那是Git的工作，不是索引的工作。
