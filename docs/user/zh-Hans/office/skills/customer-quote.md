# 客户报价单

**技能：** `office-customer-quote` · **输出：** 含含税合计的 XLSX

## 作用

根据 `data/` 中的价目表与需求说明生成客户报价，计算行项目与**含税总额**。

## 开始前

- 任务类型：**办公**
- 在 `data/` 放入价目表与需求（如 `价目表.csv`、`客户需求.md`）
- 演示文件见 `docs/harness/fixtures/office-demo/`

## 如何运行

1. 点击**客户报价单**或输入：
   > 根据 data/价目表.csv 与 data/客户需求.md 做报价，含含税合计。
2. 按需回答币种、税率、折扣等问题。
3. 在 `deliverables/` 打开 XLSX。

## 典型内容

报价头 · 行项目 · 小计 · 税额 · 合计 · 备注

## 验收

- Excel 中合计可复核
- SKU/单价与价目表一致

**完整 P0 示范：** [P0-4 客户报价](/zh-Hans/docs/office/p0-quote)

相关：[办公工作区](/zh-Hans/docs/office/workspace) · [技能索引](/zh-Hans/docs/office/skills)
