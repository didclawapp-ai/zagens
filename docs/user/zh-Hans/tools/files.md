# 文件工具

Agent 在**工作区根目录**内通过内置文件工具读写内容。

## 核心工具

| 工具 | 用途 |
|------|------|
| `read_file` | 读取文本或 Office/PDF（有大小限制） |
| `write_file` | 创建或覆盖文件 |
| `edit_file` | 定向搜索替换 |
| `apply_patch` | 统一 diff 补丁 |
| `list_dir` | 列出目录 |
| `file_info` | 元数据（大小、修改时间） |

## 搜索与发现

| 工具 | 代码模式 | 办公模式 |
|------|----------|----------|
| `glob_files` | ✅ | ✅ |
| `file_search` | ✅ | ✅ |
| `grep_files` | ✅（ripgrep） | ❌ |

**代码**推荐：`glob_files` → `grep_files` → `read_file`（见 [LHT](/zh-Hans/docs/code/lht)）。

**办公**仅 `glob_files` / `file_search`，不用 `grep_files`，也不通过 shell 跑 `grep`。

## 安全

- 路径规范化，`..` 逃逸会被拒绝。
- 写入可能触发[工具审批](/zh-Hans/docs/settings/approval)。
- 超大输出可能被截断或写入 scratchpad。

## 界面

变更会出现在 [Diff](/zh-Hans/docs/workspace/diff) 与[文件预览](/zh-Hans/docs/workspace/preview)。

相关：[Git 工具](/zh-Hans/docs/tools/git) · [办公 I/O](/zh-Hans/docs/tools/office-io)
