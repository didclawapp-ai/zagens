# File tools

The agent reads and edits files inside your **workspace root** using built-in file tools.

## Core tools

| Tool | Purpose |
|------|---------|
| `read_file` | Read text or Office/PDF content (with limits) |
| `write_file` | Create or overwrite a file |
| `edit_file` | Targeted search/replace edits |
| `apply_patch` | Unified-diff style patches |
| `list_dir` | List a directory |
| `file_info` | Metadata (size, modified time) |

## Search & discovery

| Tool | Code mode | Office mode |
|------|-----------|-------------|
| `glob_files` | ✅ | ✅ |
| `file_search` | ✅ | ✅ |
| `grep_files` | ✅ (ripgrep) | ❌ |

**Code** workflow: `glob_files` → `grep_files` → `read_file` (see [LHT](/docs/code/lht)).

**Office** uses `glob_files` / `file_search` only — no `grep_files` or shell `grep`.

## Safety

- Paths are canonicalized; `..` escapes are blocked.
- Writes may require [approval](/docs/settings/approval) depending on policy.
- Large outputs may be truncated or routed to scratchpad.

## In the UI

Changed files appear in the [diff panel](/docs/workspace/diff) and [file preview](/docs/workspace/preview).

Related: [Git tools](/docs/tools/git) · [Office I/O](/docs/tools/office-io)
