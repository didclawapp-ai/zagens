/**
 * WebView2 / Tauri fixes for Mermaid SVG output.
 */

/** Base iframe document rules (outside embedded Mermaid theme CSS). */
export const MERMAID_WEBVIEW2_IFRAME_CSS = [
  'html,body{margin:0;padding:0;background:transparent;color:#333;color-scheme:light}',
  'foreignObject{overflow:visible;background-color:transparent!important}',
  'foreignObject div,foreignObject span,foreignObject p{-webkit-text-fill-color:currentColor!important}',
  'foreignObject span,foreignObject p{fill:none!important}',
].join('');

/** Minimal fixes appended inside preserved Mermaid `<style>` blocks. */
export const MERMAID_WEBVIEW2_SVG_CSS = [
  'svg{display:block;max-width:none!important}',
  'foreignObject{overflow:visible;background-color:transparent!important}',
  'foreignObject div:not(.labelBkg){background:transparent!important}',
  'foreignObject div.labelBkg{background:rgba(232,232,232,0.85)!important;padding:1px 4px!important;border-radius:2px!important}',
  'foreignObject span:not(.edgeLabel){background:transparent!important}',
  'foreignObject p{margin:0}',
  'foreignObject div,foreignObject span,foreignObject p{-webkit-text-fill-color:currentColor!important}',
  'foreignObject span,foreignObject p{fill:none!important}',
  'foreignObject .edgeLabel,.foreignObject .edgeLabel p{color:#333333!important;-webkit-text-fill-color:#333333!important}',
  'g.label>rect[width="0"][height="0"]{fill:none!important;stroke:none!important}',
  'rect.background{fill:none!important;stroke:none!important}',
  '.edgePaths path,.edgePath path,.flowchart-link{fill:none!important}',
  '.contract>rect{stroke-dasharray:5 5!important}',
  '.edge-pattern-dotted{stroke-dasharray:3!important}',
].join('');

/** @deprecated Use MERMAID_WEBVIEW2_SVG_CSS */
export const MERMAID_WEBVIEW2_CSS = MERMAID_WEBVIEW2_SVG_CSS;

/** Split out `<style>` blocks so DOMPurify cannot strip classDef / theme rules. */
export function extractMermaidSvgStyles(svg: string): { body: string; styles: string[] } {
  const styles: string[] = [];
  const body = svg.replace(/<style([^>]*)>([\s\S]*?)<\/style>/gi, (_match, attrs: string, css: string) => {
    const idx = styles.length;
    styles.push(css);
    return `<style${attrs} data-ds-mermaid-style="${idx}">/* mds-${idx} */</style>`;
  });
  return { body, styles };
}

/** Restore preserved styles and append WebView2 compatibility rules. */
export function restoreMermaidSvgStyles(svg: string, styles: string[]): string {
  return svg.replace(
    /<style([^>]*?)data-ds-mermaid-style="(\d+)"([^>]*)>[\s\S]*?<\/style>/gi,
    (_match, before: string, idxRaw: string, after: string) => {
      const css = styles[Number(idxRaw)] ?? '';
      const attrs = `${before}${after}`.trim();
      const open = attrs.length > 0 ? `<style ${attrs}>` : '<style>';
      return `${open}${css}${MERMAID_WEBVIEW2_SVG_CSS}</style>`;
    },
  );
}

/** Join embedded `<style>` text and return SVG body without style tags. */
export function peelMermaidStyles(svg: string): { css: string; svgBody: string } {
  const chunks: string[] = [];
  const svgBody = svg.replace(/<style[^>]*>([\s\S]*?)<\/style>/gi, (_m, css: string) => {
    const trimmed = css.trim();
    if (trimmed.length > 0) {
      chunks.push(trimmed);
    }
    return '';
  });
  return { css: chunks.join('\n'), svgBody };
}

function parseCssDeclarationBlock(block: string): Record<string, string> {
  const props: Record<string, string> = {};
  for (const part of block.split(';')) {
    const idx = part.indexOf(':');
    if (idx < 0) {
      continue;
    }
    const key = part.slice(0, idx).trim().toLowerCase();
    const value = part.slice(idx + 1).trim();
    if (key.length > 0 && value.length > 0) {
      props[key] = value;
    }
  }
  return props;
}

function extractCssRuleProps(css: string, selector: RegExp): Record<string, string> | null {
  for (const block of css.matchAll(/([^{}]+)\{([^}]*)\}/g)) {
    const selectors = block[1].split(',');
    for (const sel of selectors) {
      if (selector.test(sel.trim())) {
        return parseCssDeclarationBlock(block[2]);
      }
    }
  }
  return null;
}

const CLASS_DEF_NAMES = ['product', 'contract', 'sidecar', 'ext', 'store'] as const;
const PAINT_ATTR_KEYS = ['fill', 'stroke', 'stroke-width', 'stroke-dasharray'] as const;

function stripImportant(val: string): string {
  return val.replace(/\s*!important\s*/gi, '').trim();
}

function getMermaidCss(svg: string): string {
  return svg.match(/<style[^>]*>([\s\S]*?)<\/style>/i)?.[1] ?? '';
}

function elementHasPaint(attrs: string): boolean {
  if (/\bfill\s*=/i.test(attrs)) {
    return true;
  }
  const style = (attrs.match(/style="([^"]*)"/) ?? [])[1] ?? '';
  return /fill\s*:/i.test(style);
}

/** WebView2 reliably paints SVG via presentation attributes, not `style="fill:…"`. */
function applyPaintAttrs(attrs: string, paint: Record<string, string>): string {
  let out = attrs;
  for (const key of PAINT_ATTR_KEYS) {
    const raw = paint[key];
    if (!raw) {
      continue;
    }
    const attrRe = new RegExp(`\\b${key.replace('-', '\\-')}\\s*=`, 'i');
    if (attrRe.test(out)) {
      continue;
    }
    out += ` ${key}="${stripImportant(raw)}"`;
  }
  return out;
}

function classShapePaint(css: string, cls: string, tag: string): Record<string, string> | null {
  return extractCssRuleProps(css, new RegExp(`(?:#[\\w-]+\\s+)?\\.${cls}\\s+${tag}`, 'i'));
}

/**
 * Promote `style="fill:…;stroke:…"` (Mermaid classDef output) to `fill=""` / `stroke=""` attributes.
 * Tauri WebView2 often ignores inline style properties on SVG shapes.
 */
export function promoteSvgPresentationAttributes(svg: string): string {
  let out = svg;
  for (const tag of ['rect', 'path', 'circle', 'ellipse', 'polygon']) {
    out = out.replace(
      new RegExp(`<${tag}([^>]*?)(\\/?)>`, 'gi'),
      (match, attrs: string, close: string) => {
        const styleMatch = attrs.match(/style="([^"]*)"/);
        if (!styleMatch) {
          return match;
        }
        const props = parseCssDeclarationBlock(styleMatch[1]);
        if (!props.fill && !props.stroke && !props['stroke-width'] && !props['stroke-dasharray']) {
          return match;
        }
        const newAttrs = applyPaintAttrs(attrs, props);
        return `<${tag}${newAttrs}${close}>`;
      },
    );
  }
  return out;
}

/** Flow nodes without classDef inline styles — paint from `.node rect` / `.product path` theme CSS. */
export function inlineNodeShapePaint(svg: string): string {
  const css = getMermaidCss(svg);
  const defaultRect = extractCssRuleProps(css, /(?:#[\w-]+\s+)?\.node\s+rect/);

  return svg.replace(
    /<g([^>]*class="([^"]*\bnode\b[^"]*)"[^>]*)>(\s*)<(rect|path|circle|ellipse|polygon)([^>]*?)(\/?>)/gi,
    (match, gAttrs, gClasses, ws, tag, shapeAttrs, close) => {
      if (elementHasPaint(shapeAttrs)) {
        return match;
      }

      let paint: Record<string, string> | null = null;
      for (const cls of CLASS_DEF_NAMES) {
        if (new RegExp(`\\b${cls}\\b`).test(gClasses)) {
          paint = classShapePaint(css, cls, tag)
            ?? classShapePaint(css, cls, 'rect')
            ?? classShapePaint(css, cls, 'path');
          break;
        }
      }
      if (!paint && tag === 'rect') {
        paint = defaultRect;
      }
      if (!paint) {
        return match;
      }

      const newShapeAttrs = applyPaintAttrs(shapeAttrs, paint);
      return `<g${gAttrs}>${ws}<${tag}${newShapeAttrs}${close}`;
    },
  );
}

/**
 * Subgraph cluster rects ship with `style=""` and no fill — WebView2 paints them solid black
 * when embedded `<style>` rules do not apply. Inline fill/stroke from Mermaid theme CSS.
 */
export function inlineClusterRectPaint(svg: string): string {
  const styleMatch = svg.match(/<style[^>]*>([\s\S]*?)<\/style>/i);
  const css = styleMatch?.[1] ?? '';
  const clusterProps =
    extractCssRuleProps(css, /(?:#[\w-]+\s+)?\.cluster\s+rect/)
    ?? { fill: '#ffffde', stroke: '#aaaa33', 'stroke-width': '1px' };
  if (Object.keys(clusterProps).length === 0) {
    return svg;
  }

  return svg.replace(
    /(<g[^>]*class="[^"]*\bcluster\b[^"]*"[^>]*>\s*<rect)([^>]*?)(\/?>)/gi,
    (_match, open: string, attrs: string, close: string) => {
      if (elementHasPaint(attrs)) {
        return `${open}${attrs}${close}`;
      }
      const newAttrs = applyPaintAttrs(attrs, clusterProps);
      return `${open}${newAttrs}${close}`;
    },
  );
}

/** Label backdrop rects ship without `fill`; WebView2 defaults them to solid black. */
export function fixMermaidBackgroundRects(svg: string): string {
  return svg.replace(
    /<rect(\s[^>]*\bclass="background"[^>]*?)(\s*\/?>)/gi,
    (match, attrs: string, close: string) => {
      if (/\bfill\s*=/.test(attrs)) {
        return match;
      }
      return `<rect${attrs} fill="none"${close}`;
    },
  );
}

/**
 * Edge / arrow paths rely on theme CSS for `stroke` — WebView2 leaves them invisible.
 * Only touch paths inside `edgePaths`, `flowchart-link`, and `arrowMarkerPath`.
 */
export function inlineEdgePathStroke(svg: string): string {
  const css = getMermaidCss(svg);
  const edgePaint =
    extractCssRuleProps(css, /(?:#[\w-]+\s+)?\.edgePath\s+\.path/)
    ?? extractCssRuleProps(css, /(?:#[\w-]+\s+)?\.flowchart-link/)
    ?? { stroke: '#333333', 'stroke-width': '1px', fill: 'none' };
  const markerPaint =
    extractCssRuleProps(css, /(?:#[\w-]+\s+)?\.marker/)
    ?? { fill: '#333333', stroke: '#333333' };

  let out = svg.replace(
    /(<g class="edgePaths"[^>]*>)([\s\S]*?)(<\/g>)/gi,
    (_m, open: string, body: string, close: string) => {
      const patched = body.replace(
        /<path([^>]*?)>/gi,
        (_pm, attrs: string) => {
          const newAttrs = applyPaintAttrs(attrs, { ...edgePaint, fill: 'none' });
          return `<path${newAttrs}>`;
        },
      );
      return `${open}${patched}${close}`;
    },
  );

  out = out.replace(
    /<path([^>]*class="[^"]*\bflowchart-link\b[^"]*"[^>]*?)>/gi,
    (_m, attrs: string) => {
      const newAttrs = applyPaintAttrs(attrs, edgePaint);
      return `<path${newAttrs}>`;
    },
  );

  out = out.replace(
    /<path([^>]*class="[^"]*\barrowMarkerPath\b[^"]*"[^>]*?)>/gi,
    (_m, attrs: string) => {
      const fill = stripImportant(markerPaint.fill ?? markerPaint.stroke ?? '#333333');
      const stroke = stripImportant(markerPaint.stroke ?? fill);
      const newAttrs = applyPaintAttrs(attrs, {
        fill,
        stroke,
        'stroke-width': markerPaint['stroke-width'] ?? '1px',
      });
      return `<path${newAttrs}>`;
    },
  );

  return out;
}

/** Edge connector paths only — avoid touching node shape paths (cylinder `outer-path`, etc.). */
export function patchEdgePathFill(svg: string): string {
  return svg.replace(
    /<path(\s[^>]*\bclass="[^"]*(?:\bedgePaths\b|\bedgePath\b|flowchart-link)[^"]*"[^>]*?)(\s*\/?>)/gi,
    (match, attrs: string, close: string) => {
      if (/\bfill\s*=/.test(attrs.toLowerCase())) {
        return match;
      }
      return `<path${attrs} fill="none"${close}`;
    },
  );
}

/**
 * WebView2 mishandles `em` on SVG `<tspan>` (htmlLabels:false) — convert to `px` from theme font-size.
 */
export function normalizeSvgEmUnits(svg: string): string {
  const css = getMermaidCss(svg);
  const fontMatch = css.match(/font-size:\s*(\d+(?:\.\d+)?)px/i);
  const fontSize = fontMatch ? Number(fontMatch[1]) : 16;

  return svg.replace(
    /(\s(?:x|y|dx|dy)=")(-?\d+(?:\.\d+)?)em"/gi,
    (_m, prefix: string, emRaw: string) => {
      const px = Number(emRaw) * fontSize;
      const rounded = Math.round(px * 100) / 100;
      return `${prefix}${rounded}px"`;
    },
  );
}

/** Center flowchart node labels when using SVG text (htmlLabels:false). */
export function fixFlowchartLabelTextAnchor(svg: string): string {
  return svg.replace(
    /(<g[^>]*class="[^"]*\bnode\b[^"]*"[^>]*>[\s\S]*?<g[^>]*class="[^"]*\blabel\b[^"]*"[^>]*>[\s\S]*?<text)(?![^>]*\btext-anchor\b)([^>]*>)/gi,
    '$1 text-anchor="middle"$2',
  );
}

function mergeInlineStyle(attrs: string, extra: string): string {
  const styleRe = /style="([^"]*)"/;
  if (styleRe.test(attrs)) {
    return attrs.replace(styleRe, (_m, existing: string) => {
      const merged = existing.trim().length > 0 ? `${existing};${extra}` : extra;
      return `style="${merged}"`;
    });
  }
  return `${attrs} style="${extra}"`;
}

function forceInlineStyleProp(attrs: string, prop: string, value: string): string {
  const propRe = new RegExp(`${prop}\\s*:[^;]+;?`, 'gi');
  const decl = `${prop}:${value}`;
  if (/style="/i.test(attrs)) {
    return attrs.replace(/style="([^"]*)"/i, (_m, existing: string) => {
      const stripped = existing.replace(propRe, '').replace(/;\s*;/g, ';').trim();
      const merged = stripped.length > 0 ? `${stripped};${decl}` : decl;
      return `style="${merged}"`;
    });
  }
  return `${attrs} style="${decl}"`;
}

/** WebView2 maps Mermaid theme `fill:` on spans to `-webkit-text-fill-color` (often white on light nodes). */
function forceInlineTextPaint(attrs: string, textColor: string): string {
  const colorDecl = `color:${textColor}!important`;
  const fillDecl = `-webkit-text-fill-color:${textColor}!important`;
  const svgFillDecl = 'fill:none!important';
  let out = forceInlineStyleProp(attrs, 'color', `${textColor}!important`);
  out = forceInlineStyleProp(out, '-webkit-text-fill-color', `${textColor}!important`);
  if (/style="/i.test(out)) {
    out = out.replace(/style="([^"]*)"/i, (_m, existing: string) => {
      const stripped = existing.replace(/fill\s*:[^;]+;?/gi, '').trim();
      const merged = stripped.length > 0 ? `${stripped};${svgFillDecl}` : svgFillDecl;
      return `style="${merged}"`;
    });
  } else {
    out = `${out} style="${colorDecl};${fillDecl};${svgFillDecl}"`;
  }
  return out;
}

const DARK_NODE_CLASS_NAMES = ['product', 'contract', 'sidecar', 'ext', 'store'] as const;
const DEFAULT_LABEL_COLOR = '#333333';
const DARK_NODE_LABEL_COLOR = '#ffffff';

function parseCssColorChannel(raw: string): number {
  const val = Number(raw.trim());
  if (!Number.isFinite(val)) {
    return 0;
  }
  return val > 1 ? Math.min(255, val) / 255 : Math.min(1, Math.max(0, val));
}

/** Relative luminance (sRGB) — pick label text contrast from shape fill, not classDef name. */
export function contrastingTextColor(fillRaw: string): string {
  const fill = stripImportant(fillRaw).trim();
  let r = 0;
  let g = 0;
  let b = 0;

  const hex = fill.match(/^#([0-9a-f]{3}|[0-9a-f]{6})$/i);
  if (hex) {
    const h = hex[1].length === 3
      ? hex[1].split('').map((c) => c + c).join('')
      : hex[1];
    r = parseInt(h.slice(0, 2), 16) / 255;
    g = parseInt(h.slice(2, 4), 16) / 255;
    b = parseInt(h.slice(4, 6), 16) / 255;
  } else {
    const rgb = fill.match(/^rgb\(\s*([\d.]+)\s*,\s*([\d.]+)\s*,\s*([\d.]+)\s*\)$/i);
    if (rgb) {
      r = parseCssColorChannel(rgb[1]);
      g = parseCssColorChannel(rgb[2]);
      b = parseCssColorChannel(rgb[3]);
    } else {
      return DEFAULT_LABEL_COLOR;
    }
  }

  const luminance = 0.2126 * r + 0.7152 * g + 0.0722 * b;
  return luminance > 0.55 ? DEFAULT_LABEL_COLOR : DARK_NODE_LABEL_COLOR;
}

function readShapeFillFromNodeBlock(nodeBlock: string): string | null {
  const shape = nodeBlock.match(/<(?:rect|path|circle|ellipse|polygon)([^>]*\blabel-container\b[^>]*)>/i)
    ?? nodeBlock.match(/<(?:rect|path|circle|ellipse|polygon)([^>]*)>/i);
  if (!shape) {
    return null;
  }
  const attrs = shape[1];
  const fillAttr = attrs.match(/\bfill="([^"]+)"/i);
  if (fillAttr) {
    return fillAttr[1];
  }
  const styleFill = attrs.match(/style="([^"]*)"/i);
  if (styleFill) {
    const props = parseCssDeclarationBlock(styleFill[1]);
    if (props.fill) {
      return props.fill;
    }
  }
  return null;
}

/**
 * Default nodes (e.g. U1/U2 with no `class` in MD) only have `node default` — no classDef `color:#fff`.
 * Use shape fill luminance; classDef nodes keep explicit white labels.
 */
function resolveNodeLabelTextColor(
  nodeClasses: string,
  labelGroupAttrs: string,
  nodeBlock: string,
): string {
  for (const cls of DARK_NODE_CLASS_NAMES) {
    if (new RegExp(`\\b${cls}\\b`).test(nodeClasses)) {
      return DARK_NODE_LABEL_COLOR;
    }
  }
  if (/color\s*:\s*(?:#fff(?:fff)?|rgb\(\s*255\s*,\s*255\s*,\s*255)/i.test(labelGroupAttrs)) {
    return DARK_NODE_LABEL_COLOR;
  }
  const shapeFill = readShapeFillFromNodeBlock(nodeBlock);
  if (shapeFill) {
    return contrastingTextColor(shapeFill);
  }
  return DEFAULT_LABEL_COLOR;
}

function resolveClusterLabelTextColor(css: string): string {
  const clusterSpan =
    extractCssRuleProps(css, /(?:#[\w-]+\s+)?\.cluster-label\s+span/)
    ?? extractCssRuleProps(css, /(?:#[\w-]+\s+)?\.cluster\s+span/);
  const color = clusterSpan?.color;
  if (color) {
    return stripImportant(color);
  }
  return DEFAULT_LABEL_COLOR;
}

function isEdgeLabelForeignObjectHtml(html: string): boolean {
  return /\blabelBkg\b/.test(html) || /\bedgeLabel\b/.test(html);
}

function patchForeignObjectHtmlContent(html: string, textColor: string): string {
  const edgeLabel = isEdgeLabelForeignObjectHtml(html);
  let out = html;
  out = out.replace(
    /<div((?:(?!>).)*xmlns="http:\/\/www\.w3\.org\/1999\/xhtml"(?:(?!>).)*)>/gi,
    (_m, attrs: string) => {
      let patched = forceInlineTextPaint(attrs, textColor);
      if (edgeLabel || /\blabelBkg\b/.test(attrs)) {
        patched = mergeInlineStyle(
          patched,
          'background:rgba(232,232,232,0.85)!important;padding:1px 4px!important;border-radius:2px!important',
        );
      } else if (!/background/i.test(patched)) {
        patched = mergeInlineStyle(patched, 'background:transparent!important');
      }
      return `<div${patched}>`;
    },
  );
  for (const spanClass of ['nodeLabel', 'edgeLabel'] as const) {
    out = out.replace(
      new RegExp(`<span([^>]*\\b${spanClass}\\b[^>]*)>`, 'gi'),
      (_m, attrs: string) => {
        let patched = forceInlineTextPaint(attrs, textColor);
        if (!edgeLabel && !/background/i.test(patched)) {
          patched = mergeInlineStyle(patched, 'background:transparent!important');
        }
        return `<span${patched}>`;
      },
    );
  }
  out = out.replace(/<p([^>]*)>/gi, (_m, attrs: string) => {
    let patched = forceInlineTextPaint(attrs, textColor);
    const bg = edgeLabel
      ? 'margin:0;background:transparent!important'
      : 'margin:0;background:transparent!important';
    patched = mergeInlineStyle(patched, bg);
    return `<p${patched}>`;
  });
  return out;
}

/** WebView2 paints opaque black behind foreignObject unless the element itself is transparent. */
export function fixForeignObjectElementBackground(svg: string): string {
  return svg.replace(/<foreignObject([^>]*)>/gi, (_m, attrs: string) => {
    if (/style="/i.test(attrs)) {
      if (/background/i.test(attrs)) {
        return `<foreignObject${attrs}>`;
      }
      return `<foreignObject${mergeInlineStyle(attrs, 'background-color:transparent!important;overflow:visible')}>`;
    }
    return `<foreignObject${attrs} style="background-color:transparent!important;overflow:visible">`;
  });
}

/**
 * WebView2 ignores theme CSS on foreignObject HTML — inline label text color per node class.
 */
export function inlineForeignObjectLabelColors(svg: string): string {
  const css = getMermaidCss(svg);
  const clusterTextColor = resolveClusterLabelTextColor(css);

  let out = svg.replace(
    /(<g class="([^"]*\bnode\b[^"]*)"[^>]*>)([\s\S]*?<foreignObject[^>]*>)([\s\S]*?)(<\/foreignObject>)/gi,
    (match, nodeOpen: string, nodeClasses: string, foOpen: string, foInner: string, foClose: string) => {
      const nodeBlock = `${nodeOpen}${foOpen}`;
      const labelGroupAttrs = (nodeBlock.match(/<g class="[^"]*\blabel\b[^"]*"([^>]*)>/i) ?? [])[1] ?? '';
      const textColor = resolveNodeLabelTextColor(nodeClasses, labelGroupAttrs, nodeBlock);
      return `${nodeOpen}${foOpen}${patchForeignObjectHtmlContent(foInner, textColor)}${foClose}`;
    },
  );

  out = out.replace(
    /(<g class="[^"]*\bcluster-label\b[^"]*"[^>]*>[\s\S]*?<foreignObject[^>]*>)([\s\S]*?)(<\/foreignObject>)/gi,
    (match, before: string, foInner: string, close: string) =>
      `${before}${patchForeignObjectHtmlContent(foInner, clusterTextColor)}${close}`,
  );

  out = out.replace(
    /(<g class="[^"]*\bedgeLabel\b[^"]*"[^>]*>[\s\S]*?<foreignObject[^>]*>)([\s\S]*?)(<\/foreignObject>)/gi,
    (match, before: string, foInner: string, close: string) => {
      const hasText = /\bedgeLabel\b/.test(foInner)
        || /<p[^>]*>[^<\s][^<]*</.test(foInner);
      if (!hasText) {
        return match;
      }
      return `${before}${patchForeignObjectHtmlContent(foInner, DEFAULT_LABEL_COLOR)}${close}`;
    },
  );

  return out;
}

/**
 * WebView2 paints foreignObject HTML with opaque black backgrounds unless inline styles are set.
 * Required when htmlLabels:true (same model as Cursor/GitHub).
 */
export function fixForeignObjectInlineStyles(svg: string): string {
  let out = inlineForeignObjectLabelColors(svg);
  out = fixForeignObjectElementBackground(out);
  return out;
}

/** Reinforce text paint after iframe mount (theme `fill:` can still win in WebView2). */
export function syncForeignObjectTextPaint(doc: Document, svg: SVGSVGElement): void {
  for (const fo of svg.querySelectorAll('foreignObject')) {
    fo.setAttribute('style', 'overflow:visible;background-color:transparent!important');
    for (const el of fo.querySelectorAll('div, span, p')) {
      const htmlEl = el as HTMLElement;
      const inlineColor = htmlEl.style.color;
      if (!inlineColor) {
        continue;
      }
      htmlEl.style.setProperty('-webkit-text-fill-color', inlineColor, 'important');
      htmlEl.style.setProperty('fill', 'none', 'important');
    }
  }
}

/**
 * WebView2 misaligns `foreignObject` labels when Mermaid's responsive `max-width` scales the SVG.
 * Use explicit viewBox dimensions and let the container scroll horizontally.
 */
export function fixSvgDimensionsForWebView2(svg: string): string {
  const viewBoxRaw = svg.match(/\bviewBox="([^"]+)"/i)?.[1];
  if (!viewBoxRaw) {
    return svg;
  }
  const parts = viewBoxRaw.trim().split(/\s+/).map(Number);
  if (parts.length < 4 || parts[2] <= 0 || parts[3] <= 0) {
    return svg;
  }
  const w = String(parts[2]);
  const h = String(parts[3]);

  return svg.replace(/<svg([^>]*)>/i, (_match, attrs: string) => {
    let a = attrs
      .replace(/\bwidth="[^"]*"/gi, '')
      .replace(/\bheight="[^"]*"/gi, '');

    const styleMatch = a.match(/\bstyle="([^"]*)"/i);
    if (styleMatch) {
      const cleaned = styleMatch[1]
        .replace(/max-width\s*:[^;]+;?/gi, '')
        .replace(/background-color\s*:[^;]+;?/gi, '')
        .trim();
      if (cleaned.length > 0) {
        a = a.replace(/\bstyle="[^"]*"/i, `style="${cleaned}"`);
      } else {
        a = a.replace(/\s*style="[^"]*"/i, '');
      }
    }

    return `<svg${a} width="${w}" height="${h}">`;
  });
}

function appendWebView2CompatCss(svg: string): string {
  if (svg.includes('/* ds-webview2 */')) {
    return svg;
  }
  if (/<style/i.test(svg)) {
    return svg.replace(
      /(<style[^>]*>)([\s\S]*?)(<\/style>)/i,
      `$1$2/* ds-webview2 */${MERMAID_WEBVIEW2_SVG_CSS}$3`,
    );
  }
  return svg.replace(/<svg/i, `<svg><style>/* ds-webview2 */${MERMAID_WEBVIEW2_SVG_CSS}</style>`);
}

/** Patch label placeholders only — do not rewrite node / cluster rects (theme CSS sets those). */
export function patchMermaidSvgForWebView2(svg: string): string {
  let out = svg;
  out = inlineClusterRectPaint(out);
  out = inlineNodeShapePaint(out);
  out = out.replace(
    /<rect\s*\/>/g,
    '<rect fill="none" stroke="none" width="0" height="0"/>',
  );
  out = fixMermaidBackgroundRects(out);
  out = patchEdgePathFill(out);
  out = inlineEdgePathStroke(out);
  out = fixForeignObjectInlineStyles(out);
  out = normalizeSvgEmUnits(out);
  out = fixFlowchartLabelTextAnchor(out);
  out = promoteSvgPresentationAttributes(out);
  out = fixSvgDimensionsForWebView2(out);
  out = appendWebView2CompatCss(out);
  return out;
}

/** Parse Mermaid SVG viewBox width/height (native pixel size). */
export function parseSvgViewBoxSize(svgMarkup: string): { width: number; height: number } | null {
  const viewBoxRaw = svgMarkup.match(/\bviewBox="([^"]+)"/i)?.[1];
  if (!viewBoxRaw) {
    return null;
  }
  const parts = viewBoxRaw.trim().split(/\s+/).map(Number);
  if (parts.length < 4 || parts[2] <= 0 || parts[3] <= 0) {
    return null;
  }
  return { width: parts[2], height: parts[3] };
}

function svgHeightFromBody(svgBody: string): number {
  const size = parseSvgViewBoxSize(svgBody);
  if (size) {
    return Math.ceil(size.height);
  }
  const height = /\bheight\s*=\s*"([\d.]+)/i.exec(svgBody);
  if (height) {
    const h = Number(height[1]);
    if (h > 0) {
      return Math.ceil(h);
    }
  }
  return 480;
}

/** Display height when an SVG viewBox is scaled to fit `containerWidth`. */
export function scaledSvgDisplayHeight(svgMarkup: string, containerWidth: number): number {
  const size = parseSvgViewBoxSize(svgMarkup);
  if (!size || containerWidth <= 0) {
    return 480;
  }
  return Math.ceil(size.height * (containerWidth / size.width));
}

function resolveMermaidFitContainerWidth(
  iframe: HTMLIFrameElement,
  mount?: HTMLElement,
): number {
  const widths = [
    iframe.clientWidth,
    mount?.clientWidth,
    mount?.parentElement?.clientWidth,
    iframe.parentElement?.clientWidth,
  ];
  for (const w of widths) {
    if (w != null && w > 0) {
      return w;
    }
  }
  return 0;
}

function readSvgNativeSize(svg: SVGSVGElement): { width: number; height: number } | null {
  if (!svg.dataset.dsNatW || !svg.dataset.dsNatH) {
    const vb = svg.viewBox?.baseVal;
    const attrW = Number(svg.getAttribute('width'));
    const attrH = Number(svg.getAttribute('height'));
    const natW = vb && vb.width > 0 ? vb.width : attrW;
    const natH = vb && vb.height > 0 ? vb.height : attrH;
    if (natW > 0 && natH > 0) {
      svg.dataset.dsNatW = String(natW);
      svg.dataset.dsNatH = String(natH);
    }
  }
  const width = Number(svg.dataset.dsNatW);
  const height = Number(svg.dataset.dsNatH);
  if (width > 0 && height > 0) {
    return { width, height };
  }
  return null;
}

/**
 * Scale iframe diagram uniformly (nodes, foreignObject labels, edge text) via CSS `zoom`.
 * WebView2 does not scale foreignObject HTML when only SVG `width`/`height` + viewBox change.
 */
export function fitMermaidIframeSvg(iframe: HTMLIFrameElement, mount?: HTMLElement): void {
  const doc = iframe.contentDocument;
  if (!doc) {
    return;
  }
  const svg = doc.querySelector('svg');
  if (!svg) {
    return;
  }

  const native = readSvgNativeSize(svg);
  if (!native) {
    return;
  }

  const containerW = resolveMermaidFitContainerWidth(iframe, mount);
  if (containerW <= 0) {
    return;
  }

  const scale = containerW / native.width;
  const displayW = Math.ceil(containerW);
  const displayH = Math.ceil(native.height * scale);

  svg.setAttribute('width', String(native.width));
  svg.setAttribute('height', String(native.height));
  svg.removeAttribute('preserveAspectRatio');

  const wrap = doc.getElementById('ds-mermaid-scale-wrap');
  if (wrap) {
    const wrapEl = wrap as HTMLElement;
    wrapEl.style.transform = '';
    wrapEl.style.transformOrigin = '';
    wrapEl.style.width = `${native.width}px`;
    wrapEl.style.height = `${native.height}px`;
    wrapEl.style.zoom = String(scale);
    wrapEl.style.overflow = 'visible';
  }

  syncForeignObjectTextPaint(doc, svg);

  iframe.style.height = `${displayH}px`;
  iframe.style.width = '100%';
  iframe.style.overflow = 'hidden';
  doc.documentElement.style.overflow = 'hidden';
  doc.body.style.overflow = 'hidden';
  doc.body.style.margin = '0';
  doc.body.style.width = `${displayW}px`;
}

/** Build isolated HTML document — theme CSS in `<head>` matches Cursor / browser preview. */
export function buildMermaidIframeDoc(sanitizedSvg: string): string {
  const { css, svgBody } = peelMermaidStyles(sanitizedSvg);
  const escaped = css.replace(/<\/style/gi, '<\\/style');
  return (
    '<!DOCTYPE html><html><head><meta charset="utf-8">'
    + '<meta name="viewport" content="width=device-width, initial-scale=1.0">'
    + `<style>${MERMAID_WEBVIEW2_IFRAME_CSS}${escaped}${MERMAID_WEBVIEW2_SVG_CSS}</style></head>`
    + '<body style="margin:0;padding:0;background:transparent;overflow:hidden">'
    + '<div id="ds-mermaid-scale-wrap" style="transform-origin:top left;display:block">'
    + `${svgBody}</div></body></html>`
  );
}

/** Inline SVG (styles stay inside `<svg>`) — used by zoom/pan Mermaid panel. */
export function mountMermaidSvgInline(target: HTMLElement, sanitizedSvg: string): void {
  target.replaceChildren();
  const host = document.createElement('div');
  host.innerHTML = sanitizedSvg;
  while (host.firstChild) {
    target.appendChild(host.firstChild);
  }
}

/**
 * Isolated iframe document — WebView2 applies Mermaid theme/classDef CSS reliably
 * (same model as Cursor/GitHub preview). Used by Markdown file preview.
 */
export function mountMermaidSvgIframe(target: HTMLElement, sanitizedSvg: string): void {
  target.replaceChildren();
  const iframe = document.createElement('iframe');
  iframe.className = 'ds-mermaid-iframe';
  iframe.setAttribute('title', 'Mermaid diagram');
  iframe.setAttribute('loading', 'lazy');
  iframe.setAttribute('scrolling', 'no');
  iframe.style.width = '100%';
  iframe.style.border = '0';
  iframe.style.display = 'block';
  iframe.style.minHeight = '4rem';
  const svgBody = peelMermaidStyles(sanitizedSvg).svgBody;
  const block = target.closest<HTMLElement>('.ds-mermaid-block');
  const estW = block?.clientWidth || target.clientWidth || 0;
  iframe.style.height = estW > 0
    ? `${scaledSvgDisplayHeight(svgBody, estW)}px`
    : `${svgHeightFromBody(svgBody)}px`;
  iframe.srcdoc = buildMermaidIframeDoc(sanitizedSvg);

  const applyFit = () => {
    try {
      fitMermaidIframeSvg(iframe, target);
    } catch {
      /* srcdoc is same-origin; guard for test environments without layout */
    }
  };

  iframe.addEventListener('load', () => {
    applyFit();
    requestAnimationFrame(applyFit);
    window.setTimeout(applyFit, 0);
    window.setTimeout(applyFit, 120);
  });

  if (typeof ResizeObserver !== 'undefined') {
    const ro = new ResizeObserver(() => {
      applyFit();
    });
    ro.observe(target);
    if (block) {
      ro.observe(block);
    }
    iframe.addEventListener('load', () => {
      ro.disconnect();
      ro.observe(target);
      if (block) {
        ro.observe(block);
      }
    }, { once: true });
  }

  target.appendChild(iframe);
}

/** @deprecated Use mountMermaidSvgIframe or mountMermaidSvgInline */
export function mountMermaidSvg(target: HTMLElement, sanitizedSvg: string): void {
  mountMermaidSvgIframe(target, sanitizedSvg);
}

/** @deprecated Hoist path replaced by iframe mount */
export function prepareMermaidMountMarkup(svg: string): string {
  const { css, svgBody } = peelMermaidStyles(svg);
  if (!css) {
    return svgBody;
  }
  return `<style class="ds-mermaid-hoisted-styles">${css}</style>${svgBody}`;
}
