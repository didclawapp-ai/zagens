# 文件树

**工作台**侧栏打开工作区**文件树**，浏览 Agent 可读写的目录与文件。

## 打开方式

侧栏 → **工作台**（Workspace）。树与当前会话绑定的工作区根目录同步。

## 代码模式

- 默认展示仓库根下全部可见文件
- 点击文件在右栏[预览](/zh-Hans/docs/workspace/preview)
- 右键或菜单可用系统默认应用打开（若已安装）

## 办公模式

办公会话提供快捷筛选：

| 预设 | 内容 |
|------|------|
| **全部** | 工作区根目录 |
| **交付物** | `deliverables/` — Agent 输出的 DOCX / XLSX 等 |
| **文档** | 常见文档与附件目录 |
| **变更** | 近期有改动的文件 |

`write_office` 完成后，树会聚焦并高亮新文件。详见[交付物](/zh-Hans/docs/office/deliverables)。

## 与 Agent 的关系

文件树是**只读浏览**；创建、修改、删除由 Agent 通过工具完成（`read_file`、`write_office` 等），不是完整 IDE 编辑器。

## 建议

- 办公演示：先建好 `inbox/`、`data/`，或复制 fixtures — 目录**不会**自动初始化
- 大仓库：首条消息说明关注的子目录，减少无关扫描

相关：[工作区概览](/zh-Hans/docs/workspace/overview) · [文件预览](/zh-Hans/docs/workspace/preview) · [办公工作区](/zh-Hans/docs/office/workspace)
