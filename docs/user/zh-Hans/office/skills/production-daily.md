# 生产/品质晨报

**技能：** `office-production-daily-report` · **输出：** DOCX 晨报

## 作用

读取 `data/` 中的生产与品质数据，汇总昨日状态，生成面向车间或管理的晨报 DOCX。

## 开始前

- 任务类型：**办公**
- 表格放在 `data/`（如演示包中的 `生产日报_昨日.xlsx`）
- `inbox/` 非标准输入路径；对比材料可放 `data/` 旁或单独说明

## 如何运行

1. 点击**生产品质晨报**或输入：
   > 根据昨日生产与品质数据写一份 DOCX 晨报。
2. 若技能要求确认，先审阅**文字简报**。
3. 确认后生成正式 DOCX 至 `deliverables/`。

## 典型章节

概况 · 生产指标 · 品质指标 · 异常与风险 · 待确认事项

## 验收

- KPI 与 `data/` 源表一致
- 提及 **OEE / 良率** 等关键指标
- 异常项非编造

**完整 P0 示范：** [P0-3 生产/品质晨报](/zh-Hans/docs/office/p0-production)

相关：[数据报表技能](/zh-Hans/docs/office/skills/data-report) · [办公 I/O](/zh-Hans/docs/tools/office-io)
