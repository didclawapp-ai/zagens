# UI tour

Zagens opens as a single desktop window with three main regions: **session sidebar**, **chat composer**, and **workspace / tools panels**.

## Session sidebar (left)

| Item | Purpose |
|------|---------|
| **New session** | Start a fresh conversation in the current workspace |
| **Session list** | Switch between past chats for this workspace |
| **Workspace** | Open the [file tree](/docs/workspace/file-tree) and previews |
| **Usage** | Token and cost charts (more detail under Settings) |
| **Tasks** | Background tasks and skills (**scheduled automation list is hidden**; API retained) |
| **Agents** | CRAFT / sub-agent SSE status (Code mode) |
| **Checklist** | Appears when the model uses long-horizon checklist tools (Code mode) |
| **Settings** | API key, MCP, skills, routing, system sub-panels, … |
| **Theme / language** | Light/dark; four UI locales — see [UI language](/docs/desktop/i18n) |

The sidebar also shows **runtime connection** status to the local agent sidecar.

## Chat area (center)

- **Composer** — type goals, paste prompts, or pick Office task cards on empty state
- **Streaming** — answers, tool calls, and optional **thinking** stream
- **Stop** — cancel an in-flight turn
- **Context bar** — estimated context usage for the active thread

Use the composer menu to **export session or thread JSON** for support or [replay](/docs/chat/replay-export).

## Workspace panel (right)

In **Code** mode you typically see:

- File tree and inline **preview** (code, Markdown, images, CSV, …)
- **Diff** view after edits
- **Embedded terminal** (xterm) tied to the workspace directory

In **Office** mode the panel emphasizes **deliverables** and Office file preview; shell tools are trimmed.

## Audit grid (Code, optional)

When enabled, a four-quadrant grid shows **checklist**, **audit scratchpad**, **long-horizon (LHT) graph**, and **sub-agents** — useful for multi-step engineering runs.

## Task type matters

Choosing **Code** vs **Office** at session start changes available tools and prompts. Switching task type starts a **new session** so model context stays stable. See [Task types](/docs/task-types).

Next: [Workspace overview](/docs/workspace/overview) · [Code mode](/docs/code-mode) · [Office mode](/docs/office/overview)
