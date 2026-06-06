---
name: office-production-daily-report
description: 生产与品质晨报（DOCX/XLSX），读表后先概况，默认 deliverables/
---

# 生产 + 品质晨报

## 技能契约

> 约定层（§3.2）；引擎不解析。`verify` 供人工或 headless oracle 验收。

```yaml
id: office-production-daily-report
ingest:
  - kind: files
    from: data/
    formats: [xlsx, csv]
transform:
  - summarize
  - aggregate
  - extract: exceptions
render:
  format: docx
  sections: [概况, 生产指标, 品质指标, 异常与风险, 待确认事项]
  out: deliverables/
loop:
  brief_first: true
  confirm_before_render: true
  iterable: true
verify:
  - no_fabricated_numbers
  - has_section: 概况
  - mentions: OEE
```

## 执行步骤

1. 确认：汇报日期（默认「昨日」）、是否同时输出 XLSX 摘要表（默认仅 DOCX）。
2. 用 `read_office` 读取 `data/生产日报_昨日.xlsx`（或用户指定路径）；大表分页读取，**不得编造表中不存在的数字**。
3. **先输出文字概况**（生产达成、OEE、良率、主要异常各 1～2 句），询问是否生成正式 DOCX。
4. 用户确认后，`write_office` 生成 `format: docx`，`title` 含日期；**不必填 `path`**。
5. 若用户要 XLSX：用概况中的关键指标生成简表（可选第二份 `write_office`）。
6. 增量修改：`load_office_payload` → 改 `blocks` → `write_office` 同路径覆盖。

## 演示数据

见 `docs/harness/fixtures/office-demo/data/生产日报_昨日.xlsx`（复制到工作区 `data/` 后试用）。
