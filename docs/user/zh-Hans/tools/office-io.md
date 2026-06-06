# 办公 I/O

办公交付物主要通过 **`read_office`** 与 **`write_office`** 生成（部分格式也可用 `read_file`）。

## `read_office`

从以下格式提取文本/表格：

- Word（`.docx`）
- Excel（`.xlsx`）
- PowerPoint（`.pptx`）— 幻灯片文字
- PDF — 视配置启用

用于 [办公工作区](/zh-Hans/docs/office/workspace) 下 `inbox/`、`data/` 中的文件。

## `write_office`

创建或更新结构化 Office 文件：

| 格式 | 典型产出 |
|------|----------|
| DOCX | 报告、简报、纪要 |
| XLSX | 报价、数据表 |
| PPTX | 演示文稿 |

输出在 `deliverables/`，可从办公侧栏预览并[系统打开](/zh-Hans/docs/office/deliverables)。

## `load_office_payload`

对已生成的 Office 文件，runtime 可缓存结构化 JSON。**增量编辑**流程：

1. `load_office_payload` — 取缓存的 sheets / blocks / slides
2. 修改结构
3. `write_office` — **同路径覆盖**

适合「改一列报价」「补一段纪要」等迭代，无需从零重写。

## 其他办公工具

办公模式还提供 `list_dir`、`write_file`（纯文本）、`glob_files`、`file_search` 等辅助工具；核心交付仍靠 `read_office` / `write_office`。

## 技能

内置 [办公技能](/zh-Hans/docs/office/skills) 在 `load_skill` 后调用上述工具。P0 指南：[竞品](/zh-Hans/docs/office/p0-competitive)、[经营日报](/zh-Hans/docs/office/p0-executive)、[生产晨报](/zh-Hans/docs/office/p0-production)、[客户报价](/zh-Hans/docs/office/p0-quote)。

## 限制

- 复杂 Excel 公式建议在 Excel 中复核。
- 品牌/模板宜放在 `data/` 作参考。
- 工程类任务请切代码模式，用文件工具而非办公 I/O。

相关：[网络工具](/zh-Hans/docs/tools/web) · [文件工具](/zh-Hans/docs/tools/files)
