---
name: office-meeting-minutes
description: 整理会议纪要（DOCX），默认输出到 deliverables/
---

# 会议纪要

1. 确认：会议时间、参会人、议题列表（可简短默认）。
2. 用 `read_office` 读取用户提供的会议材料或录音整理稿（若有）。
3. `write_office` 生成 `format: docx`：`blocks` 含标题、参会人、议题讨论、决议、行动项（负责人/截止日期）。**不必填 `path`**。
4. 增量修改：`load_office_payload` → 改 `blocks` → `write_office` 同路径覆盖。
