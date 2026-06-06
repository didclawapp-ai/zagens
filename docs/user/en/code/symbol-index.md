# Symbol index

The **Index** panel tracks a per-workspace **symbol index** that speeds up `grep_files` and symbol-aware search in Code mode.

## Open the panel

Sidebar → **Index** (Code sessions only). Shows status, file count, size, and last rebuild time.

## Status

| Status | Meaning |
|--------|---------|
| **Fresh** | Index matches current workspace |
| **Stale** | Files changed — rebuild recommended |
| **Missing** | No index yet — built on first search or manual rebuild |

## Actions

- **Rebuild** — rescan the workspace (may take a minute on large repos)
- **Delete** — remove cached index; auto-rebuilt on next search
- **Open folder** — reveal index cache directory on disk

The cache file lives at **`.deepseek/symbols.json`** per workspace (lazy build).

## When it helps

- Jumping to definitions across a monorepo
- `grep_files` with symbol filters instead of plain text only

Office mode does **not** use the symbol index.

## Tips

- Rebuild after large branch merges or dependency updates.
- If search feels wrong, delete and let the next turn rebuild.

Related: [File tools](/docs/tools/files) · [LHT](/docs/code/lht)
