# Tauri / Rust reference (persistence + AGENTS.md)

Published as [topic-memory-graph](https://github.com/didclawapp-ai/topic-memory-graph). DidClaw implements persistence with `read_pheromone_graph`, `write_pheromone_graph`, and
`inject_pheromone_agents_md` in `didclaw-ui/src-tauri/src/pheromone.rs`. When
adapting for your app:

1. **JSON path** — Point to any file (e.g. `app_config_dir()/memory-graph.json`).
2. **Markers** — Use `DEFAULT_MARKERS` or `DIDCLAW_PHEROMONE_MARKERS` from the
   npm package’s `inject` API; Rust must use the **same** HTML comment strings.
3. **Injection** — Either call the TypeScript `injectMemorySection()` from a
   sidecar/script, or port the string splice logic (see `inject.ts`) to Rust.

The TypeScript package stays **host-agnostic**: no Tauri dependency.
