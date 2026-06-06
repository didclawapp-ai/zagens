# 内置办公技能

Zagens 内置 **11 个办公技能**，与办公空态任务卡片一一对应，运行时通过 `load_skill` 加载 `SKILL.md`。

## 技能索引

| 技能 ID | 卡片（中文） | 输出 | 指南 |
|---------|-------------|------|------|
| `office-competitive-analysis` | 竞品分析 | DOCX | [技能](/zh-Hans/docs/office/skills/competitive) · [P0-1](/zh-Hans/docs/office/p0-competitive) |
| `office-executive-daily-brief` | 经营日报汇总 | DOCX | [技能](/zh-Hans/docs/office/skills/executive-daily) · [P0-2](/zh-Hans/docs/office/p0-executive) |
| `office-production-daily-report` | 生产品质晨报 | DOCX | [技能](/zh-Hans/docs/office/skills/production-daily) · [P0-3](/zh-Hans/docs/office/p0-production) |
| `office-customer-quote` | 客户报价单 | XLSX | [技能](/zh-Hans/docs/office/skills/customer-quote) · [P0-4](/zh-Hans/docs/office/p0-quote) |
| `office-weekly-report` | 周报 | DOCX | [周报](/zh-Hans/docs/office/skills/weekly-report) |
| `office-meeting-minutes` | 会议纪要 | DOCX | [会议纪要](/zh-Hans/docs/office/skills/meeting-minutes) |
| `office-project-report` | 项目汇报 PPT | PPTX | [项目汇报](/zh-Hans/docs/office/skills/project-report) |
| `office-data-report` | 数据报表 | XLSX | [数据报表](/zh-Hans/docs/office/skills/data-report) |
| `office-contract-draft` | 合同初稿 | DOCX | [合同初稿](/zh-Hans/docs/office/skills/contract-draft) |
| `office-resume` | 简历 / 求职信 | DOCX | [简历](/zh-Hans/docs/office/skills/resume) |
| `office-release-notes` | 发布说明 | DOCX | [发布说明](/zh-Hans/docs/office/skills/release-notes) |

## 运行方式

1. 任务类型选**办公**。
2. 准备[办公工作区](/zh-Hans/docs/office/workspace)（`inbox/`、`data/`）。
3. 点击卡片或描述目标（如「整理今天会议纪要」）。
4. Agent 执行 `load_skill`，再按需 `read_office` / `web_search` / `write_office`。
5. 结果在 `deliverables/`。

## 自定义技能

可放在 `~/.zagens/skills/` — 见[技能管理](/zh-Hans/docs/settings/skills)。

## 边界

- 技能交付**文件**；可先文字概况再出 DOCX（部分技能契约要求）。
- 重构、shell 自动化 → **代码**模式 + [LHT](/zh-Hans/docs/code/lht)。
