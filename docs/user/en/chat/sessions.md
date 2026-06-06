# Sessions

Zagens organizes work into **sessions** (conversations) scoped to a workspace.

## Session sidebar

- **New session** — fresh thread in the current workspace
- **Session list** — resume past chats for this folder
- **Show all sessions** — optional toggle to include history from other workspaces

Each session remembers messages, tool calls, and thread state in local SQLite storage (via the runtime sidecar).

## Task type and sessions

When you switch **Code ↔ Office** task type, Zagens starts a **new session** so tool surfaces and model KV stay consistent. Plan separate sessions for engineering vs document work.

## Workspace binding

Sessions belong to the active **workspace path**. Changing workspace switches which session list you see.

## Tips

- Name goals clearly in the first message — it helps long threads stay on track.
- Delete obsolete sessions from the sidebar menu when tidying up.
- For support, export JSON before deleting (see [Replay & export](/docs/chat/replay-export)).

Related: [Streaming](/docs/chat/streaming) · [Context usage](/docs/chat/context)
