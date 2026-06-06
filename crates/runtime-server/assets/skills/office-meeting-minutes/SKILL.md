---
name: office-meeting-minutes
description: 整理会议纪要（DOCX），默认输出到 deliverables/
---

# 会议纪要

## 技能契约

> 约定层（§3.2）；引擎不解析。`verify` 供人工或 headless oracle 验收。

```yaml
id: office-meeting-minutes
ingest:
  - kind: files
    formats: [docx, pdf, md]
  - kind: dictation
transform:
  - summarize
  - extract: action_items
render:
  format: docx
  sections: [基本信息, 议题讨论, 决议, 行动项]
  out: deliverables/
loop:
  brief_first: false
  confirm_before_render: false
  iterable: true
verify:
  - has_section: 行动项
```

## 执行步骤

1. 确认：会议时间、参会人、议题列表（可简短默认）。
2. 用 `read_office` 读取用户提供的会议材料或录音整理稿（若有）。
3. `write_office` 生成 `format: docx`：`blocks` 含标题、参会人、议题讨论、决议、行动项（负责人/截止日期）。**不必填 `path`**。
4. 增量修改：`load_office_payload` → 改 `blocks` → `write_office` 同路径覆盖。
