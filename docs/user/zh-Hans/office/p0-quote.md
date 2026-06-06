# P0-4：客户报价单

**技能：** `office-customer-quote` · **输出：** 含税合计的 XLSX

## 做什么

根据**价目表**与**客户需求**生成报价表 — 行项、数量、税额与合计。

## 准备工作

- 任务类型：**办公**
- 将价目表 CSV/XLSX 放入 `data/`
- 将客户需求放入 `data/` 或 `inbox/`（演示：`客户需求.md`）

## 如何运行

1. 将 `docs/harness/fixtures/office-demo/` 的 `data/` 复制到工作区（或从[应用场景](/zh-Hans/use-cases)下载 zip）。
2. 空态点击 **客户报价单**，或说明：
   > 根据价目表和客户需求生成报价 XLSX，含税额与合计。
3. 在 Excel 中打开 `deliverables/` 下的 XLSX，核对行项与公式。

## 典型输出

品项、单价、数量、小计、税额、**合计**

## 验收

- 价格来自上传价目表，不编造 SKU
- 税额与合计一致

相关：[全部 P0 示范](/zh-Hans/docs/office/scenarios) · [办公工作区](/zh-Hans/docs/office/workspace)
