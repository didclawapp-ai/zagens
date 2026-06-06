# Checklist sidebar

When the model calls `checklist_write` / `checklist_update`, a **Checklist** sidebar tracks macro steps for long Code tasks.

## Where it appears

- Left sidebar **Checklist** tab (Code mode)
- **Audit grid** → checklist quadrant

Items show status: pending, in progress, done, blocked.

## How it is populated

The agent creates the checklist — you do not hand-edit the JSON. Typical flows:

1. LHT or CRAFT macro cycle starts
2. Model writes plan items via checklist tools
3. Each tool success may mark items complete

## Why use it

- See progress on 10+ step refactors without scrolling chat
- Spot blocked items before asking "what's left?"

Office mode does not surface engineering checklists.

## Tips

- Ask explicitly: "maintain a checklist for this refactor" if the model skips it.
- Blocked items often mean a failed test or approval — check [diff](/docs/workspace/diff) and [approval](/docs/desktop/approval-dialog).

Related: [LHT](/docs/code/lht) · [Sub-agents](/docs/code/subagents)
