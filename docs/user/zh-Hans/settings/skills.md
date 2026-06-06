# 技能管理

**技能**是可复用规程（`SKILL.md`），由 Agent 通过 `load_skill` 加载 — 办公流程、审计、自定义 SOP。

## 内置办公技能

Runtime 自带 bundled 技能（如 `office-weekly-report`、`office-customer-quote`）。办公空态卡片与之对应。

见 [内置技能索引](/zh-Hans/docs/office/skills) 与各 [P0 示范](/zh-Hans/docs/office/scenarios)。

## 用户技能目录

自定义技能路径：`~/.zagens/skills/<名称>/SKILL.md`。

## 桌面界面

- 侧栏 **任务** — 浏览后台任务；创建 / 导入 / 安装技能
- **设置 → 技能** — 同一技能管理面板（`AutomationPanel`）

**说明：** 定时自动化（`GET /v1/automations`）API 仍保留，但 UI **暂不展示**自动化列表。

## 编写建议

- 步骤编号清晰，写明输入目录（`inbox/`、`data/`）。
- 声明输出格式（DOCX、XLSX）与 `deliverables/`。
- 先用小 fixtures 试跑再上生产数据。

## skill-creator

内置 `skill-creator` 技能可交互式起草新技能。

相关：[MCP](/zh-Hans/docs/settings/mcp) · [技能索引](/zh-Hans/docs/office/skills) · [办公工作区](/zh-Hans/docs/office/workspace)
