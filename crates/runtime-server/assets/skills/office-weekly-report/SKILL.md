---
name: office-weekly-report
description: 撰写周报（DOCX/XLSX），默认输出到 deliverables/
---

# 周报

## 技能契约

> **引擎关卡（Phase 2a.3）：** 同目录 [`harness.toml`](./harness.toml) — `prepare` → `write` → `readback_verify`。`load_skill name=office-weekly-report` 后生效：`write_office` 在 prepare 阶段不可调用；readback 阶段禁止再次 `write_office` / `write_file`（绕道会被 `stage_gate_blocked` 拦截）。阶段 verify 通过后自动推进；也可用 `assert_*` + `stage` 显式验收。

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
4. **readback：** 用 `read_office` 回读刚写的 docx，确认章节齐全后再结束（readback 阶段引擎会拦截再次 `write_office`）。
5. 增量修改：`load_office_payload` → 改 `blocks` → `write_office` 同路径覆盖（仅 write 阶段可用）。
