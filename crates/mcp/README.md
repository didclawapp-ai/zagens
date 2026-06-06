# `deepseek-mcp` (deprecated)

**Status:** Legacy / experimental — **not used** by Zagens or the embedded runtime.

Zagens connects to **external** MCP servers as a **client** via:

- `crates/runtime-adapters/src/mcp/` — production MCP client (stdio / SSE / Streamable HTTP)
- Desktop UI: `crates/desktop/web-ui/src/components/McpPanel.tsx`
- HTTP API: `GET /v1/apps/mcp/*` on the runtime sidecar

This crate (`deepseek-mcp`) implements a **builtin MCP server** stdio loop and `McpManager` with a different tool naming scheme (`mcp__server__tool` vs the client chain’s `mcp_{server}_{tool}`). No workspace crate depends on it.

**Do not** add new features here unless explicitly reviving it as a first-class server host. For client work, use `runtime-adapters` MCP modules and [docs/desktop/MCP_ITERATION_PLAN.md](../../docs/desktop/MCP_ITERATION_PLAN.md).
