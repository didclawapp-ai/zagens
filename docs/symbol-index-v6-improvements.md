# 符号索引 V6 改进方案

**日期**: 2026-05-28  
**基于**: V5 已实施版本（`schema_version: 3`，Rust + TS/TSX，查询层含 `match_score` / bridge / 变更追踪）  
**原则**: 轻量、无新依赖、向后兼容；与 LSP 语言检测对齐  
**前置阅读**: [`symbol-index-v5-improvements.md`](symbol-index-v5-improvements.md)

---

## V5 已实施状态回顾

| V5 项 | 内容 | 状态 |
|-------|------|:----:|
| V5-2 | `match_score` 置信度 | ✅ |
| V5-3 | `symbol_kind` 外部入口 | ✅ |
| V5-4 | mtime 指纹缓存 (`.symbols_fingerprint`) | ✅ |
| V5-5 | grep 命中附带文件符号摘要 | ✅ |
| V5-6 | Tauri bridge 跨语言关联 | ✅ |
| V5-7 | 轻量调用关系 `calls`（构建时写入） | ✅ 构建 / ❌ 查询未返回 |
| V5-8 | 符号变更追踪 (`.symbols_changes.json`) | ✅ |

**文档与实现落差（V6 要修）**

| 问题 | 说明 |
|------|------|
| README 声称支持 Go / Python / C++ | 实际仅索引 `.rs` + `.ts`/`.tsx` |
| JavaScript (`.js`/`.jsx`) 未纳入 walk | LSP 已支持，索引未覆盖 |
| `grep_files` 不返回 `calls` | 索引已有字段，查询层未序列化 |
| `get_symbol_index_info` freshness | 桌面壳只扫 `crates/`/`src/` 深度 4，与 runtime walk 不一致 |

---

## V6 目标

1. **语言覆盖**：与 `lsp/registry.rs` 的扩展名检测对齐（Rust、TS/TSX、JS/JSX/MJS/CJS、Python、Go）。
2. **查询补全**：`symbol_index_hits` 附带 `calls`。
3. **文档诚实**：README / CHANGELOG 与实现一致。
4. **`schema_version` 升至 4**：触发旧索引自动重建。

C/C++、Vue SFC 留 V7（正则复杂度高，行号可靠性需单独说明）。

---

## V6 改动清单

### V6-1：扩展 `walk_source_files`

在 `symbol_index/mod.rs` 中扩展扩展名 → 语言标签映射：

| 扩展名 | 标签 | 解析器 |
|--------|------|--------|
| `.rs` | `rs` | `extract_symbols` (syn) |
| `.ts`, `.tsx`, `.js`, `.jsx`, `.mjs`, `.cjs` | `ts` | `extract_ts_symbols` |
| `.py`, `.pyi` | `py` | `extract_py_symbols` |
| `.go` | `go` | `extract_go_symbols` |

JS 复用 TS 正则解析器（语法子集高度重叠）。

### V6-2：Python 正则解析器 (`extract_py_symbols`)

覆盖本仓库及 Office 脚本常见写法：

```python
def foo(): ...
async def bar(): ...
class Baz: ...
    def method(self): ...
```

符号 kind：`fn` / `method` / `class`。

跳过：纯注释行、`if __name__ == "__main__"` 块内的误匹配（关键字过滤）。

### V6-3：Go 正则解析器 (`extract_go_symbols`)

```go
func Foo() ...
func (r *T) Bar() ...
type MyStruct struct { ... }
type MyIface interface { ... }
```

符号 kind：`fn` / `method` / `struct` / `interface` / `type`。

### V6-4：`grep_files` 返回 `calls`

在 `lookup_symbol_hits()` 中，当索引条目含非空 `calls` 时写入 hit JSON：

```json
{
  "symbol": "loadWorkspaceFileIntoPreview",
  "file": "crates/desktop/web-ui/src/openWorkspaceFile.ts",
  "line": 28,
  "kind": "fn",
  "match_score": 1.0,
  "calls": ["normalizeWorkspaceRelPath", "detectFileType"]
}
```

### V6-5：Tauri bridge 扫描 JS

`build_bridge_pairs()` 的 TS 分支改为匹配标签 `ts`（已含 JS 扩展名），无需单独逻辑。

### V6-6：桌面 freshness 对齐

`get_symbol_index_info` 的 stale 检测扩展名列表与 walk 一致；优先读 `.symbols_fingerprint`（与 runtime `index_status` 同思路）。

### V6-7：模块拆分

`symbol_index.rs`（~1300 行）拆为：

- `symbol_index/mod.rs` — 类型、构建、查询、walk、Rust syn 解析
- `symbol_index/extract.rs` — TS/JS、Python、Go 正则解析器

符合 code-organization 软上限 ~1000 行/文件。

---

## 验收标准

| # | 操作 | 期望 |
|---|------|------|
| 1 | 重建索引后 `schema_version == 4` | 旧 v3 索引触发 stale 重建 |
| 2 | `scripts/write_pptx.py` 出现在 `symbols.json` | 含 `main` fn |
| 3 | `.mjs` 文件被索引 | 与 TS 相同解析路径 |
| 4 | `grep_files(..., symbol_index: true)` | hits 含 `calls`（非空时） |
| 5 | `cargo test -p deepseek-runtime-server symbol_index` | 全绿 |

---

## V7 预留（未在本轮实施）

| 项 | 说明 |
|----|------|
| C/C++ | `.c`/`.cpp`/`.h`/`.hpp` 正则；宏/模板行号漂移需 `symbol_index_note` |
| Vue/Svelte SFC | 提取 `<script>` 块再交给 TS 解析器 |
| CamelCase 模糊匹配 | `loadworkspace` → `loadWorkspaceFileIntoPreview` |
| callers 反向索引 | 由 `calls` 反查「谁调用了 X」 |
| rayon 并行解析 | V2-C2 遗留项 |
| CLI 启动预热 | 首次 `grep_files` 前非阻塞 `ensure_symbol_index` |

---

## 实施记录

| 项 | 状态 | 备注 |
|----|:----:|------|
| V6-1 walk 扩展 | ✅ | 2026-05-28 |
| V6-2 Python 解析 | ✅ | 2026-05-28 |
| V6-3 Go 解析 | ✅ | 2026-05-28 |
| V6-4 calls 查询返回 | ✅ | 2026-05-28 |
| V6-5 bridge JS | ✅ | 2026-05-28 |
| V6-7 模块拆分 | ✅ | 2026-05-28 |

> V7 见 [`symbol-index-v7-improvements.md`](symbol-index-v7-improvements.md)。
