# Tauri 跨边界代码事实核查规则

当审查报告中的发现涉及 Rust 后端与 TypeScript 前端之间的 Tauri invoke 调用时：

## 1. 双侧行号要求
- 必须在同一条发现中同时提供：
  - Rust 侧：`#[tauri::command]` 的函数定义位置
  - TS 侧：`invoke('xxx')` 调用位置
- 缺少任一侧 → 该发现不通过核查

## 2. 命令名一致性
- Rust 侧命令名（`#[tauri::command] fn xxx`）与 TS 侧调用名（`invoke('xxx')`）
  必须完全一致（字符串匹配）
- Auditor 用 `read_file` 分别读取两侧，做字符串对比

## 3. 参数类型对应
- Rust `String` ↔ TS `string`
- Rust `Vec<T>` ↔ TS `T[]`
- Rust `Option<T>` ↔ TS `T | null`
- Rust `bool` ↔ TS `boolean`
- Rust `u32/i32` ↔ TS `number`
- 如果发现声称"类型不匹配"，必须引用具体的类型定义行

## 4. 命令注册验证
- `main.rs` 的 `.invoke_handler()` 中必须包含该命令
- 如果发现声称"命令未注册"，必须引用 `main.rs` 中 `invoke_handler` 的行号
