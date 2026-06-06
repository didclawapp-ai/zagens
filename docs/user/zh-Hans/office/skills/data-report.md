# 数据报表

**技能：** `office-data-report` · **输出：** XLSX

## 作用

读取 `data/` 中的 CSV/XLSX，计算汇总并生成含明细与汇总工作表的 Excel 报表。

## 开始前

- 任务类型：**办公**
- 源表放在 `data/`
- 大表：Agent 可能分页读取 — 在对话归纳要点，勿整表重抄

## 如何运行

1. 点击**数据报表**或输入：
   > 根据 data/sales_q1.csv 做 Excel 报表，含明细与汇总表。
2. 确认：报表主题、关键指标、是否需要图表页。
3. XLSX 出现在 `deliverables/`。

## 典型工作表

**数据明细** · **汇总** · 可选图表页

## 验收

- 行数、合计与源数据一致
- 表头与单位标注清晰

## 建议

- 运营类数据可配合[生产晨报](/zh-Hans/docs/office/skills/production-daily)。
- 复杂模型请在 Excel 中复核公式。

相关：[办公 I/O](/zh-Hans/docs/tools/office-io) · [办公工作区](/zh-Hans/docs/office/workspace)
