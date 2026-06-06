# Workspace overview

A **workspace** is the folder Zagens agents read, edit, and run commands in. Pick one directory per project or office inbox.

## Choosing a workspace

1. On first launch or from the sidebar, select **Workspace**.
2. Point to a git repo (Code) or an office folder with `inbox/` and `data/` (Office).
3. Sessions are scoped to that path — switching workspace shows that folder's chat history.

## Code vs Office layout

| Mode | Typical layout |
|------|----------------|
| **Code** | Repository root; agent uses terminal, diffs, symbol index |
| **Office** | `inbox/` for briefs, `data/` for spreadsheets; output in `deliverables/` |

See [Office workspace](/docs/office/workspace) for the document-centric layout.

## What you can do here

- Browse files in the **[file tree](/docs/workspace/file-tree)**
- **Preview** supported formats in the right panel
- In Code mode: run an **embedded terminal**, review **[diffs](/docs/workspace/diff)**, restore **[snapshots](/docs/workspace/snapshots)**

## Tips

- Use a dedicated folder — avoid pointing at drive root or `%USERPROFILE%`.
- For Office demos, copy fixtures from `docs/harness/fixtures/office-demo/` into your workspace.
- Large repos: tell the agent which crate or subfolder matters in the first message.

Related: [File preview](/docs/workspace/preview) · [Terminal](/docs/workspace/terminal)
