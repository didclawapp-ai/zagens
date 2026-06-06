# Network policy

Outbound calls from **`fetch_url`**, **`web_search`**, and MCP HTTP transports can be restricted per domain.

## UI toggle

**Settings → System → Web search** enables or disables the web tool family globally.

## Config file rules

Fine-grained policy lives in `~/.zagens/config.toml` (or `~/.deepseek/config.toml`):

```toml
[network]
default = "prompt"   # allow | deny | prompt
allow = ["api.deepseek.com", "github.com", ".githubusercontent.com"]
deny = []
audit = true
```

| `default` | Behavior for unknown hosts |
|-----------|----------------------------|
| **prompt** | First visit may need [approval](/docs/desktop/approval-dialog) |
| **allow** | Open |
| **deny** | Blocked |

**Deny wins** over allow. Subdomain wildcards: `.example.com` matches `api.example.com`.

## What is not filtered

- LLM API calls to your configured provider
- Stdio MCP servers (local processes)

## Skills installer

`/skill install` needs GitHub hosts reachable — add `github.com` and `raw.githubusercontent.com` to `allow` or accept prompts.

Related: [Web tools](/docs/tools/web) · [MCP](/docs/settings/mcp)
