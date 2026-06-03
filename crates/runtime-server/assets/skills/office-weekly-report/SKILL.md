---
name: office-weekly-report
description: 撰写周报（DOCX/XLSX），默认输出到 deliverables/
---

# 周报

1. 向用户确认：时间范围、汇报对象、本周完成 / 下周计划 / 风险（可简短默认）。
2. 用 `read_office` 读取用户提供的 Excel/附件（若有）。
3. `write_office` 生成 `format: docx`，`title` 含周次；**不必填 `path`**（自动 `deliverables/`）。
4. 增量修改：`load_office_payload` → 改 `blocks` → `write_office` 同路径覆盖。
