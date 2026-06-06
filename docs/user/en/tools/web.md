# Web tools

When **network** is enabled, the agent can search the public web and fetch pages.

## Tools

| Tool | Purpose |
|------|---------|
| `web_search` | Query a search API for snippets and URLs |
| `fetch_url` | Download and extract readable text from a URL |
| `web.run` | Browser-style fetch with richer extraction |
| `finance` | Market/finance data helper (when configured) |

Office and Code modes register the same web family when networking is on.

## Network policy

First visit to a new domain may require approval if mode is **prompt**. Allowlists and denylists: [Network policy](/docs/settings/network) (or edit `config.toml` directly).

## Office use

Typical flow: `web_search` for industry news → `fetch_url` for articles → `write_office` for the report. See [P0 competitive](/docs/office/p0-competitive).

## Tips

- Paste URLs directly in chat if you already know the source.
- Fetched content counts toward context — ask for summaries when pages are huge.
- Disable web in air-gapped environments.

Related: [Office I/O](/docs/tools/office-io) · [MCP](/docs/settings/mcp)
