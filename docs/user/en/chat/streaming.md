# Streaming & stop

Zagens streams assistant output in real time over SSE from the local runtime sidecar.

## What streams

| Stream | Description |
|--------|-------------|
| **Answer text** | Main assistant reply |
| **Thinking** | Optional reasoning chain (provider/model dependent) |
| **Tool calls** | Tool name, arguments, and progress events |
| **Tool results** | Summaries fed back into the model |

Tool cards in the chat timeline show status while commands run.

## Stop generation

Click **Stop** during an active turn to cancel the in-flight request. Partial output may remain in the thread; you can continue with a follow-up message.

Stopping does not roll back file edits already applied — use [workspace snapshots](/docs/workspace/snapshots) or git if you need to undo.

## Connection indicator

The sidebar shows **runtime connection** health. If streaming stalls, check that the sidecar is running and your API key is valid.

## Approvals mid-stream

Some tools pause for **user approval** (shell, network, writes). Respond in the dialog to let the turn continue.

Related: [Sessions](/docs/chat/sessions) · [Settings: approval](/docs/settings/approval)
