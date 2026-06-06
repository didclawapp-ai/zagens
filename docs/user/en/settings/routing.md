# Model routing

**Routing** maps an **intent** label to a **model** so different prompts can use different models automatically.

## Open routing

**Settings → Routing** (requires a connected runtime).

## Rules

Each rule is `intent → model`, for example:

| Intent | Model (example) |
|--------|-----------------|
| `code` | `deepseek-v4-pro` |
| `chat` | `deepseek-v4-flash` |
| `research` | `deepseek-v4-pro` |

Add, edit, or remove rules in the panel. Duplicate intents are rejected.

## Using intents in chat

Pick a **route intent** on the composer (when exposed) or pass intent in API/advanced flows. On turn start the runtime matches the intent and selects the configured model.

## Tips

- Keep intents short and stable (`code`, `office`, `fast`).
- Flash models for summaries; Pro models for multi-file edits.
- Routing does not replace [API key](/docs/settings/api-key) provider setup — models must exist for your provider.

Related: [API key & providers](/docs/settings/api-key) · [Usage dashboard](/docs/settings/usage)
