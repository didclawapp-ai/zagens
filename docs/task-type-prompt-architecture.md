# TaskType Layering — Office / Code Two-Type MVP

**Status:** **MVP landed** (`crates/tui/src/task_type.rs`, prompt overlay, Office tool trimming, Zagens Composer switch)  
**Date:** 2026-05-17 (first draft, four types) · **2026-05-18** (converged to two types, aligned with current tools and prompts)  
**Based on:** [prompt-architecture.md](prompt-architecture.md)  
**Related:** [TOOLS_PRINCIPLES.md](tech/TOOLS_PRINCIPLES.md), `crates/runtime-server/src/prompts/base.md` (retrieval triad)

---

## 1. Design Conclusions (Finalized)

| Decision | Content |
|----------|---------|
| **Type count** | **Two:** `Office` (office work), `Code` (programming). No separate Chat / Architecture enums. |
| **Office scope** | General chat + document/spreadsheet/presentation/PDF office tasks. |
| **Code scope** | Write/modify/debug + architecture analysis/design review/read-only research (see §4 read-only workflow). |
| **Relationship to AppMode** | Orthogonal overlay: `AppMode` (Agent / Plan / Yolo) controls *how* to work; `TaskType` controls *what* to do. |
| **Switching type** | **Must start a new session**. Do not change TaskType mid-session — keeps KV cache prefix stable. |
| **Maintenance** | Only 2 task overlay files; `code.md` should be very short or empty (Code = existing full behavior). |

---

## 2. Background and Motivation

Currently almost every session uses the **full Code** system prompt: CRAFT, LSP, symbol index, retrieval triad, hallucination guard, full toolbox. For these scenarios it wastes tokens and confuses judgment:

| Scenario | Problem |
|----------|---------|
| Office / chat | Injects code review, LSP, `edit_file` parallel strategy, etc.; model tends to "over-engineer" |
| Office | Does not need `grep_files` / `exec_shell`, yet they appear in tool list and instructions |
| Architecture review within code session | pick-rules soft constraints alone may still trigger mistaken `edit_file` calls |

The first draft split four types (Chat / Office / Code / Architecture). **Product converged to two:** chat merged into Office (prompt distinguishes "chat vs document work"); architecture review merged into Code (**read-only via Explore sub-agent + workflow**, see §4).

---

## 3. TaskType Definition

```rust
/// Task type — controls prompt overlay and tool registry trimming.
/// Fixed at session creation; switching type = new session.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TaskType {
    /// Chat + office documents. Trimmed prompt + office tool subset.
    Office,
    /// Programming and in-repo technical work (including architecture review). Full prompt + full Agent tool surface.
    #[default]
    Code,
}

impl TaskType {
    pub fn as_str(&self) -> &'static str {
        match self {
            TaskType::Office => "office",
            TaskType::Code => "code",
        }
    }

    pub fn display_name(&self) -> &'static str {
        match self {
            TaskType::Office => "Office",
            TaskType::Code => "Code",
        }
    }

    /// Whether to register code-oriented write tools (edit_file / apply_patch / exec_shell, etc.)
    pub fn uses_code_tool_surface(&self) -> bool {
        matches!(self, TaskType::Code)
    }

    /// Whether to inject full runtime instructions (CRAFT, LSP, symbol index, retrieval triad, etc.)
    pub fn needs_full_code_prompt(&self) -> bool {
        matches!(self, TaskType::Code)
    }
}
```

---

## 4. Two-Type Difference Overview

### 4.1 Token Budget (rough estimate; measure after implementation)

| TaskType | Current (full) | Target | Notes |
|----------|----------------|--------|-------|
| Office | ~8500+ tokens | ~2000–3500 | Remove CRAFT/LSP/symbol index/code toolbox; keep personality, language, office instructions |
| Code | ~8500+ tokens | ~8500+ | **No trimming** (same as current Agent) |

> `base.md` already includes retrieval triad, Capability Claims, etc.; full count may exceed old 8500 table — use `chars/3` or tokenizer measurement.

### 4.2 Tool Permission Matrix (aligned with current implementation)

| Tool | Office | Code |
|------|:------:|:----:|
| `read_file` | ✅ | ✅ |
| `list_dir` | ✅ | ✅ |
| `file_search` | ✅ (find attachments/templates) | ✅ |
| `glob_files` | ✅ (find by path) | ✅ |
| `write_office` | ✅ | ✅ |
| `write_file` | ✅ (non-source deliverables) | ✅ |
| `grep_files` | ❌ | ✅ (incl. `output_mode`: content / files_with_matches / count) |
| `edit_file` / `apply_patch` | ❌ | ✅ |
| `exec_shell` | ❌ | ✅ |
| `agent_spawn` | ❌ (default) | ✅ |
| `run_tests` / `diagnostics` / `git_*` | ❌ | ✅ |
| `web_search` / `fetch_url` / `web.run` | optional ✅ | ✅ |

**Hard boundary:** Office must trim at **`ToolRegistryBuilder` layer**, not prompt prohibition alone. Otherwise the model can still call `edit_file`.

**No Bash search:** Both types use `grep_files` / `glob_files`; **forbid** `exec_shell` running `grep`/`rg` (see `base.md` retrieval triad).

### 4.3 Prompt Injection Trimming

| Block | Office | Code |
|-------|:------:|:----:|
| `compose_base_prompt_layer()` (base.md core) | ✅ trimmed subset or non-code sections of full base | ✅ full |
| personality + mode + approval | ✅ | ✅ |
| `tasks/office.md` overlay | ✅ | ❌ |
| `tasks/code.md` overlay | ❌ | ✅ (prefer empty: Code = default) |
| Environment / project context | ✅ optional (office often needs paths) | ✅ |
| instructions (pick-rules, etc.) | ❌ or office-only excerpts | ✅ |
| Skills catalog | ❌ | ✅ |
| Context Management (Agent/Yolo) | shortened | ✅ |
| CRAFT / LSP / symbol index / retrieval triad | ❌ | ✅ |
| Epistemic / Capability Claims | short version | ✅ full |

### 4.4 Within Office: Chat vs Document Work (same TaskType)

No third enum; distinguish in **`prompts/tasks/office.md`** via behavior rules:

- **Default:** Pure conversation — **no tool calls**; concise answers.
- **User wants to read files / generate xlsx·docx·pptx·pdf:** Use `read_file` / `write_office` / `list_dir`.
- **User wants code changes, run commands, search repo:** Prompt switch to **Code** task and **new session**.

### 4.5 Within Code: Architecture Review / Read-Only Research (former Architecture merged here)

Code sessions **still register write tools**; review tasks reduce mistaken edits via workflow:

1. **Prefer** `agent_spawn` + `type: Explore` (or Review): sub-agent read-only tool surface, conclusions return to main session (context isolation, see `subagent/mod.rs` `build_allowed_tools`).
2. **Main session** self-retrieval: `glob_files` / `grep_files` (`output_mode: files_with_matches`) → `read_file` in segments, follow `base.md` **retrieval triad**.
3. **Without explicit authorization, do not** `edit_file` / `apply_patch` / `exec_shell` to modify repo; if user requests code changes, treat as implementation not review.
4. Output: Mermaid, ADR-style conclusions, findings with paths/line numbers; change suggestions as **diff descriptions**, not direct writes.

> If mistaken edit rate remains high, add session-level `read_only: bool` later (non-MVP).

---

## 5. New Files

```
crates/tui/src/
├── task_type.rs              # enum + infer_task_type()
└── prompts/tasks/
    ├── office.md             # office + chat rules (see §5.1)
    └── code.md               # empty or very short supplement (Code = existing full)

# No longer create: chat.md, architecture.md (merged into two types)
```

### 5.1 `prompts/tasks/office.md` skeleton

```markdown
## Task: Office

This session is for general conversation and office documents, not programming.

### Chat (default)
- Do not call tools; answer directly.
- Keep concise and conversational.

### Documents and files
- Generate XLSX/DOCX/PPTX/PDF: use `write_office`.
- Read attachments or confirm paths: `read_file`, `list_dir`; find by name: `glob_files` or `file_search`.
- Confirm path and format before generating.

### Forbidden
- Do not call `grep_files`, `edit_file`, `apply_patch`, `exec_shell`, `agent_spawn` (unless product explicitly opens later).
- Do not run grep/rg via Bash.
- If user wants code changes, debugging, deep architecture: switch to **Code** task and **new session**.
```

### 5.2 `prompts/tasks/code.md` skeleton

```markdown
## Task: Code

This session uses full Agent tools and code conventions (same as default behavior).
For architecture/review tasks prefer `agent_spawn` Explore; see base.md "code retrieval triad".
```

---

## 6. Code Change Points (Implementation Checklist)

Naming matches existing source: `AppMode`, `ApprovalMode` (not first draft's `AgentMode` / `ApprovalPolicy`).

### T1 — `task_type.rs`

- `TaskType` enum (§3)
- `infer_task_type(workspace, first_message)` rules:
  1. First message contains **fix/implement/edit/bugfix** etc. → **Code** (overrides "analysis" keywords)
  2. Workspace has common code extensions (top level + shallow `src/` scan) → **Code**
  3. First message has office keywords (document/xlsx/pptx/…) without strong Code signal → **Office**
  4. Fallback → **Office** (chat-oriented) or **Code** (prefer Code when repo present) — **product default suggests `Code`**

### T2 — `prompts.rs` embed overlay

```rust
const TASK_OFFICE: &str = include_str!("prompts/tasks/office.md");
const TASK_CODE: &str = include_str!("prompts/tasks/code.md");

fn task_overlay(task_type: TaskType) -> &'static str {
    match task_type {
        TaskType::Office => TASK_OFFICE,
        TaskType::Code => TASK_CODE,
    }
}
```

In `compose_prompt_with_approval(..., task_type, ...)`, append `task_overlay` **after** mode overlay.

`system_prompt_for_mode_with_context_skills_session_and_approval(...)` adds `task_type: TaskType`, skipping Office-unneeded runtime blocks per §4.3.

### T3 — Engine + Session

- `Engine` (or session metadata) holds `task_type: TaskType`
- Written at **session creation**; `set_task_type` **only for new session** flow, not mid-session refresh
- UI/CLI type switch → prompt "will create new session" → `new_session(task_type)`

### T4 — `ToolRegistryBuilder` branches on TaskType

- `TaskType::Code` → existing `with_full_agent_surface` (unchanged)
- `TaskType::Office` → new e.g. `with_office_surface()`: `read_file`, `list_dir`, `file_search`, `glob_files`, `write_office`, `write_file` (optional), `note`; **no** grep/edit/shell/subagent

### T5 — Desktop / TUI UI

Add task type beside AppMode:

| Option | Value |
|--------|-------|
| Auto | backend `infer_task_type` |
| Office | `office` |
| Code | `code` |

On switch: **new session**, do not mutate prompt in place.

### T6 — Config persistence (optional)

```toml
# ~/.deepseek/config.toml or <workspace>/.deepseek/config.toml
[task]
default_type = "code"   # office | code | auto
```

---

## 7. Implementation Order (MVP)

| Batch | Content | Benefit |
|:-----:|---------|---------|
| **M1** | T1 + unit tests for `infer_task_type` | Testable type and rules |
| **M2** | T2 + `office.md` / `code.md` + Office prompt trimming | Token and instruction separation |
| **M3** | T4 + T3 (Engine/session fixed task_type) | Hard tool boundary |
| **M4** | T5 + T6 | User-visible, configurable |

Each batch can be independent PR; before M3 Office may still call write tools via tool list — prefer M2+M3 together or back-to-back.

---

## 8. KV Cache

- **Benefit:** Office session prefix significantly shorter; same-type continuous dialogue improves cache hits.
- **Constraint:** **Forbid** mid-session TaskType switch; switch = new session (accepted).
- **With AppMode:** Same session should also minimize `AppMode` changes; TaskType and mode both fixed at session creation.

---

## 9. First Draft (Four Types) vs Two-Type Plan

| First draft type | Placement in two-type plan |
|------------------|----------------------|
| Chat | `Office` (prompt: default no tools) |
| Office | `Office` (tools + document rules) |
| Code | `Code` (full) |
| Architecture | `Code` (§4.5 Explore + retrieval triad; no separate enum) |

---

## 10. Notes

1. **Office chat** may skip full pick-rules / AGENTS.md; if user asks project details, guide switch to **Code** new session.  
2. **Architecture review** under Code may still mistakenly write: MVP relies on Explore + prompt; monitor mistaken `edit_file` before session `read_only`.  
3. **Auto inference:** Keywords are ambiguous; **fix/implement intent → Code**; UI keeps manual override.  
4. **Plan / Yolo:** TaskType stacks with `AppMode`; Plan sessions still mainly use `update_plan` — whether Plan tools allowed under Office needs a separate rule (suggest Office skips long plan instructions).  
5. **Docs and code sync:** After implementation update [prompt-architecture.md](prompt-architecture.md) with TaskType layer; [TOOLS_PRINCIPLES.md](tech/TOOLS_PRINCIPLES.md) §1.4 and tool table split Office/Code columns.

---

## 11. Acceptance Criteria (Post-Implementation)

- [ ] New Office session: system prompt excludes CRAFT/LSP/symbol index/code retrieval triad long text  
- [ ] Office session: registry has no `edit_file`, `grep_files`, `exec_shell`  
- [ ] New Code session: same as current Agent behavior (full tools + full prompt)  
- [ ] UI switch Office ↔ Code: **creates new session**, old session unchanged  
- [ ] `infer_task_type("分析并修复 login")` → `Code` (Chinese fix intent overrides analysis keywords)  
- [ ] `grep_files` / `glob_files` only in Code (and Explore sub-agent)  

---

## 12. UI Layout (Final Recommendation)

### 12.1 Control placement

**Recommended:** Parallel to existing **run mode** (Plan / Agent / Yolo), in **Composer footer** `Composer.tsx` toolbar — left or right of `runMode` dropdown, same `composer-chip` style.

```text
[ Auto-approve ] | [ Agent ▼ ] | [ Office ▼ ]  ……  model / more
                  ↑ runMode      ↑ taskType (Office | Code | Auto)
```

**Not recommended:** Separate top bar or deep settings — task type is a **session-level** decision, visible before sending.

**Session list** (sidebar): each session should show small tag: `Office` / `Code`, to avoid opening wrong session.

### 12.2 Switch interaction (consistent with "new session")

| Action | Behavior |
|--------|----------|
| New session | Use currently selected TaskType (or "Auto" → `infer_task_type`) |
| Change TaskType in input area | **Modal:** "Switching task type will create a new session; current conversation will not carry over." → confirm → `onNewSession(taskType)` |
| Session in progress | TaskType **read-only** (chip clickable for explanation only, or disabled) |

Do not change type mid-session — avoids KV cache and tool surface mismatch.

### 12.3 UI changes under Office

**Recommendation:** Moderate convergence, not full reskin.

Principle: **Hide code-oriented entries that Office registry lacks**; keep file preview, office deliverables, general settings.

| Area | Code | Office |
|------|:----:|:------:|
| Main chat | ✅ | ✅ |
| Composer: TaskType | ✅ | ✅ |
| Composer: Run mode (Plan/Agent/Yolo) | all three | **default Agent**; Plan/Yolo hidden or grayed "Code session only" |
| Composer: auto-approve | common in Agent | **can default on** (`write_office` mostly Suggest, less interruption) |
| Composer: routing / model | ✅ | ✅ (can keep) |
| Right: **Workbench → Files** | ✅ | ✅; **default open `deliverables/`** (§13) |
| Right: **Checklist** | ✅ | hidden or only when data exists |
| Right: **Sub-agents (agents)** | ✅ | **hidden** |
| Right: **Index** | ✅ | **hidden** |
| Right: **Mermaid** | ✅ | **keep** (office also draws flow/structure) |
| Right: tasks/skills / MCP / system | settings | keep under "more" or settings, not main path |

**No need** to delete code panel paths — use `taskType === 'office'` conditional render; Code sessions restore full sidebar.

**Optional enhancement (non-MVP):** Office session top bar light strip "Office mode · output in deliverables/"; Code no strip or "Code mode".

### 12.4 AppMode combinations (Office session)

| Combination | Recommendation |
|-------------|----------------|
| Office + Agent | **default recommended** |
| Office + Plan | allowed but weakened (office rarely plans first); may hide Plan |
| Office + Yolo | not recommended; hide or block creation |

---

## 13. Office Mode: Where Generated Files Go

### 13.1 Principles

- Paths relative to **Composer workspace root** (same as `write_office` / `read_file` `context.resolve_path`).
- **Do not** default to `.deepseek/`: repo `.gitignore` ignores `.deepseek/`, users won't see deliverables in explorer/git easily.
- When user specifies path explicitly, **user wins**.

### 13.2 Default directory (recommended final)

```text
<workspace>/
  deliverables/          ← Office default output root (configurable)
    <optional subdir>/   ← by session or date, see below
      report_2026-05-18.xlsx
      draft_proposal.pptx
```

| Item | Recommended value |
|------|-------------------|
| Default root | `deliverables/` |
| Subdir strategy (MVP) | not enforced; prompt says "default `deliverables/<short-english-name>.<ext>`" |
| Subdir strategy (enhanced) | `deliverables/<YYYY-MM-DD>/` or `deliverables/<first 8 of session-id>/` to avoid overwrite |

**config.toml (optional):**

```toml
[task]
default_type = "code"
office_output_dir = "deliverables"   # relative to workspace
```

### 13.3 Three-way consistency

1. **`prompts/tasks/office.md`:** when `write_office` has no `path` → `deliverables/...`  
2. **`write_office` tool** (optional): when `path` missing, join `office_output_dir` + slug from title (define at implementation)  
3. **Zagens right panel "Files":** Office session opens workbench **default listing `deliverables/`** (if missing, hint "appears here after first generation")

### 13.4 Non-code workspaces

When Composer workspace points to `~/Documents/MyProject`, `deliverables/` still under that directory — intuitive.

When working **inside a code repo**: suggest project `.gitignore` **not** ignore `deliverables/` (or template comment); user decides commit; distinct from ignoring `.deepseek/`.

### 13.5 Two points pending confirmation

1. Default directory name **`deliverables`** vs **`documents` / `output`**? (Recommend `deliverables`, consistent with base.md deliverable recap wording.)  
2. Should Office **split subfolders by date** (avoid same-name overwrite)? MVP can use flat `deliverables/filename.ext`.  
