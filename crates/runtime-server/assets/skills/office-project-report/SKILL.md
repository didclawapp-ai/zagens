---
name: office-project-report
description: 制作项目汇报 PPT（PPTX），默认输出到 deliverables/
---

# 项目汇报 PPT

1. 确认：项目名称、汇报人、听众、汇报日期。
2. `write_office` 生成 `format: pptx`：`slides` 至少含封面（项目名+汇报人）、进展要点、风险与缓解、下一步计划。**不必填 `path`**。
3. 增量修改：`load_office_payload` → 改 `slides` → `write_office` 同路径覆盖。
