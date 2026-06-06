# PPTX 场景模板库

预设 JSON 模板，覆盖常用职场/报告场景。用户只需替换 `{{...}}` 标记的占位数据字段，即可生成完整演示文稿。

## 使用方式

```bash
python write_pptx.py --input templates/8d_report.json --output 8D报告.pptx
```

或在 Zagens 中直接让模型基于模板生成：模型会读取模板 JSON，替换占位数据后调用 `write_office`。

## 模板列表

| 模板 | 文件 | 页数 | 说明 |
|------|------|------|------|
| 8D 报告 | `8d_report.json` | 8页 | 含鱼骨图(mpl)、柏拉图(combo)、对比表、合并单元格 |
| 项目周报 | `weekly_report.json` | 6页 | 含甘特图(mpl)、KPI双图、风险表、里程碑 |
| 竞品分析 | `competitive_analysis.json` | 1页 | 左文右图、富文本关键数字高亮 |
| 年度总结 | `annual_summary.json` | 6页 | 封面+业绩看板+成就时间线+财务+展望 |
| 项目提案 | `project_proposal.json` | 6页 | 封面+问题陈述+方案+甘特图+预算+团队 |
| 销售汇报 | `sales_report.json` | 5页 | 封面+漏斗图(mpl)+地区业绩+产品排名+目标 |

## 字段说明

所有模板遵循 `pptx_engine` JSON schema（参见 `docs/pptx-generation-engine-plan.md`）：

- `{{PLACEHOLDER}}` — 替换为实际数据
- `"type": "mpl"` 块 — 需要 matplotlib 环境（可选，缺失时静默跳过）
- `"merges"` — 表格合并单元格 `[[r1,c1,r2,c2], ...]`
- `"layout"` — 栅格分栏 `{"kind":"grid","cols":[0.4,0.6],"gap":"0.2in"}`

## 依赖

- **必需**: `python-pptx`（所有模板）
- **可选**: `matplotlib`（使用 mpl 图表的模板，缺失时图表跳过但不影响其余内容）
