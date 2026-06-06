# Code mode

**Code** task type targets software engineering: read the repo, run commands, review diffs, and iterate with LHT / CRAFT-style harness tools.

## Workspace

Point Zagens at a git checkout or project folder. The agent can:

- Search and edit source files (`grep_files`, symbol index)
- Run terminal commands in a [sandboxed workspace shell](/docs/workspace/terminal)
- Produce patches you review in the [diff preview](/docs/workspace/preview)

See [Workspace overview](/docs/workspace/overview) for the file tree and snapshots.

## UI areas (Code)

| Area | Use |
|------|-----|
| **Workspace panel** | Tree, preview, diff, terminal |
| **Checklist sidebar** | Long-horizon task steps |
| **Audit grid** | Checklist, scratchpad, LHT graph, sub-agents |
| **Replay** | Inspect past tool calls |

Full layout: [UI tour](/docs/ui-tour).

## When to use Code vs Office

| Task | Mode |
|------|------|
| Refactor, tests, CI fixes | **Code** |
| Competitive brief, quote spreadsheet, executive daily | **Office** |

## Tips

- Keep one workspace per repository for clearer context.
- Use explicit goals ("add unit tests for `foo.rs`") for faster iterations.
- Large monorepos: mention the crate or subfolder you care about in the first message.
- Risky shell commands may show an **approval dialog** — configure under Settings.

Deep dives: [LHT](/docs/code/lht) · [CRAFT](/docs/code/craft) · [Audit scratchpad](/docs/code/audit-scratchpad)
