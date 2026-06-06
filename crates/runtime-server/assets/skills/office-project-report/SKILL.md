---
name: office-project-report
description: 制作项目汇报 PPT（PPTX），默认输出到 deliverables/
---

# 项目汇报 PPT

## 技能契约

> 约定层（§3.2）；引擎不解析。`verify` 供人工或 headless oracle 验收。

```yaml
id: office-project-report
ingest:
  - kind: files
    formats: [docx, pptx, md]
  - kind: dictation
transform:
  - draft
  - summarize
render:
  format: pptx
  slides: [封面, 进展要点, 风险与缓解, 下一步计划]
  out: deliverables/
loop:
  brief_first: false
  confirm_before_render: false
  iterable: true
verify:
  - has_slide: 封面
```

## 执行步骤

1. 确认：项目名称、汇报人、听众、汇报日期。
2. `write_office` 生成 `format: pptx`：`slides` 至少含封面（项目名+汇报人）、进展要点、风险与缓解、下一步计划。**不必填 `path`**。
3. 增量修改：`load_office_payload` → 改 `slides` → `write_office` 同路径覆盖。
