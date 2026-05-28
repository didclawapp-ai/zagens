# 符号索引 V7 改进方案

**日期**: 2026-05-28  
**基于**: V6 已实施版本（`schema_version: 4`，Rust / TS·JS / Python / Go，`calls` 查询返回）  
**原则**: 轻量、无新依赖（并行用 `std::thread::scope`，不引入 rayon）、向后兼容  
**前置阅读**: [`symbol-index-v6-improvements.md`](symbol-index-v6-improvements.md)

---

## V6 已实施状态回顾

| V6 项 | 内容 | 状态 |
|-------|------|:----:|
| V6-1 | walk 扩展 JS / Python / Go | ✅ |
| V6-2–3 | Python / Go 正则解析器 | ✅ |
| V6-4 | `grep_files` 返回 `calls` | ✅ |
| V6-5–6 | bridge JS + 桌面 freshness | ✅ |
| V6-7 | 模块拆分 `extract.rs` | ✅ |

---

## V7 目标

1. **C/C++** — 与 LSP `clangd` 扩展名对齐（`.c`/`.h`/`.cpp`/`.hpp` 等）。
2. **Vue / Svelte SFC** — 提取 `<script>` 块后走 TS 解析器，行号映射回 SFC 文件。
3. **CamelCase 模糊匹配** — 子序列 / 首字母缩写匹配（如 `lwfip` → `loadWorkspaceFileIntoPreview`）。
4. **callers 反向索引** — 查询时反查「谁调用了 X」。
5. **并行构建** — `std::thread::scope` 多文件并行解析（无 rayon 依赖）。
6. **启动预热** — sidecar 启动后非阻塞 `ensure_symbol_index`。
7. **`schema_version` 升至 5** — 新语言 + 查询行为变更触发重建。

---

## V7 改动清单

### V7-1：C/C++ 正则解析器 (`extract_cpp_symbols`)

| 模式 | kind |
|------|------|
| `class Foo` | class |
| `struct Bar` | struct |
| `enum class Baz` / `enum Baz` | enum |
| `namespace N` | namespace |
| 行尾 `{` 的函数定义 | fn |
| 行尾 `;` 的函数声明（头文件） | fn |

命中 C/C++ 符号时，`grep_files` 附加 `symbol_index_note` 说明宏/模板可能导致行号漂移。

### V7-2：Vue / Svelte SFC (`extract_sfc_symbols`)

- 扩展名：`.vue`、`.svelte`
- 用正则提取 `<script>...</script>` 内容
- 调用 `extract_ts_from_source(text, line_offset)` 映射行号

### V7-3：CamelCase 模糊匹配

在 `MatchMode::Substring` 中，子串未命中时依次尝试：

1. **子序列匹配**（query ≥ 3 字符）：`lwfile` ⊆ `loadWorkspaceFile…`
2. **CamelCase 首字母缩写**：`lwfip` → `loadWorkspaceFileIntoPreview`

优先级 `4`，`match_score: 0.4`。

### V7-4：callers 反向索引

新增 `query_callers(index, name)` — 扫描所有符号的 `calls` 字段，返回调用方列表。

`grep_files(symbol_index: true)` 响应新增 `symbol_index_callers`（与 hits 同 query term）。

### V7-5：并行构建

`build_index` 对需重解析的文件使用 `std::thread::scope` + 固定 worker 数（`available_parallelism` 上限 8）。

增量跳过（mtime 未变）仍在主线程完成，仅并行解析变更文件。

### V7-6：启动预热

`runtime_serve::run` 在解析 workspace 后调用 `warmup_if_needed`（`ensure_symbol_index` 别名）。

### V7-7：`schema_version` 5

旧 v4 索引自动 stale 重建。

---

## 验收标准

| # | 操作 | 期望 |
|---|------|------|
| 1 | 重建后 `schema_version == 5` | ✅ |
| 2 | `.cpp` / `.vue` 文件出现在索引 | ✅ |
| 3 | `grep_files("lwfip", symbol_index: true)` | 命中 CamelCase 符号，`match_score: 0.4` |
| 4 | 查询有 `calls` 的符号 | 响应含 `symbol_index_callers` |
| 5 | sidecar 启动 | 后台触发索引构建（非阻塞） |
| 6 | `cargo test -p deepseek-runtime-server symbol_index` | 全绿 |

---

## V8 预留

| 项 | 说明 |
|----|------|
| 专用 `lookup_symbol` 工具 | 比 `grep_files + symbol_index` 语义更清晰 |
| 文件监听增量更新 | 替代全量 walk |
| Rust `calls` 用 syn AST | 替代正则扫描 |

---

## 实施记录

| 项 | 状态 | 备注 |
|----|:----:|------|
| V7-1 C/C++ | ✅ | 2026-05-28 |
| V7-2 Vue/Svelte | ✅ | 2026-05-28 |
| V7-3 CamelCase 模糊 | ✅ | 2026-05-28 |
| V7-4 callers | ✅ | 2026-05-28 |
| V7-5 并行构建 | ✅ | 2026-05-28 |
| V7-6 启动预热 | ✅ | 2026-05-28 |
| V7-7 schema 5 | ✅ | 2026-05-28 |
