---
name: office-data-report
description: 从数据生成 Excel 报表（XLSX），默认输出到 deliverables/
---

# 数据报表

## 技能契约

> 约定层（§3.2）；引擎不解析。`verify` 供人工或 headless oracle 验收。

```yaml
id: office-data-report
ingest:
  - kind: files
    from: data/
    formats: [csv, xlsx]
transform:
  - compute
  - summarize
render:
  format: xlsx
  sheets: [数据明细, 汇总]
  out: deliverables/
loop:
  brief_first: false
  confirm_before_render: false
  iterable: true
verify:
  - has_sheet: 数据明细
```

## 执行步骤

1. 用 `read_office` 读取用户提供的 CSV/XLSX（大表用 `start_row`/`limit` 分页）；在回复中归纳要点，**不要把整表重抄进 JSON**。
2. 确认：报表主题、关键指标、是否需要图表 sheet。
3. `write_office` 生成 `format: xlsx`：`sheets` 含表头、数据区、可选图表。**不必填 `path`**。
4. 增量修改：`load_office_payload` → 改 `sheets` → `write_office` 同路径覆盖。
