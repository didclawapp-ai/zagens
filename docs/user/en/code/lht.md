# LHT long-horizon tasks

**LHT** (long-horizon task) helps Code mode run **multi-step engineering work** across many turns without losing the plot.

## What you get

- **Checklist** sidebar when the model uses `checklist_write`
- **Long-horizon panel** with macro/micro progress and cycle handoffs
- Optional **early cycle** bands when context pressure rises

Think of LHT as structured pacing for refactors, test sweeps, or audit fixes — not for one-shot questions.

## When to use LHT

| Good fit | Poor fit |
|----------|----------|
| Multi-file refactor with verification | Single-file typo fix |
| CRAFT-style review loops | Office DOCX delivery |
| Hours-long guided implementation | Quick web search |

## Settings

**Settings → LHT settings** mirrors the panel’s four sections: harness presets, long-horizon harness, completion gate, macro review loop — full field reference in **[LHT settings](/docs/settings/lht)** (composer overrides, defaults, disabled states, `config.toml` map).

Click the **LHT** chip above the composer to cycle **LHT → LHT·strict → LHT·off** (per-turn override in `~/.zagens/settings.toml`).

## UI location

Enable the **audit grid** for checklist, [audit scratchpad](/docs/code/audit-scratchpad), LHT graph, and sub-agents together ([UI tour](/docs/ui-tour)). Title bar **Show/Hide audit grid** appears in Code when harness data exists.

Related: [CRAFT](/docs/code/craft) · [Context usage](/docs/chat/context) · [Code mode](/docs/code-mode)
