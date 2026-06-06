# Tool approval

Risky tool calls can pause until you **approve** or **deny** in a desktop dialog (default timeout ~120s).

## What triggers approval

Depends on your **approval policy**, typically:

- Shell commands outside safe prefixes
- File writes in guarded modes
- First use of a network domain (when set to prompt)

## Policy levels (runtime)

| Level | Behavior |
|-------|----------|
| **on-request** | Ask before risky operations |
| **untrusted** | Ask before writes |
| **never** | Auto-approve (use with caution) |
| **auto** | Runtime auto-decides per rules (used with on-request) |

Configure under **Settings → System** alongside **execution policy** (`read-only`, `workspace-write`, …).

## Session memory

Approvals can be **remembered for the session** so repeated safe commands do not nag.

## Tips

- Prefer `workspace-write` for daily coding; reserve `danger-full-access` for trusted maintenance only.
- If a turn hangs, check for a hidden approval dialog behind the main window.

Related: [Streaming](/docs/chat/streaming) · [Embedded terminal](/docs/workspace/terminal)
