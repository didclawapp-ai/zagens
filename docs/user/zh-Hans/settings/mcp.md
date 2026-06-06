# MCP 服务器

**MCP**（Model Context Protocol）通过 stdio 服务把外部工具与数据源接入 Agent。

## MCP 面板

**设置 → MCP** 可：

- 添加/编辑 MCP 服务配置
- 按工具名启用/禁用
- 配置 allow/deny 过滤

更改在后续回合生效。

## 典型用途

- 封装内部 API、数据库
- 第三方 MCP 集成
- 补充内置 `web_search`、文件工具

## 安全

MCP 工具同样受**审批**与**网络策略**约束（如适用）。启用前请审阅服务来源。

## 任务类型

代码与办公均可配置 MCP；办公模式仍会收束工程向内置工具。

相关：[技能管理](/zh-Hans/docs/settings/skills) · [工具审批](/zh-Hans/docs/settings/approval)
