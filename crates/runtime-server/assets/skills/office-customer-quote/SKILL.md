---
name: office-customer-quote
description: 客户报价单（XLSX），价目表 + 需求计算，默认 deliverables/
---

# 客户报价单

## 技能契约

> 约定层（§3.2）；引擎不解析。`verify` 供人工或 headless oracle 验收。

```yaml
id: office-customer-quote
ingest:
  - kind: files
    from: data/
    formats: [xlsx, csv]
  - kind: dictation
    notes: 客户需求（数量、型号、折扣）
transform:
  - compute
  - extract: line_items
render:
  format: xlsx
  sheets: [报价明细, 汇总]
  out: deliverables/
loop:
  brief_first: false
  confirm_before_render: false
  iterable: true
verify:
  - has_sheet: 报价明细
  - has_column: 含税合计
  - no_fabricated_prices
```

## 执行步骤

1. 确认：客户名称、报价有效期（默认 30 天）、税率（默认 13%）、是否含运费。
2. 用 `read_office` 读取 `data/价目表.csv` 或 `data/价目表.xlsx`（若用户未指定则 `list_dir data/` 查找价目表）。
3. 根据用户需求匹配产品行、计算数量 × 单价、折扣、含税合计；**不得编造价目表中不存在的单价**。
4. `write_office` 生成 `format: xlsx`，`title` 含客户名与日期；**不必填 `path`**（自动 `deliverables/`）。
5. 增量改价：`load_office_payload` → 改 `sheets` 对应行 → `write_office` 同路径覆盖。

## 演示数据

见 `fixtures/harness/office-demo/data/`（复制到工作区 `data/` 后试用）。
