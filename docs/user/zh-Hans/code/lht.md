# LHT 长程任务

**LHT**（long-horizon task）帮助**代码模式**跨多轮完成**多步骤工程任务**而不丢主线。

## 提供什么

- 模型使用 `checklist_write` 时的**清单**侧栏
- **长程面板**：宏/微进度与周期交接
- 上下文压力升高时的**提前换周期**提示

适合重构、测试清扫、审计修复等 — 不适合一句话问答。

## 何时用 LHT

| 适合 | 不适合 |
|------|--------|
| 多文件重构并验证 | 单文件小改 |
| CRAFT 审查循环 | 办公 DOCX 交付 |
| 数小时引导实现 | 简单联网查询 |

## 设置

**设置 → LHT 配置** 与面板四段一一对应：Harness 预置、长程 harness、完成门禁、宏观审查循环 — 完整字段说明见 **[LHT 设置](/zh-Hans/docs/settings/lht)**（含 Composer 三态覆盖关系、默认值、禁用条件与 `config.toml` 对照）。

编写器上方 **LHT** 芯片可循环 **LHT → LHT·严格 → LHT·关**（按 turn 覆盖，写入 `~/.zagens/settings.toml`）。

## 界面位置

开启**审计网格**可同时看清单、[审计 Scratchpad](/zh-Hans/docs/code/audit-scratchpad)、LHT 图与子代理（[界面导览](/zh-Hans/docs/ui-tour)）。标题栏按钮 **显示/隐藏审计网格** 在代码会话有 harness 数据时出现。

相关：[CRAFT](/zh-Hans/docs/code/craft) · [上下文](/zh-Hans/docs/chat/context) · [代码模式](/zh-Hans/docs/code-mode)
