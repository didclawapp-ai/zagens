# Diff 审阅

Agent 修改代码时，工作区面板用 **diff2html** 展示**并排 diff**。

## 何时出现

常见触发：

- `edit_file` / `apply_patch` 工具结果
- 待确认的补丁提案

## 如何使用

1. 从文件树或工具卡片打开变更文件。
2. 在 diff 视图查看增删。
3. 按流程应用或拒绝（部分流程在审批后自动应用）。

## 办公模式

Diff 面向**代码**编辑。办公交付物多为 `deliverables/` 新文件 — 用文档预览即可。

## 建议

- 结合终端中的 `git status` 二次确认。
- 超大补丁可要求 Agent 拆成多次小改。

相关：[文件预览](/zh-Hans/docs/workspace/preview) · [代码模式](/zh-Hans/docs/code-mode)
