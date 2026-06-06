# P0-2：经营日报汇总

**技能：** `office-executive-daily-brief` · **输出：** 含待决事项的 DOCX

## 做什么

将 `inbox/` 中多部门材料汇总为一份经营层日报，包含风险与**待决事项**。

## 准备工作

- 任务类型：**办公**
- 将各部门简报放入 `inbox/`（可用 `docs/harness/fixtures/office-demo/` 或[应用场景](/zh-Hans/use-cases) fixtures）
- 支持 DOCX、XLSX、PDF、Markdown

## 如何运行

1. 确认 `inbox/` 中有昨日各部门文件。
2. 空态点击 **经营日报汇总**，或说明：
   > 汇总昨日 inbox 多源简报给经营层，列出待决事项。
3. 先阅读 Agent 给出的**文字概况** — 技能会在生成正式 DOCX 前征求确认。
4. 确认后文档写入 `deliverables/`。

## 典型章节

概况 · 各部门要点 · 风险 · **待决事项** · 附录

## 验收

- 数字与附件一致，不编造
- 含「待决事项」章节

相关：[全部 P0 示范](/zh-Hans/docs/office/scenarios)
