---
name: office-release-notes
description: 产品发布说明（DOCX），默认输出到 deliverables/
---

# 发布说明

1. 确认：产品名、版本号、发布日期、受众（内部/客户/开发者）。
2. 用 `read_file` / `read_office` 读取 CHANGELOG、PR 列表或用户提供的变更清单（若有）。
3. `write_office` 生成 `format: docx`：版本摘要、新功能、改进、修复、已知问题、升级指引。**不必填 `path`**。
4. 增量修改：`load_office_payload` → 改 `blocks` → `write_office` 同路径覆盖。
