# Shell tools

**Code** mode can run commands in your workspace via shell tools. Office mode does **not** expose shell execution.

## Main tools

| Tool | Behavior |
|------|----------|
| `exec_shell` | Run a command (foreground or background) |
| `exec_shell_wait` / `exec_wait` | Wait for a background job |
| `exec_shell_interact` / `exec_interact` | Send stdin to a running process |
| `exec_shell_cancel` | Cancel a background shell task |
| `task_shell_start` / `task_shell_wait` | Long-running task helpers |

Output streams to the tool result and often mirrors in the [embedded terminal](/docs/workspace/terminal).

## Approval

Most non-trivial commands trigger the [approval dialog](/docs/desktop/approval-dialog). A **safe prefix dictionary** auto-allows common dev commands (`cargo test`, `npm run`, …) when policy permits.

Configure under [Settings → approval](/docs/settings/approval) and execution mode in System settings.

## Safety notes

- Commands run as your Windows user in the workspace cwd.
- Destructive patterns (`rm -rf`, system paths) should be denied.
- Optional external sandbox may apply when configured.

## Tips

- Prefer `run_tests` or `cargo check` over ad-hoc scripts when available.
- For servers (`npm run dev`), use background `exec_shell` and `exec_shell_wait`.

Related: [File tools](/docs/tools/files) · [CRAFT](/docs/code/craft)
