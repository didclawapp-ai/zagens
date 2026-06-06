---
name: office-resume
description: 简历与求职信（DOCX），默认输出到 deliverables/
---

# 简历 / 求职信

## 技能契约

> 约定层（§3.2）；引擎不解析。`verify` 供人工或 headless oracle 验收。

```yaml
id: office-resume
ingest:
  - kind: files
    formats: [docx, pdf]
    optional: true
  - kind: dictation
transform:
  - draft
render:
  format: docx
  sections: [联系方式, 摘要, 经历, 技能, 教育, 求职信]
  out: deliverables/
loop:
  brief_first: false
  confirm_before_render: false
  iterable: true
verify:
  - has_section: 经历
```

## 执行步骤

1. 确认：目标岗位、公司（可选）、语言、是否附求职信。
2. 用 `read_office` 读取用户提供的旧简历或 JD（若有）。
3. `write_office` 生成 `format: docx`：联系方式、摘要、经历（倒序）、技能、教育；求职信单独一节或第二份文档。**不必填 `path`**。
4. 增量修改：`load_office_payload` → 改 `blocks` → `write_office` 同路径覆盖。
