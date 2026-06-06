# 经营日报汇总

**技能：** `office-executive-daily-brief` · **输出：** 含待决事项的 DOCX

## 作用

汇总 `inbox/` 中各部门简报，生成面向管理层的摘要，含风险与**待决事项**。

## 开始前

- 任务类型：**办公**
- 将部门简报放入 `inbox/`（DOCX、XLSX、PDF、Markdown）
- 演示数据：`docs/harness/fixtures/office-demo/` 或[用例页](/zh-Hans/use-cases) zip

## 如何运行

1. 确保 `inbox/` 有昨日各部门文件。
2. 点击**经营日报汇总**或输入：
   > 汇总昨日 inbox 简报给管理层，列出待决事项。
3. **先阅读文字概况** — 技能会**先输出要点摘要，再询问是否生成正式 DOCX**（`confirm_before_render`）。
4. 确认后写入 `deliverables/`。

## 典型章节

总览 · 部门要点 · 风险 · **待决事项** · 附录

## 验收

- 数字与附件一致
- 含「待决事项」章节

**完整 P0 示范：** [P0-2 经营日报](/zh-Hans/docs/office/p0-executive)

相关：[办公工作区](/zh-Hans/docs/office/workspace) · [技能索引](/zh-Hans/docs/office/skills)
