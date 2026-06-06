---
name: office-competitive-analysis
description: 竞品分析报告（DOCX），含调研来源，默认 deliverables/
---

# 竞品分析

## 技能契约

> 约定层（§3.2）；引擎不解析。`verify` 供人工或 headless oracle 验收。

```yaml
id: office-competitive-analysis
ingest:
  - kind: web
  - kind: files
    formats: [docx, md]
    optional: true
transform:
  - summarize
  - compare
render:
  format: docx
  sections: [摘要, 竞品对比, 优劣势, 建议, 来源]
  out: deliverables/
loop:
  brief_first: false
  confirm_before_render: false
  iterable: true
verify:
  - sources_cited
  - has_section: 来源
```

## 执行步骤

1. 确认：分析对象、对比维度、时间范围。
2. 用 `web_search` / `fetch_url` 收集公开信息；**文末列出来源**（标题 + URL + 访问日期）。
3. `write_office` 生成 `format: docx`：摘要、竞品对比表、优劣势、建议。**不必填 `path`**。
4. 增量修改：`load_office_payload` → 改 `blocks` → `write_office` 同路径覆盖。
