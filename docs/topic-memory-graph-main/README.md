# topic-memory-graph

**[中文说明](README.zh.md)**

TypeScript library that builds a **topic graph** from chat text (with simple decay,
associations, and optional mood/trail signals) and renders a **Markdown section**
suitable for injection into `AGENTS.md` or similar agent context files.

Extracted from the [DidClaw](https://github.com/) client (LCLAW monorepo) for reuse
and standalone publishing. **Zero runtime dependencies.**

## Screenshots

The [DidClaw](https://github.com/) desktop app includes a **Cognitive Map (Experimental)** UI on top of the same graph data: a force-directed network and a structured panel with hot topics and associations.

![Topic graph — network view](assets/cognitive-map-network.png)

![Topic graph — topics & associations panel](assets/cognitive-map-panel.png)

## Install

```bash
pnpm add topic-memory-graph
# or: npm install topic-memory-graph
```

## Quick usage

```ts
import {
  emptyGraph,
  updateGraph,
  applyDecay,
  generateMemorySection,
  shouldInjectMemory,
  injectMemorySection,
  DEFAULT_MARKERS,
  DEFAULT_INJECT_INTERVAL_RUNS,
} from "topic-memory-graph";

let graph = emptyGraph();
graph = updateGraph(graph, "user message", "assistant reply");

const md = generateMemorySection(graph, { attribution: "MyApp" });
const agentsBody = injectMemorySection(existingAgentsMd, md, DEFAULT_MARKERS);
```

Persist `graph` as JSON however you like (filesystem, Tauri, IndexedDB). See
`integrations/tauri-reference.md` for notes aligned with the DidClaw desktop app.

## API highlights

| Export | Role |
|--------|------|
| `extractTopics`, `updateGraph`, `applyDecay` | Core graph maintenance |
| `generateMemorySection(graph, { attribution? })` | Markdown for agents |
| `shouldInjectMemory(graph, runsSinceLastInject, minRuns?)` | When to re-inject |
| `injectMemorySection(md, content, markers?)` | Safe replace/append via HTML comments |
| `DEFAULT_MARKERS`, `DIDCLAW_PHEROMONE_MARKERS` | Marker pairs for injection |

## Graph shape

`PheromoneGraph` is versioned JSON (`GRAPH_SCHEMA_VERSION`). Hosts should preserve
unknown fields if they merge with future library versions.

## Emotion modes

`A` / `B` / `C` / `N` are **heuristic** labels from regex signals on user text (e.g.
narrow focus vs expansive vs low-energy). They are not clinical sentiment scores.

## License

[MIT](LICENSE). (The wider LCLAW monorepo may use other licenses for unrelated code.)

## Developing this package

```bash
cd packages/cognitive-memory-graph
pnpm install
pnpm test
pnpm build
```

Standalone clone: `git clone https://github.com/didclawapp-ai/topic-memory-graph.git`. In the LCLAW monorepo the same sources currently live under `packages/cognitive-memory-graph` (rename locally if you prefer to match this repo name).
