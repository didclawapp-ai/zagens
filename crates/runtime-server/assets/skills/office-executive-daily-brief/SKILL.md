---
name: office-executive-daily-brief
description: 经营日报汇总（DOCX），多部门 inbox 附件聚合，默认 deliverables/
---

# 经营日报汇总

## 技能契约

> 约定层（§3.2）；引擎不解析。`verify` 供人工或 headless oracle 验收。

```yaml
id: office-executive-daily-brief
ingest:
  - kind: files
    from: inbox/
    formats: [docx, xlsx, pdf, md]
transform:
  - summarize_per_source
  - aggregate
  - extract: pending_decisions
render:
  format: docx
  sections: [概况, 各部门要点, 风险与异常, 待决事项, 附录]
  out: deliverables/
loop:
  brief_first: true
  confirm_before_render: true
  iterable: true
verify:
  - has_section: 待决事项
  - no_fabricated_numbers
```

## 执行步骤

1. 向用户确认：汇总日期（默认「昨日」）、汇报对象（默认「经营层」）、`inbox/` 下要纳入的附件（若用户未指定则 `list_dir inbox/` 后全部纳入）。
2. 对每个附件用 `read_office`（Office 格式）或 `read_file`（`.md` 等纯文本）读取；**不得编造附件中不存在的数字或部门**。
3. **先输出文字概况**（5 条以内要点 + 风险一句 + 待决事项草稿），询问是否生成正式 DOCX。
4. 用户确认后，`write_office` 生成 `format: docx`，`title` 含日期（如「经营日报 2026-06-05」）；**不必填 `path`**（自动 `deliverables/`）。
5. 增量修改：`load_office_payload` → 改 `blocks` → `write_office` 同路径覆盖。

## 演示数据

见 `docs/harness/fixtures/office-demo/inbox/`（复制到工作区 `inbox/` 后试用）。
