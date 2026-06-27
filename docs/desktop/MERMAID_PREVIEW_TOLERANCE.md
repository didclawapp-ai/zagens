# Zagens desktop — Mermaid preview tolerance layer (WebView2)

> **Status (2026-06):** Active engineering. Markdown preview embeds ` ```mermaid ` ` fences via the same engine as the Mermaid panel. This document describes **why** the adapter exists, **how** it works, and **what to regression-test**.

## Problem statement

Mermaid is permissive at the syntax level: the same visual diagram can be authored through **disjoint paths** (default nodes vs `classDef`, `subgraph` titles vs node labels, edge quotes vs node text). Each path produces different SVG structure and CSS reliance.

**Cursor / Chromium** tolerates that variability. **Tauri WebView2** does not reliably apply embedded SVG `<style>`, `foreignObject` HTML, or scaled `viewBox` the same way. Without an adapter, complex flowcharts (e.g. [`tech/RUNTIME_ARCHITECTURE.md`](../tech/RUNTIME_ARCHITECTURE.md) §1.1) show black blocks, missing connectors, clipped layout, or invisible text (selectable but not painted).

**Product goal:** improve **fault tolerance** — authors keep writing normal Mermaid; Zagens normalizes output for WebView2. **No requirement** to change doc syntax (e.g. adding `classDef` to `U1`/`U2`).

## Where it runs

| Surface | Mount | Trust |
|---------|-------|-------|
| Markdown file preview (`MarkdownRenderer`) | `mountMermaidSvgIframe` | `trusted` (lightweight SVG threat scan) |
| Mermaid panel (`MermaidPanel.tsx`) | `mountMermaidSvgInline` | `trusted` |
| Chat bubbles | Fenced source only; no inline render | — |

Primary regression fixture: **`docs/tech/RUNTIME_ARCHITECTURE.md`** — §1.1 (conceptual layers + `classDef`) and §1.2 (code-path detail).

## Mermaid syntax → adapter branches

| Author syntax (MD) | SVG output | Adapter branch |
|--------------------|------------|----------------|
| `U1["Zagens desktop user"]` — **no** `class` line | `<g class="node default">` + theme fill | Default node: shape `fill` luminance → label color |
| `UI[...]` + `class UI product` | `node default product` + inline dark fill | `product` / `contract` / `sidecar` / `ext` / `store` → white text |
| `subgraph entry["User Entry"]` | `cluster-label` + `foreignObject` | Theme `.cluster` / `.cluster-label` span color |
| `SUP -- "spawn + DS_PICK_READY" --> GATE` | `edgeLabel` + `labelBkg` | Gray background + `#333` text |

`<br/>` in fences is normalized to `\n` before render (`normalizeMermaidSourceForSvgLabels`) so labels match Cursor/GitHub.

## Pipeline

```
markdown-it fence (markdownMermaidFence.ts)
  → .ds-mermaid-block placeholder
  → renderMermaidToSvg (mermaidRuntime.ts)
       mermaid.initialize({ htmlLabels: true, securityLevel: 'loose' })
  → patchMermaidSvgForWebView2 (mermaidSvgPostProcess.ts)
  → mountMermaidSvgIframe + fitMermaidIframeSvg on load / resize
```

### Post-process stages (`patchMermaidSvgForWebView2`)

| Stage | Purpose |
|-------|---------|
| `inlineClusterRectPaint` | Subgraph `<rect>` fill/stroke from theme CSS (avoids black clusters) |
| `inlineNodeShapePaint` | Node shapes without paint; classDef + default `.node rect` |
| `fixMermaidBackgroundRects` | Zero-size label backdrop rects |
| `inlineEdgePathStroke` | Connector `stroke` as SVG attributes |
| `inlineForeignObjectLabelColors` | Per-branch text color + `labelBkg` for edges |
| `fixForeignObjectElementBackground` | Transparent `foreignObject` element (WebView2 black box) |
| `normalizeSvgEmUnits` | Legacy `em` on SVG text (htmlLabels:false path) |
| `promoteSvgPresentationAttributes` | `style="fill:…"` → `fill=""` attributes |
| `fixSvgDimensionsForWebView2` | Native viewBox width/height; strip responsive `max-width` |
| `appendWebView2CompatCss` | Minimal overrides inside preserved Mermaid `<style>` |

### Iframe fit (`fitMermaidIframeSvg`)

- Keep SVG at **native viewBox size** inside iframe.
- Apply **`zoom = containerWidth / nativeWidth`** on `#ds-mermaid-scale-wrap` so shapes and `foreignObject` HTML scale together (uniform; avoids clipping from `transform: scale` layout bugs).
- `syncForeignObjectTextPaint` reinforces `color` → `-webkit-text-fill-color` after mount (theme `fill:` on spans must not paint white glyphs on light nodes).

### Label color policy (`resolveNodeLabelTextColor`)

1. Known classDef classes → `#ffffff`
2. Else read shape `fill` on the node block → `contrastingTextColor(fill)` (`#333` on light fills, `#fff` on dark)
3. Fallback `#333333`

This covers **unclassed** entry nodes (`U1`, `U2`) without doc changes.

## Source files

| File | Role |
|------|------|
| `crates/desktop/web-ui/src/lib/mermaidRuntime.ts` | Init, render, block lifecycle; `MERMAID_INIT_REV` cache bust |
| `crates/desktop/web-ui/src/lib/mermaidSvgPostProcess.ts` | WebView2 SVG/HTML patches + iframe fit |
| `crates/desktop/web-ui/src/lib/markdownMermaidFence.ts` | Fence → placeholder HTML |
| `crates/desktop/web-ui/src/components/preview/renderers/MarkdownRenderer.tsx` | Preview mount + theme |
| `crates/desktop/web-ui/src/lib/mermaid.test.ts` | `npm test` (Vitest) |

Bump **`MERMAID_INIT_REV`** in `mermaidRuntime.ts` when init or post-process semantics change so cached diagrams re-render after app update.

## Verification

```bash
cd crates/desktop/web-ui
npm test
npm run build
# Desktop bundle (maintainer)
cd ../.. && npx @tauri-apps/cli@2 build
```

Manual checklist (§1.1):

- [ ] Full diagram visible (no top/bottom clip); width fits preview pane
- [ ] Light default nodes: `Zagens desktop user`, `Scripts / CI / headless HTTP`
- [ ] Subgraph titles: `User Entry`, `L3 — Zagens product shell`, etc.
- [ ] Edge labels: `spawn + DS_PICK_READY`, `bundled embed`
- [ ] classDef nodes: readable white text on blue/orange/purple/green shapes
- [ ] Connectors visible end-to-end

## Known WebView2 failure modes (historical)

| Symptom | Root cause | Mitigation |
|---------|------------|------------|
| Solid black rectangles | CSS `fill` not applied on `<rect>` | Inline SVG presentation attributes |
| Missing edge lines | CSS `stroke` ignored on paths | `inlineEdgePathStroke` |
| Layout offset / overlap | `foreignObject` + parent flex / CSS scale | iframe isolation + `zoom` fit |
| Bottom-clipped labels | `transform: scale` layout vs visual size | Replaced with `zoom` + native SVG size |
| Invisible but selectable text | `fill:` → `-webkit-text-fill-color: white` on light nodes | `forceInlineTextPaint` + runtime sync |
| Edge / cluster text missing | `labelBkg` forced transparent | Restore `rgba(232,232,232,0.85)` for edge labels |

## Open work (continue incrementally)

- Dark app theme + Mermaid `dark` theme interaction on docs with light `classDef` colors
- Additional diagram types in `RUNTIME_ARCHITECTURE.md` (§1.2 detail graph, sequence charts)
- Mermaid panel inline mount parity if zoom-only path differs from iframe
- Optional: golden SVG fixtures in CI (beyond `mermaid.test.ts`)

## Related

- [PREVIEW_ARCHITECTURE.md](./PREVIEW_ARCHITECTURE.md) — preview panel module map
- [CHANGELOG.md](../../CHANGELOG.md) — `[Unreleased]` Desktop (Mermaid preview) entries
