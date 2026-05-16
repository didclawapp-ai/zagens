# 符号索引 V4 改进方案

**日期**: 2026-05-16  
**基于**: `symbol_index.rs` (781 行) V3 已实施版本  
**原则**: 轻量、无新依赖、向后兼容  

---

## 问题诊断

当前索引有三个核心缺陷：

| # | 问题 | 影响 |
|---|------|------|
| G1 | 只走 `.rs` 文件，TypeScript 完全未覆盖 | 模型查 TS 函数查不到，只能自己写脚本硬读 |
| G2 | `trait` 块内的方法未索引 | 查 trait 方法名返回空，模型幻觉填充 |
| G3 | 嵌套 `mod` 只走一层 | 深层模块的函数漏索引 |

---

## V4 改动（3 项，~130 行）

---

### V4-1：TypeScript / TSX 解析器

**不引入任何新依赖**，用 `regex` crate（已在 `search.rs` 中使用）覆盖项目实际出现的所有 TS 函数写法。

覆盖的写法（从你的项目文件确认）：

```typescript
// 1. export function / export async function
export function normalizeWorkspaceRelPath(raw: string): string { ... }
export async function loadWorkspaceFileIntoPreview(opts: {...}): Promise<...> { ... }

// 2. 非导出 async function
async function bootstrap() { ... }

// 3. export const 箭头函数
export const handleClick = () => { ... }
export const handleClick = async (e: Event) => { ... }

// 4. React 组件
export default function App() { ... }

// 5. interface / type alias
export interface PreviewState { ... }
export type FileType = 'text' | 'binary';

// 6. class 及其方法
export class MyService { ... }
  public async fetchData(): Promise<...> { ... }
  private handleError(e: Error): void { ... }

// 7. enum
export enum Status { ... }
```

**新增函数 `extract_ts_symbols()`**，加在 `extract_symbols()` 之后：

```rust
/// Extract symbols from a TypeScript / TSX file using regex patterns.
/// Uses the `regex` crate already present in Cargo.toml (used by search.rs).
/// Covers all function writing styles found in this project.
fn extract_ts_symbols(
    path: &Path,
    source_mtime: u64,
) -> Option<Vec<SymbolEntry>> {
    use std::io::{BufRead, BufReader};

    let file = std::fs::File::open(path).ok()?;
    let reader = BufReader::new(file);
    let mut symbols: Vec<SymbolEntry> = Vec::new();

    // Patterns: (regex, kind)
    // Order matters — first match wins per line.
    let patterns: &[(&str, &str)] = &[
        // export default function App / export default async function
        (r"^export\s+default\s+(?:async\s+)?function\s+(\w+)", "fn"),
        // export async function foo / export function foo
        (r"^export\s+(?:async\s+)?function\s+(\w+)", "fn"),
        // async function foo / function foo (non-exported)
        (r"^(?:async\s+)?function\s+(\w+)", "fn"),
        // export const foo = async () => / = () => / = (e: Event) =>
        (r"^export\s+const\s+(\w+)\s*=\s*(?:async\s+)?(?:\(|[A-Za-z_]\w*\s*=>)", "fn"),
        // export interface Foo / export default interface Foo
        (r"^export\s+(?:default\s+)?interface\s+(\w+)", "interface"),
        // interface Foo (non-exported)
        (r"^interface\s+(\w+)", "interface"),
        // export type Foo = / export type Foo<
        (r"^export\s+type\s+(\w+)\s*[=<]", "type"),
        // export class / export default class / class
        (r"^(?:export\s+(?:default\s+)?)?class\s+(\w+)", "class"),
        // export enum / enum
        (r"^(?:export\s+)?(?:const\s+)?enum\s+(\w+)", "enum"),
        // class methods (indented): public/private/protected/async methodName(
        (r"^\s{2,}(?:(?:public|private|protected|static|async|readonly|override)\s+)*(\w+)\s*[<(]", "method"),
    ];

    let compiled: Vec<(regex::Regex, &str)> = patterns
        .iter()
        .filter_map(|(pat, kind)| {
            regex::Regex::new(pat).ok().map(|r| (r, *kind))
        })
        .collect();

    // Keywords that regex might accidentally match as function names
    const SKIP_NAMES: &[&str] = &[
        "if", "for", "while", "switch", "catch", "return", "new",
        "typeof", "instanceof", "in", "of", "from", "import", "export",
        "constructor", "super", "extends", "implements",
    ];

    let mut current_class: Option<String> = None;
    let mut brace_depth: i32 = 0;
    let mut class_brace_start: i32 = -1;

    for (line_idx, line_result) in reader.lines().enumerate() {
        let line = match line_result {
            Ok(l) => l,
            Err(_) => continue,
        };
        let line_num = line_idx + 1; // 1-based

        // Track brace depth to know when class scope ends
        for ch in line.chars() {
            match ch {
                '{' => brace_depth += 1,
                '}' => {
                    brace_depth -= 1;
                    if current_class.is_some() && brace_depth <= class_brace_start {
                        current_class = None;
                        class_brace_start = -1;
                    }
                }
                _ => {}
            }
        }

        // Skip pure comment lines
        let trimmed = line.trim();
        if trimmed.starts_with("//")
            || trimmed.starts_with('*')
            || trimmed.starts_with("/*")
        {
            continue;
        }

        for (re, kind) in &compiled {
            if let Some(cap) = re.captures(&line) {
                if let Some(name_match) = cap.get(1) {
                    let name = name_match.as_str().to_string();

                    if SKIP_NAMES.contains(&name.as_str()) {
                        continue;
                    }

                    let full_name = if *kind == "method" {
                        match &current_class {
                            Some(cls) => format!("{}::{}", cls, name),
                            None => continue, // method outside class context — skip
                        }
                    } else {
                        if *kind == "class" {
                            current_class = Some(name.clone());
                            class_brace_start = brace_depth;
                        }
                        name
                    };

                    symbols.push(SymbolEntry {
                        kind: kind.to_string(),
                        name: full_name,
                        line: line_num,
                        source_mtime,
                    });
                    break; // first matching pattern wins
                }
            }
        }
    }

    symbols.sort_by_key(|s| s.line);
    symbols.dedup_by(|a, b| a.kind == b.kind && a.name == b.name);

    Some(symbols)
}
```

**改动 `walk_rs_files` → `walk_source_files`**，扩展支持 `.ts`/`.tsx`：

```rust
/// Walk source files (.rs, .ts, .tsx), returning (path, mtime, lang).
fn walk_source_files(root: &Path) -> Vec<(PathBuf, u64, &'static str)> {
    let mut files = Vec::new();
    walk_source_files_impl(root, &mut files);
    files
}

fn walk_source_files_impl(dir: &Path, out: &mut Vec<(PathBuf, u64, &'static str)>) {
    let Some(name) = dir.file_name().and_then(|n| n.to_str()) else {
        return;
    };
    if SKIP_DIRS.contains(&name) {
        return;
    }
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let p = entry.path();
        if p.is_dir() {
            walk_source_files_impl(&p, out);
        } else {
            let lang = match p.extension().and_then(|e| e.to_str()) {
                Some("rs") => "rs",
                Some("ts") | Some("tsx") => "ts",
                _ => continue,
            };
            let mtime = std::fs::metadata(&p)
                .ok()
                .and_then(|m| m.modified().ok())
                .map(|t| {
                    t.duration_since(std::time::UNIX_EPOCH)
                        .unwrap_or_default()
                        .as_secs()
                })
                .unwrap_or(0);
            out.push((p, mtime, lang));
        }
    }
}
```

**改动 `build_index()` 分发逻辑**（替换原有 `walk_rs_files` 调用处）：

```rust
// 原来：
let rs_entries = walk_rs_files(workspace);
for (path, mtime) in rs_entries {
    // ...
    if let Some(symbols) = extract_symbols(&path, visibility, mtime) {

// V4 替换为：
let src_entries = walk_source_files(workspace);
for (path, mtime, lang) in src_entries {
    // ... rel_str, incremental check 完全不变 ...

    let symbols = match lang {
        "rs" => extract_symbols(&path, visibility, mtime),
        "ts" => extract_ts_symbols(&path, mtime),
        _ => None,
    };

    if let Some(syms) = symbols {
        if !syms.is_empty() {
            files.insert(rel_str, FileSymbols { symbols: syms });
        }
    }
}
```

同理，`index_status()` 里的 `walk_rs_files` 调用也替换为 `walk_source_files`，忽略 lang 字段即可：

```rust
for (path, mtime, _lang) in walk_source_files(workspace) {
```

#### 改动汇总

| 文件 | 位置 | 改动量 |
|------|------|--------|
| `symbol_index.rs` | 新增 `extract_ts_symbols()` | +~90 行 |
| `symbol_index.rs` | `walk_rs_files` → `walk_source_files` | +10 行，删 8 行 |
| `symbol_index.rs` | `build_index()` 分发 | +5 行，改 2 行 |
| `symbol_index.rs` | `index_status()` walk 调用 | 改 1 行 |

#### 验收

```
grep_files("loadWorkspaceFileIntoPreview")
  → symbol_index_hits 命中 openWorkspaceFile.ts:28

grep_files("normalizeWorkspaceRelPath")
  → symbol_index_hits 命中 openWorkspaceFile.ts:11

grep_files("bootstrap")
  → symbol_index_hits 命中 main.tsx:9

grep_files("PreviewState")  （interface）
  → symbol_index_hits 命中对应 .ts 文件

# TS 文件变更后增量重建生效（mtime 驱动）
```

---

### V4-2：Rust trait 方法补漏

**问题**：`extract_symbols()` 处理 `syn::Item::Trait` 时只记录 trait 名本身，trait 里的方法签名完全跳过。

**修复**：在 `extract_symbols()` 的 impl 块遍历之后，加 trait 方法遍历：

```rust
// 加在 impl 块遍历（for item in &file.items { if let syn::Item::Impl ...}）之后

// Trait methods: index as "TraitName::method_name" with kind "trait_fn"
for item in &file.items {
    if let syn::Item::Trait(t) = item {
        if visibility == SymbolVisibility::Public && !is_pub(&t.vis) {
            continue;
        }
        let trait_name = t.ident.to_string();
        for trait_item in &t.items {
            if let syn::TraitItem::Fn(method) = trait_item {
                let name = format!("{}::{}", trait_name, method.sig.ident);
                symbols.push(make_entry(
                    name,
                    "trait_fn",
                    method.sig.ident.span().byte_range().start,
                    &line_starts,
                    source_mtime,
                ));
            }
        }
    }
}
```

新增 `kind = "trait_fn"`，与 `impl_fn` 对称，方便 `kind_filter` 区分。

#### 改动汇总

| 文件 | 位置 | 改动量 |
|------|------|--------|
| `symbol_index.rs` | `extract_symbols()` 末尾 | +12 行 |

#### 验收

```
# 假设 search.rs 里有 trait FileSearch { fn grep(...) }
grep_files("FileSearch::grep")
  → symbol_index_hits 命中对应行，kind = "trait_fn"
```

---

### V4-3：嵌套 mod 递归走完整深度

**问题**：现在 `extract_symbols()` 只走顶层 mod 的直接 content，二层以上嵌套 mod 的函数漏掉。

**修复**：提取递归函数 `extract_mod_items()`，替换现有的两段重复遍历逻辑：

```rust
/// Recursively extract symbols from a list of items (handles nested mods).
fn extract_mod_items(
    items: &[syn::Item],
    visibility: SymbolVisibility,
    line_starts: &[usize],
    source_mtime: u64,
    symbols: &mut Vec<SymbolEntry>,
) {
    for item in items {
        if matches!(item, syn::Item::Impl(_)) {
            continue; // impl blocks handled separately
        }
        if let Some(entry) = item_symbol(item, visibility, line_starts, source_mtime) {
            symbols.push(entry);
        }
        // Recurse into inline mod content
        if let syn::Item::Mod(m) = item {
            if let Some((_, ref content)) = m.content {
                extract_mod_items(content, visibility, line_starts, source_mtime, symbols);
            }
        }
    }
}
```

然后 `extract_symbols()` 里原来两段遍历（顶层 + 一层 mod）合并为一行：

```rust
// 替换原有的两个 for 循环（顶层遍历 + mod content 遍历）
extract_mod_items(&file.items, visibility, &line_starts, source_mtime, &mut symbols);

// impl 块遍历保持不变（单独 for 循环，不变）
// trait 方法遍历保持不变（V4-2 新增，不变）
```

#### 改动汇总

| 文件 | 位置 | 改动量 |
|------|------|--------|
| `symbol_index.rs` | 新增 `extract_mod_items()` | +18 行 |
| `symbol_index.rs` | `extract_symbols()` 简化 | 删 ~15 行，改 1 行 |

---

## 实施顺序

| 批次 | 项 | 理由 |
|:--:|------|------|
| **G** | V4-1（TS支持） | 最大收益，直接解决"模型自己写脚本"问题 |
| **H** | V4-2 + V4-3 | 改动量小，一起做，顺手补完 |

---

## 改动总计

| 项 | 净增行数 |
|----|---------|
| V4-1 TS解析器 | +~100 行 |
| V4-2 trait方法 | +12 行 |
| V4-3 嵌套mod | +5 行（新增18，删15） |
| **总计** | **~117 行** |

---

## 注意事项

**regex crate**：`extract_ts_symbols` 使用 `regex::Regex`。确认 `crates/tui/Cargo.toml` 已有：

```toml
regex = "1"
```

如果 `search.rs` 里已经 `use regex::Regex`，则无需新增依赖。

**TS 方法误匹配**：`method` 模式用 2+ 空格缩进区分类方法和顶层函数。对缩进规范的 TypeScript 代码（Prettier 格式化后）完全可靠。如发现误匹配，可在后续迭代收紧正则。

**Python 支持（V5 候选）**：office 脚本是 Python，如有需要，`def foo(` 模式加 10 行即可，本次不做保持轻量。
