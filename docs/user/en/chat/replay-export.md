# Replay & export

Review past agent behavior or share a thread for debugging.

## Turn replay

**Replay** walks through completed turns: user messages, assistant text, tool invocations, and outputs. Use it to audit what the agent actually ran.

Open replay from the session UI when reviewing a finished or paused thread.

## Export JSON

From the **composer menu**:

- **Export session JSON** — full session metadata and threads
- **Export thread JSON** — single thread payload

Exports help support tickets, regression tests, or archival. They may contain workspace paths and tool output — handle accordingly.

## What export does not do

- Export is not a backup of workspace files — use git or copies of `deliverables/`.
- Re-import of JSON into Zagens is not a primary user workflow today.

## Privacy

Exports can include API-related metadata and file snippets. Redact before sharing externally.

Related: [Sessions](/docs/chat/sessions) · [FAQ](/docs/faq)
