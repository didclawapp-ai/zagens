# Rust 代码事实核查规则

审查报告中引用 Rust 源代码时，以下条件必须满足：

## 1. 文件路径
- 必须以 `.rs` 结尾
- 必须为仓库相对路径（如 `crates/tui/src/tools/subagent/mod.rs`）

## 2. 行号
- 必须为具体数字或数字范围（如 `:42` 或 `:120-145`）
- 不允许 "around line"、"approximately" 等模糊表述

## 3. 内容匹配（机械检查 — 字符串包含）
- `read_file` 读取对应行
- 该行内容必须在以下类别之一内：
  - `fn` / `pub fn` / `async fn` 定义
  - `struct` / `enum` / `impl` / `trait` 定义
  - 表达式（`unwrap()` / `expect()` / `unsafe { }` 块）
  - 错误处理路径（`?` 操作符、`match Err`、`if let Err`）
  - `#[tauri::command]` 注解（桌面命令）
  - `pub(crate)` / `pub` 可见性声明

## 4. 涉及编译或运行时行为
- 如果发现声称"会 panic"、"会编译失败"、"会死锁"
- 核查只验证代码引用的位置存在对应代码
- Auditor 不做编译或运行时验证

## 5. 涉及"A 改为 B"
- 发现必须引用改动前后的具体行号
- Auditor 用 `read_file` 验证改动后在引用行中确实为 B
- 如果只说了"应改为 B"而没有引用改动位置 → 内容不符
