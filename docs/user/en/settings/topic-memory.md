# Topic memory graph

**Topic memory** builds a lightweight graph of recurring themes across turns and injects relevant snippets back into context.

## Enable

**Settings → System → Advanced → Topic memory** (+ optional **inject interval** in turns).

Office sessions hide the topic memory sidebar tab.

## Graph panel

Sidebar → **Topic memory** shows nodes (topics) and weighted edges. Layout caps visible nodes for readability — matches runtime injection limits.

## How it behaves

- The engine extracts topics from conversation over time
- Every N turns (interval setting), top related notes may appear in the system context
- Complements optional **user memory** (`remember` tool) — topic memory is automatic graphing, not manual notes

## Tips

- Start with interval **5–10** on long research threads.
- Disable on short one-off questions to save tokens.
- Not a substitute for project `AGENTS.md` — use files for stable repo facts.

Related: [Context usage](/docs/chat/context) · [Chat sessions](/docs/chat/sessions)
