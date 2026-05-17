# Changelog

All notable changes to this project will be documented in this file.

**Update policy:** Record **every notable change** (features, fixes, docs, DS
Pick desktop, CLI/TUI, tooling) in this file—typically under `[Unreleased]`,
in the **same PR/commit** as the change when practical.

**DS Pick** (desktop app in `crates/desktop/`) has its **own** version line:
**MAJOR.MINOR.PATCH** in **SemVer** (e.g. **v0.2.1**). Display form **vX.Y.Z**;
each numeric segment is one or more digits (e.g. `0.2.1`, `0.10.3`). This line
**does not** follow the root workspace version used by `deepseek` / `deepseek-tui`
crates (see root `Cargo.toml` `[workspace.package] version`).

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### DS Pick (desktop)

- **技能** — 运行时新增 `POST /v1/skills`（在选定技能根目录下创建 `SKILL.md` 模板）；侧栏「任务与技能」面板支持「新建技能」、全局/工作区选择与（桌面）文件夹选择器。
- **任务与技能面板** — 侧栏「设置」下入口文案为「任务与技能」（右栏 `view` 仍为 `automation`）；**定时自动化**（`/v1/automations` 列表 UI）按产品决定暂不展示，`fetchAutomations` 仍保留在 `client.ts`。
- **文档** — `docs/desktop/TUI_DS_PICK_GAP.md` 实施审核收尾：更新差距表与实施状态；定时自动化 UI 标为暂缓。
- **Markdown preview links** — 预览区内相对路径链接改为在应用内打开下一篇文档，
  不再触发 WebView 整页跳转（避免打断流式生成并丢失未落盘的会话 UI 状态）。`http(s)://`
  等外链仍在新窗口打开；`#` 锚点仅滚动当前预览。
- **Chat markdown + clickable workspace files** — assistant/user 消息按 Markdown
  渲染；形如 `` `docs/foo.md` `` 的本地路径、以及相对路径形式的 `[](…)` 链接在点击
  时在右侧 **工作台** 打开与目录树相同的预览（文本 / 图片 / 二进制等），并自动
  切到「工作区目录」视图。
- **v0.2.1** — Independent desktop semver: `deepseek-desktop` crate, Tauri
  `version`, and `crates/desktop/web-ui/package.json`; sidebar label **DS Pick
  v0.2.1**. Dependency pins to workspace crates (e.g. `deepseek-config`) remain
  at the shared workspace release (e.g. `0.8.15`).

### Process

- Documented changelog maintenance and DS Pick versioning rules (this file,
  header section).
- **Portable rules:** root [project_rules.md](project_rules.md) aggregates `.cursor/rules/*.mdc` for non-Cursor environments (file was `CURSOR_RULES.md`; renamed for clarity).
- **Cursor rules:** `.cursor/rules/security-trust.mdc` (trust & safe changes),
  `code-organization.mdc` (~1000-line soft cap, module layout); `rust-workspace.mdc`
  and `desktop-web-ui.mdc` updated for module size and **TypeScript `strict`** /
  avoid `any`. [AGENTS.md](AGENTS.md) lists rule files for contributors.

### Docs

- **DS Pick:** [TUI vs DS Pick feature parity gap](docs/desktop/TUI_DS_PICK_GAP.md)
  — planning inventory (aligned areas, gaps, suggested prioritization).

### Added
- **办公文档读写** — `read_file` 新增 `.xlsx` / `.pptx` 文本提取（与现有 `.pdf` / `.docx` 同级，通过 OOXML ZIP 解包 + 正则提取纯文本）。`write_office` 工具：从结构化 JSON 生成 `.xlsx`（纯 Rust，`rust_xlsxwriter`）、`.docx`（Python `python-docx` 首选，纯 Rust 最小 OOXML 兜底）、`.pptx`（Python `python-pptx`；`theme` 支持预设 4 套 + 自定义 hex 色板，`subtitle` 显式封面副标题；16:9 宽屏，自动封面+尾页），详见 [docs/office-doc-capability-plan.md](docs/office-doc-capability-plan.md)。
- **Python 运行时检测** — 新增 `python_env.rs`：`find_python()` 统一检测 Python ≥ 3.8（候选链 `python3 → python → py -3`），`ensure_office_venv()` 管理 `~/.deepseek/office-py/` 专用 venv + pinned 依赖（`python-docx` / `python-pptx`）。RLM 和 `code_execution` 同步消除硬编码 `"python3"`。
- **Runtime API: `POST /v1/skills`** — create `<skills_root>/<name>/SKILL.md` with a starter frontmatter template under the configured global skills directory, workspace `.agents/skills/` / `skills/`, or an existing allowed root passed as `parent_directory` (desktop folder picker). See [docs/RUNTIME_API.md](docs/RUNTIME_API.md) § Skills.
- **Runtime HTTP interactive tool approval** — when a thread turn runs with
  `auto_approve: false`, the runtime emits an `approval.required` event and
  blocks until `POST /v1/threads/{thread_id}/turns/{turn_id}/resolve-approval`
  with body `{ "tool_call_id", "decision": "approve" | "deny" }`, or until a
  timeout (default **120s**, overridable via `DEEPSEEK_RUNTIME_APPROVAL_TIMEOUT_SECS`)
  auto-denies the tool. Clients that never call this endpoint will see the turn
  stall until that timeout.


