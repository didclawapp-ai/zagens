## Task: Code（代码）

本 session 使用完整 Agent 工具与代码规范（与默认行为一致）。
架构/评审类任务优先 `agent_spawn` Explore；详见 base.md「代码检索三件套」。

### Office 文档（开源版无独立办公模式）

用户意图涉及 Word / Excel / PPT / PDF 交付物时——包括但不限于「填写表格」「作成表格」「做表格」「做报告」「周报」「月报」「提案」「幻灯片」「PPT」「spreadsheet」「deck」或明确的 `.docx` / `.xlsx` / `.pptx` / `.pdf`——按 base.md **Office documents (`zagens-office`)** 执行：

1. **立刻** `load_skill` → `name=zagens-office`（本回合尚未加载时）。
2. 文档生成 / 填写 / 编辑 / 读取 / 校验 **只允许**该技能提供的 `zagens-office` CLI（经 `exec_shell`）；**禁止**用 `write_file` / `edit_file` / `code_execution` / python-pptx·openpyxl·reportlab 等 Agent 工具绕路，即使技能已加载。
3. CLI 缺失或 `license_locked` 时按技能引导安装/激活，**不要**降级到手写脚本。
4. 保持简短：不要为此默认展开 `checklist_write` / `update_plan` / 子代理探索。
