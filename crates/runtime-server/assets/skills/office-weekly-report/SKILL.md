---
name: office-weekly-report
description: 撰写周报（DOCX/XLSX），默认输出到 deliverables/
---

# 周报

## 技能契约

> 约定层（§3.2）；引擎不解析。`verify` 供人工或 headless oracle 验收。

```yaml
id: office-weekly-report
ingest:
  - kind: files
    formats: [docx, xlsx, pdf]
  - kind: dictation
transform:
  - summarize
  - draft
render:
  format: docx
  sections: [本周完成, 下周计划, 风险与阻塞]
  out: deliverables/
loop:
  brief_first: false
  confirm_before_render: false
  iterable: true
verify:
  - has_section: 本周完成
```

## 执行步骤

1. 向用户确认：时间范围、汇报对象、本周完成 / 下周计划 / 风险（可简短默认）。
2. 用 `read_office` 读取用户提供的 Excel/附件（若有）。
3. `write_office` 生成 `format: docx`，`title` 含周次；**不必填 `path`**（自动 `deliverables/`）。
4. 增量修改：`load_office_payload` → 改 `blocks` → `write_office` 同路径覆盖。
