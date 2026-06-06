# MCP servers

**MCP** (Model Context Protocol) connects external tools and data sources to the agent via stdio servers.

## MCP panel

Open **Settings → MCP** to:

- Add or edit MCP server definitions
- Enable/disable individual tools exposed by a server
- Apply allow/deny filters per tool name

Changes apply to new tool calls in subsequent turns.

## Typical uses

- Internal APIs or databases wrapped as MCP tools
- Third-party integrations shipping MCP servers
- Complementing built-in `web_search` / file tools

## Safety

MCP tools obey the same **approval** and **network** policies as native tools where applicable. Review server source before enabling.

## Office vs Code

Both task types can use MCP if enabled in config; Office mode still trims engineering-heavy builtins regardless of MCP.

Related: [Skills](/docs/settings/skills) · [Tool approval](/docs/settings/approval)
