# Git tools

In **Code** mode the agent can inspect repository state without leaving the chat.

## Available tools

| Tool | Typical use |
|------|-------------|
| `git_status` | Staged/unstaged summary |
| `git_diff` | View changes (optionally scoped) |
| `git_log` | Recent commits |
| `git_show` | One commit or file at a revision |
| `git_blame` | Line attribution |

These are **read-oriented** helpers for context — the agent may still use `exec_shell` for `git commit` / `git push` when you ask, subject to approval.

## When to use

- "What did we change since yesterday?"
- "Summarize the last 5 commits on this branch"
- Before proposing a patch, align with current `git diff`

## Office mode

Git tools are **not** registered for Office task type. Use Code mode for repo work.

## Tips

- Set workspace root to your repo root (or monorepo subfolder) so paths resolve correctly.
- Pair with [snapshots](/docs/workspace/snapshots) before risky refactors.

Related: [File tools](/docs/tools/files) · [Shell](/docs/tools/shell)
