# Usage & cost

The **Usage** inspector summarizes token spend and estimated cost per session.

## Open usage

Sidebar → **Usage** (chart icon) when the runtime is connected.

## What you see

- Totals: input / output / cached tokens, estimated **USD** cost
- **Group by:** day, model, provider, or thread
- Cache hit rate when prompt caching applies

Display currency for labels can be USD or CNY under **Settings → System** (`cost_currency`) — backend costs remain USD-based.

## Data source

Each completed turn records usage from the provider response. Estimates use bundled pricing tables; self-hosted gateways may show $0 or approximate values.

## Tips

- Compare **Flash vs Pro** routing with [model routing](/docs/settings/routing).
- Long LHT runs — watch context % in [chat context](/docs/chat/context) alongside cost.
- Usage is local to your machine; not sent to zagens.com.

Related: [API key](/docs/settings/api-key) · [Context usage](/docs/chat/context)
