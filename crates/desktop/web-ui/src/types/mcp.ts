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
  /** Transport selector: 'stdio' | 'sse' | 'http'. Omit to infer from command/url. */
  transport?: string | null;
  /** HTTP headers for remote servers; prefer `${ENV_VAR}` over plaintext secrets. */
  headers?: Record<string, string>;
  /** Bearer / API key auth shorthand (secrets omitted on GET when redacted). */
  auth?: {
    type?: string | null;
    token?: string | null;
    header?: string | null;
    apiKey?: string | null;
  } | null;
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

/** Discovered tool/resource/prompt from GET /v1/apps/mcp/discover */
export interface McpDiscoveredItem {
  name: string;
  model_name: string;
  description: string | null;
  enabled: boolean;
}

export interface McpServerDiscoverEntry {
  name: string;
  enabled: boolean;
  required: boolean;
  transport: string;
  command_or_url: string;
  connected: boolean;
  error: string | null;
  tools: McpDiscoveredItem[];
  resources: McpDiscoveredItem[];
  prompts: McpDiscoveredItem[];
}

export interface McpManagerSnapshot {
  config_path: string;
  config_exists: boolean;
  restart_required: boolean;
  servers: McpServerDiscoverEntry[];
}

export interface McpCallRecord {
  timestamp_ms: number;
  server: string;
  method: string;
  duration_ms: number;
  success: boolean;
  error?: string | null;
  result_bytes: number;
}

export interface McpDiscoverResponse {
  snapshot: McpManagerSnapshot;
  recent_calls: McpCallRecord[];
}
