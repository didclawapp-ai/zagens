# File tree

The **Workspace** sidebar opens a **file tree** for the folder the agent can read and write.

## How to open

Sidebar → **Workspace**. The tree follows the workspace root bound to the active session.

## Code mode

- Shows the repo root by default
- Click a file for a [preview](/docs/workspace/preview) in the right panel
- Open with the system default app when available

## Office mode

Office sessions add quick filters:

| Preset | Contents |
|--------|----------|
| **All** | Workspace root |
| **Deliverables** | `deliverables/` — agent DOCX / XLSX outputs |
| **Docs** | Common document and attachment folders |
| **Changes** | Recently modified files |

After `write_office`, the tree can focus and highlight new files. See [Deliverables](/docs/office/deliverables).

## Relation to the agent

The tree is **browse-only**; create, edit, and delete happen through agent tools (`read_file`, `write_office`, …), not a full IDE.

## Tips

- Office demos: create `inbox/` and `data/` first, or copy fixtures — folders are **not** auto-created
- Large repos: state the subfolder you care about in the first message

Related: [Workspace overview](/docs/workspace/overview) · [File preview](/docs/workspace/preview) · [Office workspace](/docs/office/workspace)
