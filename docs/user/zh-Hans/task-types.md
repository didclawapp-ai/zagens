# 任务类型：代码 vs 办公

Zagens 用 **任务类型** 区分工程向与文档向工作流。启动新会话时选择 **代码** 或 **办公**，会加载不同的系统提示与工具面。

## 何时选哪种

| 类型 | 适合 |
|------|------|
| **代码** | 改仓库、跑测试、终端、diff、符号索引、LHT / CRAFT、子代理 |
| **办公** | 读表、联网调研、写 DOCX / XLSX / PPTX / PDF，交付到 `deliverables/` |

一般聊天与文档创作归入 **办公**；纯代码审查、重构、调试选 **代码**。

## 切换规则

**切换任务类型会开启新会话** — 不在同一会话内混用，以保持模型上下文前缀稳定。

在 Composer 或新会话流程中重新选择类型即可。

## 工具差异（概要）

**代码**模式典型工具：`grep_files`、`exec_shell`、Git、`edit_file` / `apply_patch`、符号索引、子代理等。

**办公**模式保留：`read_office`、`write_office`、`load_office_payload`、`glob_files`、`file_search`、`load_skill`、可选联网与 `describe_image`。**不提供** shell 与 patch 类工程工具。

详见 [Agent 工具](/zh-Hans/docs/tools/files) 与 [办公 I/O](/zh-Hans/docs/tools/office-io)。

## 设置差异

办公会话侧栏会隐藏 **路由**、**话题记忆**、**符号索引**、**LHT 设置** 等代码向入口；用量、MCP、技能、API Key 仍可用。

## 下一步

| 目标 | 文档 |
|------|------|
| 工程开发 | [代码模式](/zh-Hans/docs/code-mode) · [工作区](/zh-Hans/docs/workspace/overview) |
| 文档办公 | [办公模式](/zh-Hans/docs/office/overview) · [办公工作区](/zh-Hans/docs/office/workspace) |
| 界面入口 | [界面导览](/zh-Hans/docs/ui-tour) |
