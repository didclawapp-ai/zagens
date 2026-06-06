# 交付物预览与打开

办公模式的最终产出在 **`deliverables/`** — Agent 通过 `write_office` 写入 DOCX、XLSX、PPTX 或 PDF。

## 目录何时出现

`deliverables/` **不会**在工作区初始化时自动建好；**首次 `write_office` 写入时**由 runtime 创建。可手动预先创建空目录。

## 在工作区查看

1. 侧栏 → **工作台**
2. 办公预设选 **交付物**，或展开 `deliverables/` 节点
3. 点击文件 — 右栏显示[提取文本预览](/zh-Hans/docs/workspace/preview)（DOCX / XLSX / PPTX / PDF）

任务完成后，新文件常会**高亮**便于定位。

## 用系统应用打开

需要完整版式、公式或动画时：

- 文件树上下文菜单 → **用系统应用打开**（调用 `open_with_system_app`）
- 或在预览区使用相同入口

Word / Excel / PowerPoint 会按本机默认程序启动。

## 增量修改

Agent 可用 `load_office_payload` 读取已生成文件的缓存结构，改 sheets / blocks / slides 后 **`write_office` 同路径覆盖**。用户可在预览确认后再让 Agent 迭代。

## 验收建议

- 数字与源表、联网来源一致（见各 [P0 示范](/zh-Hans/docs/office/scenarios)）
- 复杂 Excel 公式建议在 Excel 中复核

相关：[办公工作区](/zh-Hans/docs/office/workspace) · [办公 I/O](/zh-Hans/docs/tools/office-io) · [文件树](/zh-Hans/docs/workspace/file-tree)
