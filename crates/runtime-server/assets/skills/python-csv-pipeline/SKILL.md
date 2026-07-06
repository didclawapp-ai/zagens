---
name: python-csv-pipeline
description: Summarize CSV data with Python and write JSON under deliverables/ (non-Office pilot)
---

# Python CSV 流水线

## 适用场景

用户给出 workspace 内 CSV（或文本表格），需要 **Python 统计摘要** 并输出 **JSON 交付物**（非 Office、非 Rust 编译链）。

## 执行步骤

1. **inspect**：`glob_files` / `read_file` 定位 CSV；确认列名与行数。
2. **analyze**：`exec_shell` 运行 Python（示例：`python -c "..."` 或 `python scripts/summary.py`）生成中间统计；stdout 应可解析。
3. **deliver**：`write_file` 写入 `deliverables/summary.json`（结构化 JSON，含 row_count / columns / sample_stats）。
4. **verify**：调用 `assert_file_count` / `assert_output_matches` 完成 stage verify（引擎自动跑 `[[verify]]` 谓词）。

## 约束

- 输出路径默认 **`deliverables/`**（与队列 / handoff 叙事一致）。
- 不要在 **analyze** 阶段调用 `write_file`（stage gate 会拦）；不要跳过 **verify** stage 的 assert 工具绕道。
