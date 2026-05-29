# `write_file` 工具优化方案

> **文档路径:** `docs/tech/WRITE_FILE_IMPROVEMENTS.md`（与 [TOOLS_PRINCIPLES.md](TOOLS_PRINCIPLES.md) 同属 `docs/tech/`）
> **最后更新:** 2026-05-29 | **权威实现:** `crates/runtime-server/src/tools/file/write.rs`
> **相关工具:** `edit_file`（`file/edit.rs`）、`read_file`（`file/read.rs`）

---

## 1. 背景

`write_file` 负责把 UTF-8 内容整体写入工作区文件，是 Agent 创建 / 覆写文件的主要入口。当前实现（`file/write.rs`）相比同目录的 `edit_file`、`read_file` 明显简陋：后两者已经积累了**行尾归一化、多编码探测、诊断式错误信息**等成熟处理，而 `write_file` 直接 `fs::write` 了事。

本文记录已识别的优化方向、原因与落地建议，按优先级排列。

### 1.1 当前流程

```
resolve_path → 读旧内容(read_to_string) → create_dir_all → fs::write
            → make_unified_diff → 追加 LSP 诊断 → 返回
```

### 1.2 问题汇总

| # | 方向 | 类型 | 优先级 |
|---|------|------|--------|
| 1 | 覆写时不保留原行尾（CRLF） | 正确性 / diff 噪声 | 高 |
| 2 | 旧内容读取吞掉非 UTF-8 → diff 失真 | 正确性 | 高 |
| 3 | 缺 JSX 括号平衡检查 | 一致性（低成本） | 高 |
| 4 | 非原子写入，中途失败会损坏原文件 | 健壮性 | 中 |
| 5 | 错误信息未本地化 / 与邻居不一致 | 一致性 | 中 |
| 6 | 无意外大幅截断保护 | 产品安全 | 低 |
| 7 | 内容完全相同时仍写入（动 mtime） | 性能 / 副作用 | 低 |
| 8 | 新建大文件时 diff = 整文件副本，输出 token 翻倍 | 性能 / 上下文 | 高（大写入场景） |
| 9 | diff 计算成本随内容增大急剧上升（Myers O(N·D)） | 性能 | 中（大写入场景） |
| 10 | 无写入字节上限（`read_file` 有 `MAX_FILE_SIZE`，write 没有） | 健壮性 | 中 |
| 11 | 单次巨量生成被模型截断 → 落盘语法损坏的半截文件 | 正确性 | 高（大写入场景） |

---

## 2. 优化项详解

### 2.1 覆写时保留原文件行尾（CRLF）— 高优先级

**现状：** `edit_file` 会检测文件原有行尾并把模型给的 `\n` 归一化：

```rust
// file/edit.rs
let file_le = if contents.contains("\r\n") { "\r\n" } else { "\n" };
// ... normalize_line_endings(text, file_le)
```

而 `write_file` 直接写入模型给的内容（几乎总是 LF）：

```rust
// file/write.rs
fs::write(&file_path, file_content) // 模型给的 LF 原样落盘
```

**问题：** 在 Windows 仓库覆写一个原本是 CRLF 的文件后，整个文件行尾被改成 LF，git 会报告**全文件变更**，制造巨大的无关 diff，污染审查。

**建议：** 当 `existed_before == true` 时，沿用旧文件探测出的行尾，对 `content` 调用 `normalize_line_endings`（已是 `file` 模块内的公共函数）。新建文件保持原样（默认 LF）。

### 2.2 旧内容读取需编码安全 — 高优先级

**现状：**

```rust
// file/write.rs
let prior_contents = if existed_before {
    fs::read_to_string(&file_path).unwrap_or_default()
} else {
    String::new()
};
```

**问题：** 若旧文件是 GB18030 / UTF-16（`read_file` 专门用 `detect_and_decode` 处理这些编码），`read_to_string` 会失败，`unwrap_or_default()` 静默拿到空串。后果：

- `make_unified_diff` 把整文件当作新增；
- summary 里 `existed_before` 仍为 `true` 但 diff 与事实不符，误导模型与用户。

**建议：** 复用 `read.rs` 的 `detect_and_decode`（需提升可见性或抽到共享 helper）读取旧内容；并把探测到的编码用于 2.1 的行尾判断。至少在非 UTF-8 时不要假装"空文件"。

### 2.3 补齐 JSX 括号平衡检查 — 高优先级（低成本）

**现状：** `edit_file` 对 `.tsx/.jsx` 追加 `jsx_balance_warning`，`write_file` 没有。

**问题：** 整体新建 / 覆写 `.tsx` 恰恰是最容易写出括号不平衡的场景，却恰恰没有这层提示。

**建议：** 在返回前调用现成的 `jsx_balance_warning(&file_path, file_content)` 并拼接到 body，与 `edit_file` 行为对齐。改动量极小。

### 2.4 原子写入 — 中优先级

**现状：** `fs::write` 就地截断写。

**问题：** 中途失败（磁盘满、进程被杀、断电）会留下半截或空文件，直接破坏用户原文件——对一个"覆写整文件"的工具风险尤为突出。

**建议：** 改为"写同目录临时文件 + `fs::rename` 原子替换"。注意：

- 临时文件放在**目标同目录**，避免跨设备 rename 失败；
- Windows 上 `rename` 到已存在文件可能失败，需用 `fs::rename` 的覆盖语义或先删；
- 保留 `create_dir_all` 逻辑。

### 2.5 错误信息本地化 / 统一 — 中优先级

**现状：** `write.rs` 用英文 `"Failed to write {} : {}"`；`read.rs` / `edit.rs` 用带标签的中文诊断（`[NOT_FOUND]` / `[PERMISSION]`）。

**建议：** 抽取一个共享的 `map_write_io_error`（参照 `read.rs::map_plain_read_io_error`），按 `ErrorKind` 区分 `NotFound` / `PermissionDenied` / 其他，统一标签与中文文案，便于模型识别重试策略，也符合仓库默认中文的约定。

### 2.6 意外大幅截断保护 — 低优先级（可选）

**建议：** 当覆写已存在文件且**新内容字节数远小于原文件**（例如不足 20%）时，在 summary 追加一句醒目提示，降低"误把整文件覆写成一小段"的风险。偏产品取向，可选。

### 2.7 内容相同跳过写入 — 低优先级

**现状：** 即使内容与磁盘完全一致也会 `fs::write`，更新 mtime，可能触发文件 watcher / 重新编译。

**建议：** 在覆写且（归一化行尾后的）内容与 `prior_contents` 完全一致时，跳过 `fs::write`，直接返回 `(no changes)`。注意要在行尾归一化**之后**比较，否则纯行尾差异会被误判为"有变化"。

---

## 3. 写入大量代码的专项场景

当 `content` 是几百到几千行的整文件（典型如新建组件、生成样板代码）时，会触发一批和"小改"完全不同的瓶颈。这些是大写入场景下**最该先解决**的。

### 3.1 新建大文件 diff = 整文件副本，输出 token 翻倍 — 高优先级

**现状：** `make_unified_diff` 基于 `similar::TextDiff::from_lines`，新建文件时 `old == ""`：

```rust
// adapters/tools/diff_format.rs
let diff = TextDiff::from_lines(old, new); // old="" → 整份 new 全部成为 + 行
```

返回 body 形如 `{diff}\n{summary}`，于是**整份代码会被再复制一遍**（每行带 `+` 前缀）。一个 2000 行的新文件，工具结果里等于塞进 4000 行——直接挤占模型上下文，并触发 `LargeOutputRouter` 的压缩开销。

**建议：** 仅在**输入超阈值**（新内容或旧内容 > `DIFF_MAX_INPUT_BYTES`）时跳过完整 diff，改为 summary（行数 / 字节数）+ head 预览。**小的新建文件仍输出真实 unified diff**，否则会丢失前端体验——web-ui 的 `diffEntries.ts` 把 `write_file` 列入 `DIFF_TOOL_NAMES`，新建文件的 diff 会渲染成 `DiffCard` 并进入 Office「本轮变更」面板。

> **前端兼容约束：** 摘要 / 预览文本**不得以 `--- ` / `+++ ` / `@@` 开头**，否则会命中 `looksLikeDiff` 的 `/^--- /m` 正则被误当成 unified diff 渲染。实现里预览头用 `=== preview (head) ===`，每行带 `行号 | ` 前缀规避。

### 3.2 diff 计算成本随内容增大急剧上升 — 中优先级

**问题：** Myers diff 复杂度约 O(N·D)（D 为编辑距离）。覆写时若新旧内容差异很大，D 接近 N，大文件上计算明显变慢，纯属为了一段"反正全变了"的 diff 白白消耗 CPU。

**建议：** 设阈值（如内容 > 256 KB 或 > 5000 行）时**跳过或降级** diff，直接走 3.1 的摘要分支。阈值可复用 `file/mod.rs` 已有常量风格新增一个 `MAX_DIFF_INPUT_BYTES`。

### 3.3 写入字节上限缺失 — 中优先级

**现状：** `read_file` 有 `MAX_FILE_SIZE`（100 MB）守卫，`write_file` 没有任何上限。

**问题：** 失控生成（模型循环、超长粘贴）可能写出超大文件，且没有早停。

**建议：** 对 `content.len()` 加 guard，超过上限返回 `[TOO_LARGE]` 诊断错误，文案与 `read.rs` 对齐。

### 3.4 单次巨量生成被截断 → 落盘半截文件 — 高优先级

**问题（大写入最危险的一项）：** 模型把几千行塞进一个 `content` 参数时，容易因 `max_tokens` 被截断，于是把一个**语法损坏的半截文件**直接落盘覆写掉原文件。后续 `edit_file` 还可能因为找不到搜索串而连环失败。

**缓解（多管齐下）：**
1. **回显写入规模**：返回里明确给出写入的字节数 / 行数，让模型能自检"是不是比预期短"。
2. **扩展平衡检查作为截断信号**：现有 `check_jsx_balance` 只覆盖 `.tsx/.jsx`，可推广为通用的括号 / 引号平衡探测（`.rs`/`.ts`/`.json` 等），不平衡时给 `[TRUNCATION_SUSPECTED]` 提示——大括号严重不平衡常常就是被截断的信号。
3. **工具描述引导分块**：在 `description()` / 提示词里引导——超大文件优先「先 `write_file` 写骨架，再用 `edit_file` 的 `insert_after` 增量补全」，而不是一次性巨写。这能同时缓解 3.1 的输出膨胀。
4. **配合 2.4 原子写入**：写入窗口越长被打断概率越高，原子替换能保证截断/失败时不破坏原文件。

> 注：本仓库已有 `apply_patch` 等工具，对"大改但非全新"的场景，引导走补丁/增量路径通常比整文件覆写更稳，也更省 token。

---

## 4. 建议落地顺序

**通用正确性 / 一致性：**
1. **2.1 行尾保留** + **2.2 编码安全读旧内容**（合并实现，二者共享编码探测结果）
2. **2.3 JSX 检查**（顺手对齐，成本最低）
3. **2.4 原子写入**（健壮性，对大写入尤其重要）
4. **2.5 错误信息统一**
5. **2.6 / 2.7**（可选增强）

**大量代码写入（若大文件是主要负载，可提前到与上面并行推进）：**
1. **3.1 新建/大改不输出整文件 diff** + **3.2 大输入跳过 diff 计算**（合并实现，收益最大：省上下文又省 CPU）
2. **3.4 截断防护**（回显规模 + 平衡检查信号 + 工具描述引导分块）
3. **3.3 写入字节上限**

> 通用项里前两项解决正确性与 diff 噪声；大写入项里 3.1+3.2 直接决定整文件写入的开销与可用性，应优先。

---

## 5. 验证清单

每项改动后按 `rust-workspace` 规则执行：

```bash
cargo build
cargo test --workspace --all-features
cargo clippy --workspace --all-targets --all-features
```

建议在 `file/tests.rs` 中补充用例：

- 覆写 CRLF 文件后行尾仍为 CRLF；
- 覆写 GB18030 / UTF-16 文件时 diff 不再把全文件当新增；
- 新建 `.tsx` 且括号不平衡时返回 `[JSX_WARNING]`；
- 原子写入：写入失败不破坏原文件（可用只读目录或注入错误模拟）；
- 内容相同跳过写入时 mtime 不变；
- 新建大文件（> 阈值）时返回结构摘要而非整文件 diff，且 body 体积远小于 `content`；
- 超过写入字节上限的 `content` 返回 `[TOO_LARGE]`；
- 括号严重不平衡的内容触发 `[TRUNCATION_SUSPECTED]` / `[JSX_WARNING]`。

每次改动同步在 [CHANGELOG.md](../../CHANGELOG.md) 的 `[Unreleased]` 记录（仓库规则要求）。

---

## 6. 其它工具的同源加固（已实施）

`write_file` 暴露的几类问题在其它工具里同样存在。为统一行为，把 `tools/file` 内的 helper 提升为 `pub(crate)` 并共享复用：`normalize_line_endings`、`atomic_write`、`detect_and_decode`、`line_ending_of`。

### 6.1 `apply_patch`（`tools/apply_patch.rs`）

另一条主要写入路径，原本与旧 `write_file` 有完全相同的缺陷：

- **行尾丢失**：`base_content.lines().join("\n")` 把 CRLF 压成 LF。→ 用 `line_ending_of` 记录原行尾，写回前 `normalize_line_endings`。
- **末尾换行丢失**：`lines()` 吃掉结尾换行，补丁后产生 "No newline at end of file" 噪声。→ 记录 `had_trailing_newline`（新文件默认 `true`），写回时补回。
- **非原子写**：逐文件 `fs::write` + best-effort 回滚。→ 写入与回滚都改用 `atomic_write`。
- **`changes` 全量替换**：等价于 `write_file` 覆写，现同样保留原行尾 + 原子写。

> 注：补丁模式仍以 UTF-8 读取原文件（`read_to_string`）。**刻意不**对非 UTF-8 文件做 `detect_and_decode` 后打补丁——那会把文件转码成 UTF-8 写回、破坏原编码；保持「非 UTF-8 文件打补丁直接报错」更安全。

### 6.2 `grep_files`（`tools/search.rs`）

- **编码安全（准确性）**：原 `fs::read_to_string` 对 GB18030/UTF-16 源文件失败 → 被计入 `files_skipped_binary` 而搜不到。改用 `fs::read` + `detect_and_decode`，与 `read_file` 一致。中文 Windows 仓库尤其受益。
- **`files_with_matches` 提前停（效率）**：该模式只需判断文件是否命中，首行命中即 `break`，不再扫完整文件。
- **BM25 去重（效率）**：原 `dl = matches.iter().filter(...).count()` 在每个文件上重扫全部 matches（O(文件×匹配)）。改为预计算 `file_match_total` 哈希表后 O(1) 查表。

### 6.3 `list_dir`（`tools/file/list_dir.rs`）

- **稳定排序**：`read_dir` 顺序依赖文件系统、不可复现。改为**目录优先 + 名称升序**。
- **分页上限**：新增 `limit`（默认 1000、上限 10000），返回 `{ path, total, truncated, entries }`，避免超大目录灌爆上下文。
- **符号链接标识**：条目新增 `is_symlink` 字段。

### 6.4 暂未做（需单独评估）

- **`grep_files` 并行搜索**：当前单线程顺序 `read + regex`，大仓库慢于 ripgrep。并行化（如 rayon）收益明显但会引入依赖、改变结果时序，且与 BM25 重排交互需验证，作为独立改动评估。

### 6.5 验证

`apply_patch` / `grep_files` / `list_dir` 各补充回归测试：补丁保留 CRLF + 末尾换行；grep 命中 GB18030 文件；list_dir 目录优先 + 名称排序 + `total`/`truncated`。三组测试均通过。
