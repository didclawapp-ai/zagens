# Approval dialog

When a tool call is considered risky, Zagens pauses and shows a **desktop approval dialog** (not the browser WebView).

## What you see

- Tool name and summary of the action (shell command, file write, network fetch, …)
- **Approve** or **Deny** buttons
- Countdown toward timeout (~120s default) — denied if you do not respond

## Common triggers

- Shell commands outside the safe prefix dictionary
- Writes when policy is `untrusted` or `on-request`
- First visit to a network domain when mode is `prompt`

Configure policies under [Settings → approval](/docs/settings/approval) and **System** execution mode.

## During streaming

The chat stream pauses until you decide. Other UI remains responsive.

## Session memory

Approving once may **remember for this session** so identical safe commands auto-continue.

## Tips

- If the dialog hides behind Zagens, check the taskbar flash.
- Deny and rephrase the goal if the agent proposed an overly broad command (`rm -rf`, system paths, …).

Related: [Streaming](/docs/chat/streaming) · [Embedded terminal](/docs/workspace/terminal)
