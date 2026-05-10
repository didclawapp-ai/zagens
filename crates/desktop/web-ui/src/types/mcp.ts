/** MCP server entry as returned by GET /v1/apps/mcp/servers */
export interface McpServerEntry {
  name: string;
  enabled: boolean;
  required: boolean;
  command: string | null;
  url: string | null;
  args: string[];
  transport?: string | null;
  connected: boolean;
  enabled_tools: string[];
  disabled_tools: string[];
}

/** One server block from GET /v1/apps/mcp/servers/{name} (matches `McpServerConfig`). */
export interface McpServerConfigPayload {
  command?: string | null;
  args: string[];
  env: Record<string, string>;
  url?: string | null;
  connect_timeout?: number | null;
  execute_timeout?: number | null;
  read_timeout?: number | null;
  disabled: boolean;
  enabled: boolean;
  required: boolean;
  enabled_tools: string[];
  disabled_tools: string[];
}

export interface McpServersResponse {
  servers: McpServerEntry[];
}

/** MCP tool entry as returned by GET /v1/apps/mcp/tools */
export interface McpToolEntry {
  server: string;
  name: string;
  prefixed_name: string;
  description: string | null;
  input_schema: unknown;
}

export interface McpToolsResponse {
  tools: McpToolEntry[];
}
