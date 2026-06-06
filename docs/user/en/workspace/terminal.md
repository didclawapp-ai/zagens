# Embedded terminal

In **Code** task type, Zagens embeds an **xterm.js** terminal bound to your workspace directory.

## When to use it

- Watch long-running commands the agent starts
- Run manual commands alongside the agent
- Debug build/test output in real time

The terminal is **not** available in Office mode — document workflows use `read_office` / `write_office` instead.

## Safety

Shell execution follows your **execution policy** (e.g. workspace-write). Risky commands may trigger an **approval dialog** before running.

Configure policy and approvals under **Settings → System**.

## Tips

- Keep one workspace per repo so the terminal cwd matches the project.
- If output stalls, check the runtime connection indicator in the sidebar.
- Prefer asking the agent to run repeatable commands so actions stay in the session replay.

Related: [Code mode](/docs/code-mode) · [Workspace overview](/docs/workspace/overview)
