# 文件预览

在工作区文件树中点击文件，可在右栏以**只读预览**打开（视格式而定）。

## 支持的预览

| 类型 | 行为 |
|------|------|
| **代码与 Markdown** | 带语法风格的文本视图 |
| **图片** | 内嵌显示 |
| **CSV** | 表格式视图 |
| **Office 与 PDF** | 提取文本预览（DOCX、XLSX、PPTX、PDF） |
| **Mermaid** | 识别后渲染图表 |
| **二进制** | 未知格式显示十六进制片段 |

Office 文件也可从工作区 UI 用系统默认应用打开。

## Diff 审阅

`edit_file` 或 `apply_patch` 之后，Zagens 用 **diff2html** 展示变更，便于确认后再应用。

## 交付物（办公）

`write_office` 生成的文件通常在 `deliverables/`。任务完成后预览面板会高亮新输出。详见[交付物](/zh-Hans/docs/office/deliverables)。

## 限制

- 预览用于查看；编辑通过 Agent 工具完成，不是完整 IDE。
- 超大文件可能截断预览；可让 Agent 只读指定章节。

相关：[工作区概览](/zh-Hans/docs/workspace/overview) · [交付物](/zh-Hans/docs/office/deliverables)
