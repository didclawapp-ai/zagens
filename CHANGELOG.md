# Changelog

All notable changes to this project will be documented in this file.

**Update policy:** Record **every notable change** (features, fixes, docs, DS
Pick desktop, CLI/TUI, tooling) in this file—typically under `[Unreleased]`,
in the **same PR/commit** as the change when practical. Cursor agents: see
`.cursor/rules/ds-pick-repo.mdc` § Changelog.

**DS Pick** (desktop app in `crates/desktop/`) has its **own** version line:
**MAJOR.MINOR.PATCH** in **SemVer** (e.g. **v0.3.0**). Display form **vX.Y.Z**;
each numeric segment is one or more digits (e.g. `0.2.1`, `0.10.3`). This line
**does not** follow the root workspace version used by `deepseek` / `deepseek-tui`
crates (see root `Cargo.toml` `[workspace.package] version`).

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

> **2026-05-19:** `[0.3.0]` consolidates work since v0.2.2 (backfilled from `git log`).
> Prefer updating `[Unreleased]` incrementally going forward.

## [Unreleased]

### Added

- **Audit scratchpad (Phase B):** Runtime tools `scratchpad_*`; `ScratchpadStore` + layered P2 summary injection, readonly nudge (B4), cycle handoff pointer (B3b), `ThreadRecord.scratchpad_run_id` (B2), TTL cleanup (B7), `GET /v1/threads/{id}/scratchpad/status`, DS Pick `AuditScratchpadBar` (B5). Config: `[scratchpad]` in `config.toml`.
- **Audit scratchpad (B7 hardening):** `supersedes` transitive closure; `scratchpad_append` schema tightened; per-turn single `<scratchpad_summary>`; `git_blame` counts toward readonly nudge.
- **Audit scratchpad (Phase A):** Full-repo review external memory — `pick-rules.md` §7, `base.md`, bundled **`audit-repo`** skill. Design: [audit-scratchpad-design.md](docs/desktop/audit-scratchpad-design.md).
- **Docs:** [audit-scratchpad-test.md](docs/desktop/audit-scratchpad-test.md) — Phase A smoke, resume, and **14-area** `crates/tui/src/` run (`2026-05-19-tui-src-review`).
- **Docs:** [audit-scratchpad-test.md](docs/desktop/audit-scratchpad-test.md) — Phase B smoke/gate/resume/synthesize on `2026-05-19-phase-b-smoke`; design §6 marked implemented.
- **Docs:** [audit-scratchpad-design.md](docs/desktop/audit-scratchpad-design.md) — Phase C plan §6.12 (C0–C4: compaction, coverage gate, Auditor binding, blackboard mirror); §6.10 B7 marked shipped.
- **Docs:** Phase C design review §13.5 — deferred meta gate, L0-only compact, C1/B1 boundary, Auditor track B prose check, soft-warn format.
- **Audit scratchpad (Phase C0/C1):** Compaction pin + L0-only handoff; `coverage_gate` (soft/hard); `set_area(deferred)` requires `kind=meta`; `[scratchpad]` config fields.
- **Audit scratchpad (Phase C2/C3):** `agent_spawn(type=auditor)` builds track A/B from scratchpad; blackboard `scratchpad` mirror partition; `scratchpad_run_id` spawn param.
- **Skills:** `audit-repo` — append-before-`done` ordering; bundled skills marker v3.

### Changed

- **DS Pick (web UI):** Assistant **Reasoning** and **工具调用** blocks default to **collapsed**; click header to expand (streaming shows “推理中…” / “N 个进行中” hints while folded).
- **Docs:** [README.md](README.md) — lead with verified differentiators; split desktop vs shared runtime; trim misleading feature tables; fix dev commands (`cargo tauri dev`); align doc links (`API_DESIGN.md`, `DEV_NOTES.md`). Cursor/portable rules updated for dead links.

## [0.3.0] - 2026-05-19

### DS Pick (desktop)

- **v0.3.0** — `deepseek-desktop`、`tauri.conf.json`、`web-ui/package.json` 与侧栏标签对齐 **v0.3.0**。
- **会话回放与 diff** — 会话回放、diff 面板、上下文用量展示；Code 工作区 **交互式 PTY 终端**；切换会话时恢复上下文用量；聊天代码块复制。
- **Composer UI** — 新版输入区/composer 布局；Mermaid 渲染修复。
- **系统设置** — 完整系统设置视图迁入右侧面板；**系统托盘**（关闭窗口可隐藏到托盘）；turn 完成且窗口最小化/隐藏时 **原生桌面通知**。
- **i18n** — Web UI 国际化框架（中/英等）。
- **子代理设置** — 与 TUI 双轨对齐：`max_subagents` 优先级与 subagents 功能开关。
- **侧栏/技能** — 技能导入；Office/Code 任务类型；Documents 默认工作区；搜索类工具 UI/配置。
- **文档** — 技术文档迁至 `docs/tech/`；diff 面板暗色主题修复。

### Added

- **符号索引 V3–V5** — 懒加载按工作区构建（去除启动阻塞）；MermaidPanel 图表实时渲染；`edit_file` v0/v1；索引管理面板；符号变更追踪与 bridge 关联（V5）；调用关系与置信度（见 `docs/symbol-index-v*.md`）。
- **Sidecar 加固** — Supervisor 强化、SQLite 存储、启动门控、keyring 密钥。
- **CRAFT** — P0 结构化裁决、P1 黑板、P3 工具裁剪、fix-loop 协议（`docs/craft-v2-improvements.md`）。
- **Vision** — 默认切换 **Qwen3-VL**；模型感知提示词；代理/超时配置；`describe_image` 工具与 OCR bridge。
- **Office** — PDF 读取支持；`write_office` / xlsx 生产级修复（6 项）；工具进度流式输出与 LLM 使用指南。
- **搜索工具** — Office/Code 任务类型相关检索能力扩展。
- **提示词** — V4 幻觉抑制类系统提示调整。

### Changed

- **DS Pick** — Light theme “极淡乳白” palette (warm stone canvas/card, softer dividers, blue accent retained); center chat column uses `card` surface; dark theme slightly warm-tinted.
- **DS Pick** — Sidebar: app icon in brand row; title bar brand text removed; connection status at bottom (“连接正常” / “未连接”); version blurb moved to **关于** panel under Settings.
- **Runtime** — 消除 tokio worker 中的阻塞 I/O；诊断与相关文档更新。

### Fixed

- **DS Pick** — Session restore `GET …/events?replay_only=1` no longer returns HTTP 400 (accept `1`/`0` query booleans; client uses `replay_only=true`).
- **DS Pick** — Switching sessions keeps in-memory UI snapshots (tools + thinking); thread event replay still refreshes from runtime when available.
- **DS Pick** — Checklist panel: sidebar **清单** entry, persist `checklist_write` on thread record (survives sidecar restart), faster poll while streaming; auto-switch no longer blocks manual **工作台** tab during streaming.
- **DS Pick** — Left sidebar width is draggable on the column edge (persisted; 180px–45% viewport); fixes broken resize handle that was absolutely positioned without a containing block.
- **DS Pick** — Unified divider tokens (`chrome-seam` vs `divider` vs `card-border`); column resize gutters use inset seams only; shell panels use tint bands instead of stacked border lines.
- **Desktop** — Unicode 工作区路径下的可点击聊天链接。
- **xlsx** — 生成路径六项生产级修复。

### Security

- CODE_REVIEW **Step 1** — 依赖与安全清理（见 `docs` 中 Step 1 跟踪）。

### Docs

- Agent 可靠性/CRAFT/符号索引/V5 等规划与实施记录更新；部分旧版 brief 归档或移除。

### Process

- **Changelog 维护** — 写入 `.cursor/rules/ds-pick-repo.mdc` 与 `project_rules.md`（notable 变更需同步本文件）。
- **Portable rules** — `project_rules.md` 聚合 `.cursor/rules/*.mdc`（原 `CURSOR_RULES.md` 更名）。

---

## [0.2.2] - 2026-05-11

### DS Pick (desktop)

- **v0.2.2** — `deepseek-desktop`、Tauri `version`、`web-ui/package.json` 与侧栏标签对齐 **v0.2.2**；打包脚本与 bundled Python 准备（`prepare-python.mjs`、`docs/bundled-python-plan.md`）。
- **任务与技能** — 侧栏「任务与技能」；`POST /v1/skills` 创建 `SKILL.md` 模板；全局/工作区技能根与桌面文件夹选择器；定时自动化列表 UI 暂缓展示（`fetchAutomations` 保留）。
- 关闭应用时 **自动结束 sidecar**；Web UI 样式 refinements；用户消息气泡可读文本色修复；用量扫描性能、终端主题、**USD 成本** 标签。

### Added

- **办公文档** — `read_file` 支持 `.xlsx` / `.pptx` 文本提取；**`write_office`** 从 JSON 生成 `.xlsx`（Rust）、`.docx` / `.pptx`（Python，主题与 16:9 布局）；详见 [docs/office-doc-capability-plan.md](docs/office-doc-capability-plan.md)。
- **Python 运行时** — `python_env.rs`：`find_python()`、`ensure_office_venv()`（`~/.deepseek/office-py/`）；RLM / `code_execution` 去除硬编码 `python3`。
- **Runtime API** — `POST /v1/skills`；交互式工具审批 HTTP 流（`approval.required` + `resolve-approval`，默认 120s 超时）。

### Fixed

- Shell 工具终端 UX；PPTX 引擎资源扩展。

---

## [0.2.1] - 2026-05-10

### DS Pick (desktop)

- **v0.2.1** — 独立桌面 SemVer；侧栏 **DS Pick v0.2.1**（workspace crate 依赖仍为共享线，如 `0.8.15`）。
- **三栏 shell 增强** — 统一聊天壳、右侧面板（runtime API 对接）、devtools 开关、窗口权限与标题栏。
- **Markdown** — 聊天区 Markdown 渲染；`` `path/to/file` `` 与相对 `[](…)` 链接在工作台打开预览；预览区内相对链接 **应用内** 打开（外链新窗口，`#` 锚点滚动）。
- **启动** — DS Pick 与 sidecar 就绪速度优化。

### Docs

- `docs/desktop/DEV_NOTES.md`；[TUI vs DS Pick 差距表](docs/desktop/TUI_DS_PICK_GAP.md) 初版。

---

## [0.2.0] - 2026-05-09

### DS Pick (desktop)

- **v0.2.0** — DS Pick 产品化里程碑：文档布局、Cursor 规则、桌面与 runtime 一批修复。
- **Tauri 壳** — Sidecar 生命周期、三栏布局、会话文件同步、工作区与附件、运行模式。
- **工作台** — 文件预览模块（文本/图片/二进制）、安全 `read_workspace_binary`；`web_search` Bing 回退。
- **主题** — 明/暗样式与 Markdown 预览 demo 打磨。
- **安全** — CODE_REVIEW：sidecar token 环境变量、CSP、MCP 过滤、updater 相关加固。

### Added

- **Runtime HTTP** — Phase 1 SSE `id` 序列；Phase 2 Tauri + Web MVP；thinking/reasoning 经 SSE 流式输出；工具审批范围与超时配置（Phase 2a）。
- **TUI** — `read_file` / `file_info` 工具链；大文件 plaintext 流式读取；`file_info` RFC3339 `mtime`。

### Fixed

- 会话恢复时还原已保存工作区；sidecar 默认 `cwd` 合理化。

### Docs

- README DS Pick 章节与仓库布局表；桌面文档迁入 `docs/desktop/`。

---

## [0.1.0] - 2026-05-07

### Added

- **Initial fork** — DeepSeek TUI desktop 工作区：共享 `deepseek` CLI/TUI/runtime crates 与 **DS Pick**（`crates/desktop/`）骨架。
- **Runtime API** — `/v1/...` 契约与 [docs/RUNTIME_API.md](docs/RUNTIME_API.md) 实施文档（Phase 1）。

### Changed

- Desktop sidecar / main Rust 源码 `rustfmt`。
