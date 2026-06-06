---
name: office-contract-draft
description: 合同/协议初稿（DOCX），默认输出到 deliverables/
---

# 合同初稿

## 技能契约

> 约定层（§3.2）；引擎不解析。`verify` 供人工或 headless oracle 验收。

```yaml
id: office-contract-draft
ingest:
  - kind: files
    formats: [docx, pdf]
    optional: true
  - kind: dictation
transform:
  - draft
render:
  format: docx
  sections: [当事人, 标的, 权利义务, 付款, 违约, 争议解决, 签署页]
  out: deliverables/
loop:
  brief_first: false
  confirm_before_render: true
  iterable: true
verify:
  - has_section: 当事人
  - disclaimer: legal_review
```

## 执行步骤

1. 确认：合同类型、甲乙双方、标的、期限、适用法律或管辖区（可提示用户补充）。
2. 用 `read_office` 读取用户提供的范本或条款清单（若有）。
3. `write_office` 生成 `format: docx`：标题、当事人、定义、权利义务、付款、违约、争议解决、签署页。**不必填 `path`**。
4. **免责声明**：输出为初稿模板，非法律意见；建议法务审阅。
5. 增量修改：`load_office_payload` → 改 `blocks` → `write_office` 同路径覆盖。
