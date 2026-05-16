# 符号索引 V3 改进方案

**基于**: `symbol_index.rs` (780 行) + `search.rs` + `pick-rules.md` §6  
**日期**: 2026-05-15  
**原则**: 每项独立可验证，改动量可控，不与现有管道冲突  
**前置阅读**: [`symbol-index-v2-improvements.md`](symbol-index-v2-improvements.md)（V2 方案及 A+B+C 批次实施记录）

---

## V2 已实施状态回顾

V2 的 A+B+C 批次中，除 C2（并行解析）外**均已落地**：

| V2 项 | 内容 | 状态 | 代码位置 |
|--------|------|:--:|------|
| **A1** | 排除 `target/`/`node_modules/`/`dist/` | ✅ | `symbol_index.rs` `walk_rs_files` |
| **A2** | prompt 引导模型使用索引 | ✅ | `pick-rules.md` §6 |
| **A3** | `symbol_index_status` 状态标记 | ✅ | `search.rs:232` + `symbol_index.rs:68` |
| **B1** | 查询语义（整词/前缀/精确/kind 过滤） | ✅ | `symbol_index.rs` `query_symbol_with_mode` |
| **B2** | 增量重建（mtime 对比） | ✅ | `symbol_index.rs` `build_index` |
| **B3** | 扩展符号种类（type/const/static/macro/impl_fn） | ✅ | `symbol_index.rs` `SymbolEntry.kind` |
| **C1** | 非 pub 符号（`SymbolVisibility::All`） | ✅ | `symbol_index.rs:42` |
| **C2** | 并行解析（rayon） | ❌ 未实施 | — |

当前索引规模：475KB，覆盖 325 个 `.rs` 文件，`grep_files` 返回结构含 `symbol_index_hits` + `symbol_index_status`。

---

## V3 新增方向（5 项）

这些方向来自 V2 实施后发现的**实际使用中的局限**，按性价比排序。

---

### V3-1：行号可靠性文档化

#### 问题

`syn::parse_file` 的 span 行号在以下场景可能与 IDE/编辑器行号不一致：

- **宏展开后的代码** — `syn` 解析的是展开后的 token 流，行号可能对不上源文件
- **`include!()` / `include_str!()` 宏** — 文件边界模糊
- **`#[derive(...)]` 生成的 impl** — `syn` 会走 `ImplItemFn` 但不一定有准确的 span

当前索引对此完全静默，模型拿着行号去 `read_file` 可能扑空。

#### 方案

在两处加说明：

**1. `pick-rules.md` §6 末尾** — 加局限说明：

```
Symbol index line numbers come from `syn` span data. They are accurate
for hand-written `fn`/`struct`/`enum` definitions but may drift for:
- macro-expanded code (`#[derive]` impls, `macro_rules!` invocations)
- `include!()`-ed files
When a symbol_index_hits line doesn't match, fall back to `grep_files`
and read the surrounding context.
```

**2. `grep_files` 返回** — 当 hits 非空时附加 `symbol_index_note` 字段：

```json
{
  "symbol_index_hits": [...],
  "symbol_index_status": "fresh",
  "symbol_index_note": "Line numbers from syn spans; may drift for macro-expanded code."
}
```

#### 改动

| 文件 | 改动 |
|------|------|
| `.deepseek/pick-rules.md` | +8 行（§6 末尾加局限说明） |
| `crates/tui/src/tools/search.rs` | +2 行（hits 非空时注入 `symbol_index_note`） |

#### 验收

- `grep_files("StructuredVerdict")` → 返回中带 `symbol_index_note`
- 模型读 `symbol_index_hits` 时不会盲信行号

---

### V3-2：symbol_index_hits 与 grep 结果融合

#### 问题

当前 `grep_files` 返回两个独立数组：

```json
{
  "matches": [{"file": "mod.rs", "line": 4146, ...}, ...],
  "symbol_index_hits": [{"symbol": "StructuredVerdict", "file": "mod.rs", "line": 4146}]
}
```

当两者指向同一 `(file, line)` 时：JSON 体积膨胀，模型可能困惑"两个 4146 行是同一件事吗"，且正则 hit 和索引 hit 无法互相强化信号。

#### 方案

在 BM25 排序后加权重提升步骤——索引命中的行对应的正则结果自动排到最前：

```rust
fn boost_index_hits(
    results: &mut Vec<GrepMatch>,
    symbol_hits: &[serde_json::Value],
) {
    let hit_set: HashSet<(String, usize)> = symbol_hits
        .iter()
        .filter_map(|h| {
            let file = h.get("file")?.as_str()?;
            let line = h.get("line")?.as_u64()? as usize;
            Some((file.to_string(), line))
        })
        .collect();

    results.sort_by_key(|m| {
        let key = (m.file.clone(), m.line_number);
        if hit_set.contains(&key) { 0u8 } else { 1u8 }
    });
}
```

不改输出结构——仅影响 `matches` 数组的内部顺序。

#### 改动

| 文件 | 位置 | 改动 |
|------|------|------|
| `crates/tui/src/tools/search.rs` | `bm25_rank` 调用后 | +~25 行 `boost_index_hits` 函数 + 调用 |

#### 验收

- 搜 `StructuredVerdict` → 定义行排在最前（权重融合生效）
- 同一 `(file, line)` 在 matches 和 symbol_index_hits 中重复出现时，matches 中的那一条被提升到顶部

---

### V3-3：read_file 索引引导（默认行窗口）

#### 问题

模型从 `grep_files` 拿到 `symbol_index_hits: [{file: "mod.rs", line: 4146}]` 后，仍然可能：

- 从第 1 行开始读 → 浪费 token
- 在 4000+ 行文件中找不到目标区域
- 读错行号（手工加减 offset 出错）

#### 方案

在 `pick-rules.md` §6 加操作指引：

```
When symbol_index_hits gives you file:line of a definition:
- read_file(path, start_line = hit.line - 5, limit = 60)
  to see the definition with surrounding context.
- Do NOT read from line 1 unless you need module-level context.
```

纯 prompt 改动——不涉及 Rust 代码。

#### 改动

| 文件 | 改动 |
|------|------|
| `.deepseek/pick-rules.md` | +5 行（§6 加操作指引） |

#### 验收

- 模型在拿到 `symbol_index_hits` 后，`read_file` 调用带有目标行附近的 `start_line`，而非从第 1 行开始

---

### V3-4：人类可读前缀块

#### 问题

`symbol_index_hits` 以 JSON 数组形式出现在 `grep_files` 返回中。部分模型（尤其上下文已膨胀到衰减区时）只读工具返回的开头文本，跳过后面的 JSON 字段，`symbol_index_hits` 可能被完全忽略。

#### 方案

在 `grep_files` 返回 JSON 中，`symbol_index_hits` 非空时在顶层加一个人类可读的摘要字符串：

```json
{
  "matches": [...],
  "symbol_index_hits": [...],
  "symbol_index_summary": "Symbol index: StructuredVerdict -> crates/tui/src/tools/subagent/mod.rs:4146, parse_structured_verdict -> crates/tui/src/tools/subagent/mod.rs:4189"
}
```

格式：`Symbol index: {name} -> {file}:{line}, {name} -> {file}:{line}, ...`（最多 3 条，超出加 `... and N more`）。

#### 改动

| 文件 | 位置 | 改动 |
|------|------|------|
| `crates/tui/src/tools/search.rs` | hits 序列化前 | +~10 行（拼接 summary 字符串） |

#### 验收

- `grep_files("StructuredVerdict")` → 返回 JSON 中含 `symbol_index_summary` 字符串
- 模型在不逐字读 JSON 的情况下也能看到索引命中

---

### V3-5：多工作区索引（DS Pick 集成）

#### 问题

DS Pick 的每个 Composer / 恢复线程可以打开不同的工作区路径。但当前索引在 `serve --http` 启动时构建一次，基于 sidecar 进程的 `current_dir`。切换工作区后索引仍然是旧的。

#### 方案

分两步：

**Step 1：索引路径已经 workspace-aware（已完成）**

`build_index(workspace: &Path)` 和 `lookup_symbol_hits(workspace: &Path, ...)` 已支持传入工作区路径。

**Step 2：DS Pick 前端在切换工作区时触发重建**

当用户切换 Composer / 打开新项目时，Tauri 后端调用 `rebuild_symbol_index` 命令（后台线程，不阻塞 UI）：

```rust
#[tauri::command]
fn rebuild_symbol_index(state: tauri::State<AppState>, workspace: String) -> Result<(), String> {
    let ws = PathBuf::from(&workspace);
    std::thread::spawn(move || {
        let index = symbol_index::build_index(&ws, SymbolVisibility::Public);
        let path = ws.join(".deepseek").join("symbols.json");
        if let Some(parent) = path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        let _ = std::fs::write(&path, serde_json::to_string_pretty(&index).unwrap_or_default());
    });
    Ok(())
}
```

#### 改动

| 文件 | 改动 |
|------|------|
| `crates/desktop/src/commands.rs` | +15 行 `rebuild_symbol_index` 命令 |
| `crates/desktop/src/main.rs` | +1 行注册命令 |

#### 验收

- 打开项目 A → `.deepseek/symbols.json` 含项目 A 的符号
- 切换到项目 B → 重建 → `.deepseek/symbols.json` 含项目 B 的符号
- 两个项目的索引互不污染

---

## V3 优先级排序

| 批次 | 项 | 改动量 | 收益 |
|:--:|-----|--------|------|
| **D** | V3-1（行号局限）+ V3-3（read_file 引导）+ V3-4（人类可读前缀） | ~25 行 | 消除模型盲用/盲信索引的两个高频问题 |
| **E** | V3-2（BM25-索引融合） | ~25 行 | 搜索结果质量进一步提升 |
| **F** | V3-5（多工作区索引） | ~16 行 | DS Pick 切换项目时索引不失效 |
| **后置** | V3-6 并行解析（C2 遗留） | ~30 行 + rayon dep | 大型仓库启动加速 |

---

## 改动汇总（全 6 项）

| 项 | 文件 | 改动量 |
|----|------|--------|
| V3-1 | `pick-rules.md` + `search.rs` | +10 行 |
| V3-2 | `search.rs` | +25 行 |
| V3-3 | `pick-rules.md` | +5 行 |
| V3-4 | `search.rs` | +10 行 |
| V3-5 | `commands.rs` + `main.rs` | +16 行 |
| V3-6 | `symbol_index.rs` + `Cargo.toml` | +30 行 + 1 dep |
| **总计** | | **~96 行** |

---

## 对比：V2 vs V3

| 维度 | V2 | V3 |
|------|-----|-----|
| 核心关注 | 索引**建得好不好**（减少误中、增量、种类） | 索引**用得对不对**（模型如何消费、多工作区对齐） |
| 主要改动层 | `symbol_index.rs` 索引引擎 | `search.rs` 输出层 + `pick-rules.md` 消费指引 |
| 新增能力 | 增量重建、私有符号、符号种类扩展 | 行号可信度标注、read_file 引导、人类可读前缀、BM25 融合 |
