---
name: office-resume
description: 简历与求职信（DOCX），默认输出到 deliverables/
---

# 简历 / 求职信

1. 确认：目标岗位、公司（可选）、语言、是否附求职信。
2. 用 `read_office` 读取用户提供的旧简历或 JD（若有）。
3. `write_office` 生成 `format: docx`：联系方式、摘要、经历（倒序）、技能、教育；求职信单独一节或第二份文档。**不必填 `path`**。
4. 增量修改：`load_office_payload` → 改 `blocks` → `write_office` 同路径覆盖。
