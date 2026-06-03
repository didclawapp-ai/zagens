---
name: office-competitive-analysis
description: 竞品分析报告（DOCX），含调研来源，默认 deliverables/
---

# 竞品分析

1. 确认：分析对象、对比维度、时间范围。
2. 用 `web_search` / `fetch_url` 收集公开信息；**文末列出来源**（标题 + URL + 访问日期）。
3. `write_office` 生成 `format: docx`：摘要、竞品对比表、优劣势、建议。**不必填 `path`**。
4. 增量修改：`load_office_payload` → 改 `blocks` → `write_office` 同路径覆盖。
