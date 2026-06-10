---
name: audit-repo
description: Full-repo audit (scratchpad + verified findings, finding-first report). Security, correctness, tests, release, and maintainability — not security alone. Runtime requires reviewed_ratio ≥ 40% before write_file unless partial_closeout approved.
metadata:
  short-description: Full-repo audit scratchpad workflow
---

# Full-repository audit (scratchpad)

Use this skill for **repo-wide code audit**. Pair with `base.md` § Full-repository code review mode (verification, Auditor, caller-trace).

**Do not use** for one module, a single PR, or a quick skim.

**Priority:** **verified findings first**, then coverage honesty. Optional `[D1]`–`[D10]` tags on `claim` when helpful — not required on every line.

## Read-only audit (mandatory — parent and children)

Full-repo audit is **review and report only**. You are **not** the implementer for this session.

| Allowed | Forbidden during P0–P3 |
|---------|-------------------------|
| `read_file`, `grep_files`, `list_dir`, `git_*`, `scratchpad_*`, `agent_spawn` (explore/auditor), `write_file` **only** to audit deliverable paths | `edit_file`, `apply_patch`, `write_file` to `src/**`, `web-ui/**`, `Cargo.toml`, config, `.env`, etc. |
| `run_tests` / `exec_shell` (e.g. `cargo check`, `cargo test`, `cargo clippy`) **to observe** exit code + stderr | **Fixing** failing tests, clippy, or build errors — no “repair then re-run” loop |
| Record failures as **findings** or baseline bullets | Spawning `implementer` / “fixing before publish” unless the user **explicitly** asks to fix issues in a **new** task |

**Verification commands:** run **at most one** scoped check if needed (e.g. `cargo check -p zagens-core`), capture exit code — then **stop**. Do **not** run full-workspace `cargo clippy` + per-crate test matrix + fix loop unless the user asked for a CI audit task.

**Explore/Review sub-agents** are already tool-capped read-only; the **parent** must follow the same rule.

**Secrets in reports:** never paste live API keys, tokens, or private key material — redact (`sk-…`, `mk-…` → `sk-***redacted***`). Cite `file:line` only.

**Stop after P2:** once the audit deliverable is `write_file`’d, **end the turn** — do not continue fixing code or re-running clippy in the same audit session.

## Coverage dimensions (guidance — not a checkbox form)

Areas slice by **path**; dimensions slice by **concern**. You do **not** need all 10 dimensions closed — but **D1, D2, D6 must be examined** in every full-repo run (grep + read_file in the relevant paths).

| ID | Dimension | Must-hit paths (grep/read) |
|----|-----------|----------------------------|
| **D1** | Trust & security | `desktop/`, `runtime-server/src/tools/`, `windows-sandbox/`, `secrets/` |
| **D2** | Correctness & concurrency | `core/src/engine/`, `runtime-orchestrator/` |
| **D6** | Release & signing | `updater.key`, `tauri.conf.json`, `*.pub`, signing config |
| D3–D5, D7–D10 | Architecture, tests, supply chain, maintainability, observability, cross-platform, docs | Cover when areas are `done`; one-line defer in report is OK |

**Severity** (per `base.md`): use **BLOCKER/CRITICAL** only for indefensible exposures. Do **not** label HIGH findings as "阻塞级" in prose unless severity is BLOCKER/CRITICAL. P0 actions may reference HIGH items — keep counts consistent.

## Regression probes (P0 — before area spawns)

Run these **every** full-repo audit; append hits as verified findings or `kind=cleared`:

| Probe | Command / action |
|-------|------------------|
| Signing key in tree | `grep_files` / `file_search`: `updater.key`, `*.pem`, private key filenames under `crates/desktop/` |
| Hardcoded API keys | `grep_files`: `api_key`, `API_KEY`, `mk-`, `sk-` in `crates/` |
| `trust_mode` from client | `grep_files`: `trust_mode` in `runtime-api`, `stream.rs`, `spec.rs` |
| LoopGuard concurrency | `read_file`: `core/src/engine/loop_guard.rs` if engine area in scope |
| Baseline (3 bullets) | Zero-test crates (grep `#[test]` per crate); `deny.toml` / `cargo-deny` exists?; files **>1000** lines (top 5) — append as `_global` `kind=meta` or report §基线指标 |

## Runtime gates (know before P2)

| Gate | Rule |
|------|------|
| `accounted_ratio` | ≥ 60% — each `done` needs `finding` or `cleared`; each `deferred` needs `meta` reason |
| **`reviewed_ratio`** | **≥ 40%** — only **`done`** areas with `finding`/`cleared` count; **mass `deferred` does not substitute for review** |
| Partial report | If user explicitly approves partial close-out: `scratchpad_append({ kind:"meta", area_id:"_global", claim:"partial_closeout: …" })` then title must include **「部分审核」** |

If `write_file` to audit deliverables is blocked, call `scratchpad_status` — do not fake a full-repo report in chat only.

## External memory (mandatory)

`.deepseek/scratchpad/{run_id}/` — `inventory.json` + `notes.jsonl`

**Phase B tools:** `scratchpad_init`, `scratchpad_status`, `scratchpad_append`, `scratchpad_list_notes`, `scratchpad_set_area`, `scratchpad_import_agent`, `scratchpad_verify_note`.

**Sidebar:** `checklist_write` / `checklist_update` — one row per inventory area (`{area_id}: {path}`).

`write_file` is **fallback only** when scratchpad tools truly fail.

## P0 — Inventory + checklist

```
scratchpad_init({ "template": "workspace_audit", "scope": "…" })
```

- **10–40 areas**, ≤ ~20 source files per row — **do not over-split** (avoid 35+ rows that force mass defer).
- Run **Regression probes** + 3 baseline bullets (above).
- `scratchpad_append` `{"kind":"meta","area_id":"_global","claim":"inventory_version 1, N areas"}`.
- **`checklist_write`** one todo per area (same turn).

## P1 — Examine (sub-agent pipeline)

- **`agent_spawn(type=explore)`**, `task_id` = `run_id`.
- **`step_timeout_ms` by file count:**

| Files in area | `step_timeout_ms` |
|---------------|-------------------|
| ≤10 | 600000 |
| 11–20 | 900000 |
| 21–40 | 1200000 |
| >40 or `runtime-server/src/tools` | 1800000 |

- Assignment: exact **`area_id`**, **`path`**, dimensions to stress (from table), `file`+`line` + caller-trace for security claims.
- Join: `agent_wait` → import → verify HIGH/BLOCKER → `scratchpad_set_area(done|deferred)`.
- **Outlier rule:** cancel + defer agents stuck past `2× median` duration.
- **`StepLimitReached` ≠ done** — re-spawn narrower scope.
- **Prefer `done` over `deferred`** until `reviewed_ratio` would pass 40% gate.

### Sub-agent output

```
<!-- audit-findings -->
{ "area_id", "area_path", "items": [{ "severity", "file", "line", "claim", "evidence" }], "summary" }
```

### `notes.jsonl`

| Field | Rule |
|-------|------|
| `kind` | `finding` \| `cleared` \| `meta` \| `todo` (todo not in report) |
| `status` | Report uses **`verified`** findings only |
| HIGH/BLOCKER | `file` + `line` required |

`done` without findings → `kind=cleared` before `scratchpad_set_area(done)`.

**Defer many pending areas:** never batch `scratchpad_set_area(deferred)` in one step. Per area, in **separate** model steps: `scratchpad_append` (`kind=meta`, non-empty defer `claim`) → `scratchpad_set_area(deferred)` for that `area_id` only.

## P2 — Synthesize (finding-first report)

**Pre-flight:** inventory closed; `reviewed_ratio` ≥ 40% OR `_global` `partial_closeout`; Auditor for HIGH/BLOCKER per `base.md`.

**Paths:** `deliverables/*audit*`, `doc/*audit*`, `CODE_AUDIT*.md`

### Report template

Title: `# Zagens 全库代码审核报告` — add **`（部分审核）`** when `reviewed_ratio` < 50% or `partial_closeout`.

**Line numbers (mandatory):** every finding uses repo-relative **`位置`** = `path:line` or `path:line-line_end` (per `audit_rules/rust.md` — no “around line”). **`证据`** = one `read_file`/`grep_files` excerpt (≤3 lines, secrets redacted). Parent re-checks HIGH/BLOCKER `位置` before `write_file`.

```markdown
# Zagens 全库代码审核报告

**审核日期:** YYYY-MM-DD
**版本:** {crate version}
**实审覆盖:** X done / N areas（reviewed_ratio …%）

---

## 执行摘要

| 严重级别 | 数量 | 状态分布 |
|---------|------|----------|
| HIGH | n | open m / fixed k / deferred d |
| MEDIUM | n | … |
| LOW | n | … |

- **实审覆盖:** X done / N areas（reviewed_ratio …%）— 与 deferred 数量分开写
- **裁决:** Request changes / Approve with backlog — **不要**写 LHT「full gate」话术

---

## 回归探针（P0）

| 探针 | 结果 | 备注 |
|------|------|------|
| `updater.key` / 私钥入库 | cleared / finding | grep 路径 |
| `sk-` / `mk-` / `api_key=` 硬编码于 `crates/` | cleared / finding | 不含 `.env` 即 cleared 仅限源码树 |
| `trust_mode` 客户端可控 | cleared / finding | `runtime-api` |
| LoopGuard 并发 | cleared / finding | `loop_guard.rs` |
| （可选）其他 D6 路径 | … | |

---

## 基线指标

- 规模、测试分布、**deny.toml / cargo-deny**、**>1000 行文件 Top 5**
- 与 finding 矛盾时以探针/finding 为准（勿写「无硬编码密钥」同时又报 `.env` key）

---

## BLOCKER / CRITICAL
（无则写「无」）

## HIGH

### H-01: {标题}

| 字段 | 内容 |
|------|------|
| **状态** | `open` \| `fixed` \| `deferred`（fixed 可注 commit/日期） |
| **位置** | `crates/foo/bar.rs:42` 或 `crates/foo/bar.rs:120-145` |
| **发现项** | 一句话结论 |
| **证据** | 摘录：`fn foo()` / `unsafe { … }` / grep 命中（密钥 → `sk-***redacted***`） |
| **影响** | 用户/安全/正确性影响 |
| **建议** | 可执行修复方向 |
| **note_id** | scratchpad note id（可选） |

（每条 HIGH 重复上表；MEDIUM 可用下表压缩。）

## MEDIUM

| ID | 状态 | 位置 | 发现项 | 建议 |
|----|------|------|--------|------|
| M-01 | open | `path:line` | … | … |

## LOW

| ID | 状态 | 位置 | 发现项 |
|----|------|------|--------|
| L-01 | open | `path:line` | … |

> LOW 表为**全部** LOW 条目；若只列代表性子集，标题须写「LOW（节选）」并说明省略数量。

---

## 亮点与已验证无问题
`kind=cleared` + 正向探针（SSRF、路径 canonicalize 等），每项附 **`位置`** 或模块路径。

---

## 各 Area 审核摘要

| area_id | status | 主要发现 |
|---------|--------|----------|

> 完整 inventory 见 scratchpad `inventory.json`（N 行）；上表可为合并视图，须脚注说明。

---

## 未覆盖与 Deferred
逐条 `area_id` + reason；无则写「无」。

---

## Verification summary

| 检查项 | 结果 |
|--------|------|
| HIGH 位置复核 | H-01 `path:line` ✅ / ❌ |
| 回归探针 | 见上表 |
| 降级项 | 无 / 列表 |
| 子代理 | N 成功 / 超时 / 失败 |

---

## 建议优先级

### P0（发布前）
| 引用 | 状态 | 行动 |

### P1 / P2
…
```

**Same turn:** `write_file` after gates pass — no prose-only finale. **Then stop** — do not fix findings or re-run clippy/tests in the same audit thread.

### Partial close-out

1. User explicitly requests partial report / 部分收口.
2. `scratchpad_append({ kind:"meta", area_id:"_global", claim:"partial_closeout: user approved …" })`.
3. Defer remaining areas with per-area `meta` reason.
4. Report title **（部分审核）** + honest §未覆盖.

## P3 — Verify

`agent_spawn(type=auditor)` for HIGH/BLOCKER. Prose claims must map to scratchpad `note_id`.

## Reference

Design doc: `docs/desktop/audit-scratchpad-design.md`
