# Context usage

Long threads fill the model **context window**. Zagens surfaces usage and can compact older content automatically.

## Context bar

The composer area shows an estimated **context pressure** bar for the active thread — how much of the window is consumed by history, tools, and attachments.

When pressure is high, compaction or a fresh session may be recommended.

## Compaction

The runtime can summarize or trim older turns (L1/L2/L3 thresholds in config) so new messages still fit. Compaction preserves recent turns and critical tool results where possible.

You do not need to manage this manually in normal use.

## Tips for long tasks

- Start a **new session** when pivoting to an unrelated task.
- In Code mode, use [LHT](/docs/code/lht) for multi-hour work instead of one giant chat.
- Paste large logs as files in the workspace and ask the agent to `read_file` slices.

## Office mode

Office workflows are usually shorter per deliverable. If a thread grows while iterating on a DOCX, consider confirming each `write_office` revision instead of one endless chat.

Related: [LHT long-horizon tasks](/docs/code/lht) · [Sessions](/docs/chat/sessions)
