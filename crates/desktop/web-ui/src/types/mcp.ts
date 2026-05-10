/** MCP server entry as returned by GET /v1/apps/mcp/servers */
export interface McpServerEntry {
  name: string;
  enabled: boolean;
  required: boolean;
  command: string | null;
  url: string | null;
  transport: string | null;
  connected: boolean;
  tool_count: number;
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
