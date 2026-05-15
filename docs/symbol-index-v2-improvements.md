# 符号索引 V2 改进方案

**基于**: 现有 `symbol_index.rs` + `search.rs` 管道  
**日期**: 2026-05-15  
**原则**: 每项独立可验证，改动量可控，不与现有管道冲突

---

## 优先级排序

| 批次 | 项 | 改动量 | 收益 | 理由 |
|:--:|-----|--------|------|------|
| **A1** | 排除 `target/` / `node_modules/` / `dist/` | ~15 行 | 消除 60%+ 的无效解析 | 当前索引 475KB 有一半是 build artifacts |
| **A2** | 索引时提示模型使用 | ~8 行 prompt | 模型遵从率从"碰运气"到"显式指令" | 零 Rust 改动 |
| **A3** | 陈旧/缺失索引状态标记 | ~10 行 | 消除"以为有这个符号"的幻觉 | 一行 `symbol_index_status` |
| **B1** | 查询语义增强（整词/前缀/kind 过滤） | ~30 行 | 减少误中 | 搜 `Config` 不再命中 `ConfigStore` |
| **B2** | 增量重建（mtime） | ~40 行 | 大仓库启动从 5s → 0.2s | 只在变更时重扫 |
| **B3** | 扩展符号种类（impl, type, const, static, macro） | ~25 行 | 减少"正则 grep 兜底" | 当前只有 fn/struct/enum |
| **C1** | 非 pub 符号（workspace crate 内） | ~20 行 | 内部函数可被索引 | 可配置粒度 |
| **C2** | 并行解析（rayon） | ~30 行 | 索引构建加速 | 纯 CPU 优化 |
| **后置** | 多语言（TS/TSX）、文件监听 | 各 ~100+ 行 | 生态扩展 | 仓内 TS 不多 |

---

## A1：排除构建产物和依赖目录

### 现状

`build_index` 全量遍历 `.rs` 文件，无排除逻辑。当前 `symbols.json` 475KB 中有大量 `target/` 下的 build script 和 `crates/*/target/` 下的重复内容。

### 方案

在 `build_index` 的文件遍历循环中加跳过列表：

```rust
const SKIP_DIRS: &[&str] = &["target", "node_modules", "dist", ".git", ".deepseek"];
const SKIP_PREFIXES: &[&str] = &[".", "~"];
```

遍历时 `entry.file_name()` 命中 `SKIP_DIRS` 或 `.starts_with(SKIP_PREFIXES)` → 跳过整棵子树。

### 改动

| 文件 | 位置 | 改动 |
|------|------|------|
| `crates/tui/src/symbol_index.rs` | `build_index` 函数 | +~15 行过滤逻辑 |

### 验收

- 重建索引 → `symbols.json` 不再含 `target/`、`node_modules/` 等路径
- 文件大小预期从 475KB 降至 ~200KB

---

## A2：系统提示中显式引导模型使用符号索引

### 现状

`grep_files` 返回结果中有 `symbol_index_hits` 字段，但没有任何 prompt 告诉模型"这个字段怎么用"。模型可能完全忽略。

### 方案

在系统 prompt（`base.md` 或 `GENERAL_AGENT_PROMPT`）的工具使用说明中加一段：

```
When `grep_files` returns a `symbol_index_hits` array, use it as your
first lookup before `read_file`. Each hit gives you the exact file:line
of the definition — read that line range first, then decide if you need
more context. Do not re-scan file trees when the index already answered
the question.
```

### 改动

| 文件 | 改动 |
|------|------|
| 系统 prompt（TUI 侧 `prompts.rs` 或 `base.md`） | +~8 行 |

### 验收

- 下次讨论代码时，观察我是否在 `grep_files` 返回后优先用 `symbol_index_hits` 而非再 `read_file` 碰运气

---

## A3：陈旧/缺失索引状态标记

### 现状

如果 `.deepseek/symbols.json` 不存在或过期，`lookup_symbol_hits` 静默返回空数组。模型不知道"没有这个符号"和"索引没建"的区别。

### 方案

在 `grep_files` 返回体中加一个人类可读的 `symbol_index_status` 字段：

```json
{
  "matches": [...],
  "symbol_index_hits": [...],
  "symbol_index_status": "fresh" | "stale" | "missing" | "building"
}
```

判断逻辑：
- `missing`：`symbols.json` 文件不存在
- `stale`：`symbols.json` 的 mtime 早于工作区中最新 `.rs` 文件的 mtime
- `building`：索引构建线程仍在运行（`AtomicBool` 标记）
- `fresh`：其余情况

### 改动

| 文件 | 位置 | 改动 |
|------|------|------|
| `crates/tui/src/tools/search.rs` | `lookup_symbol_hits` 调用处 | +~10 行状态判断 + 字段注入 |

### 验收

- 删除 `symbols.json` → grep 返回 `"symbol_index_status": "missing"`
- `touch` 任意 `.rs` → grep 返回 `"symbol_index_status": "stale"`

---

## B1：查询语义增强

### 现状

`query_symbol` 做子串匹配——搜 `Config` 命中 `ConfigStore`、`AppConfig`、`Reconfigure` 等。对于大项目，`symbol_index_hits` 数组经常超过 20 条。

### 方案

在 `query_symbol` 中加匹配模式选择：

```rust
enum MatchMode {
    Substring,    // 默认，向后兼容
    WholeWord,    // 整词匹配（前后非字母数字）
    Prefix,       // 前缀匹配
    Exact,        // 完全相等
}
```

默认保持 `Substring`。未来可在 `grep_files` 工具 schema 中加 `symbol_match: "substring" | "whole_word" | "prefix"` 参数。

同时加 `kind` 过滤：`query_symbol("Config", kind: Some("struct"))` 只返回 struct。

### 改动

| 文件 | 位置 | 改动 |
|------|------|------|
| `crates/tui/src/symbol_index.rs` | `query_symbol` 函数签名 | +~20 行匹配逻辑 |
| `crates/tui/src/tools/search.rs` | `lookup_symbol_hits` 调用 | +~10 行 kind 过滤 |

### 验收

- `query_symbol("Config", Exact)` → 只返回名为 `Config` 的符号，不返回 `ConfigStore`
- `query_symbol("Config", WholeWord)` → 返回 `Config`、`AppConfig`，不返回 `Reconfigure`

---

## B2：增量重建

### 现状

每次 `serve --http` 启动都全量重建 `symbols.json`。475KB 的索引解析 ~400 个 `.rs` 文件约需 5-8 秒。重复启动时浪费。

### 方案

1. 读取现有 `symbols.json`，记录每项的 `source_mtime`（需在索引项中新增此字段）
2. 遍历 `.rs` 文件时，如果某文件的 mtime ≤ 索引中记录的 mtime，跳过解析，直接复用旧条目
3. 如果目录结构变了（文件新增/删除），处理 delta——新增的解析，删除的从索引中移除

最简实现（只做"旧文件跳过"）：

```rust
let existing_index: HashMap<PathBuf, SymbolEntry> = load_old_index();
let mut new_symbols: Vec<SymbolEntry> = Vec::new();

for (path, mtime) in walk_rs_files() {
    if let Some(old) = existing_index.get(&path) {
        if old.source_mtime >= mtime {
            new_symbols.push(old.clone()); // 复用
            continue;
        }
    }
    // 解析新文件
    new_symbols.extend(parse_file(&path));
}
```

### 改动

| 文件 | 改动 |
|------|------|
| `crates/tui/src/symbol_index.rs` | `SymbolEntry` 加 `source_mtime: u64`，`build_index` 加增量逻辑 ~40 行 |

### 验收

- 首次构建：耗时正常（5-8 秒）
- 第二次构建（无文件变更）：耗时 < 0.5 秒
- 修改一个 `.rs` 后重建：仅该文件重解析

---

## B3：扩展符号种类

### 现状

当前只索引 `fn`、`struct`、`enum`、`trait`。`impl` 块、`type` 别名、`const`、`static`、`macro_rules!` 都不在索引中。

### 方案

扩展 `syn` 的 `Visit` impl，增加对以下 AST 节点的收集：

| AST 节点 | 索引 kind | 示例 |
|----------|----------|------|
| `ItemType` | `type` | `type SharedSubAgentManager = Arc<...>` |
| `ItemConst` | `const` | `const MAX_DEPTH: usize = 10` |
| `ItemStatic` | `static` | `static CONFIG: Lazy<Config> = ...` |
| `ItemMacro` | `macro` | `macro_rules! json_response { ... }` |
| `ImplItemFn` | `impl_fn` | `impl Config { fn new() -> ... }` (trait impl 中的方法) |

每个种类都标注所在的 `impl` 块（如 `impl ConfigToml`），便模型区分"这个 `new()` 是 `Config::new()` 而非 `Session::new()`"。

### 改动

| 文件 | 改动 |
|------|------|
| `crates/tui/src/symbol_index.rs` | `SymbolKind` 枚举 + `Visit` impl + ~25 行 |

### 验收

- 搜 `MAX_DEPTH` → 命中，非正则 grep 兜底
- 搜 `macro_rules! parse_request` → 命中
- `impl_fn` 条目显示其所属 impl 块名称

---

## C1：非 pub 符号（workspace crate 内）

### 现状

只索引 `pub` 符号。但模型经常问到内部函数（如 `fn subagent_done_sentinel`、`fn workspace_root`）。

### 方案

加配置开关 `include_private`。默认 `false`（向后兼容）。当 `true` 时，对 workspace crate 内的文件也索引私有符号。对外部依赖（`~/.cargo/registry`）始终只索 `pub`。

粒度控制——用 `enum SymbolVisibility { Public, Private, All }` + 默认 `Public`，环境变量 `DEEPSEEK_SYMBOL_VISIBILITY=all` 可覆盖。

### 改动

| 文件 | 改动 |
|------|------|
| `crates/tui/src/symbol_index.rs` | `build_index` 加 `visibility` 参数 + ~20 行过滤逻辑 |
| 启动参数 | `serve --http` 加可选 env/flag |

### 验收

- `DEEPSEEK_SYMBOL_VISIBILITY=all` → 索引含 `workspace_root`（当前为私有函数）
- 默认 → 不含

---

## C2：并行解析

### 现状

`build_index` 单线程顺序解析，每个文件串行 `syn::parse_file`。

### 方案

用 `rayon` 并行化文件解析：

```rust
use rayon::prelude::*;

let symbols: Vec<SymbolEntry> = rs_files
    .par_iter()
    .flat_map(|path| {
        parse_file_symbols(path).unwrap_or_default()
    })
    .collect();
```

注意：`syn::parse_file` 需要大栈（当前已用 `thread::Builder::stack_size(8MB)`）。`rayon` 的全局线程池默认栈为 2MB——需在 rayon `ThreadPoolBuilder` 中设置 8MB 栈。

### 改动

| 文件 | 改动 |
|------|------|
| `crates/tui/Cargo.toml` | +`rayon = "1"` |
| `crates/tui/src/symbol_index.rs` | `build_index` ~30 行改写 |

### 验收

- 索引构建时间从 5-8s 降至 1-2s（4 核）

---

## 实施批次

| 批次 | 内容 | 总改动量 | 时间 |
|:--:|------|------|------|
| **A** | A1（排除）+ A2（prompt）+ A3（状态标记） | ~33 行 | 一次 PR |
| **B** | B1（查询语义）+ B2（增量）+ B3（符号种类） | ~95 行 | 一次 PR |
| **C** | C1（私有符号）+ C2（并行解析） | ~50 行 + 1 dep | 按需 |
| **后置** | 多语言、文件监听、多工作区文档 | ~200+ 行 | 后续 |

A 批次改动最小，信噪比最高——三个改动加起来 33 行，解决的是"索引不可信"和"模型不读索引"两个最常见问题。
