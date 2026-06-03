---
name: office-data-report
description: 从数据生成 Excel 报表（XLSX），默认输出到 deliverables/
---

# 数据报表

1. 用 `read_office` 读取用户提供的 CSV/XLSX（大表用 `start_row`/`limit` 分页）；在回复中归纳要点，**不要把整表重抄进 JSON**。
2. 确认：报表主题、关键指标、是否需要图表 sheet。
3. `write_office` 生成 `format: xlsx`：`sheets` 含表头、数据区、可选图表。**不必填 `path`**。
4. 增量修改：`load_office_payload` → 改 `sheets` → `write_office` 同路径覆盖。
