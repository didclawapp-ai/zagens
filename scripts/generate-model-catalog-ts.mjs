#!/usr/bin/env node
/**
 * Generate TypeScript model catalog helpers from crates/shared-defs/model-catalog.json.
 * Mirrors zagens-core match semantics (lowercase; first family wins).
 */
import fs from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..");
const src = path.join(root, "crates/shared-defs/model-catalog.json");
const outDir = path.join(root, "crates/desktop/web-ui/src/lib/generated");
const out = path.join(outDir, "modelCatalog.ts");

const catalog = JSON.parse(fs.readFileSync(src, "utf8"));

function family(id) {
  const f = catalog.families.find((x) => x.id === id);
  if (!f) throw new Error(`missing family ${id} in model-catalog.json`);
  return f;
}

const deepseekV4 = family("deepseek_v4");
const kimiK3 = family("kimi_k3");
const agnesChat = family("agnes_chat");
const defaults = catalog.defaults;

const header = `/* eslint-disable */
/**
 * AUTO-GENERATED from crates/shared-defs/model-catalog.json
 * Do not edit by hand — run \`just model-catalog\` (or \`node scripts/generate-model-catalog-ts.mjs\`).
 */
`;

const body = `
export type MatchLeaf = {
  contains?: string;
  starts_with?: string;
  equals?: string;
  not_contains?: string;
};

export type MatchNode =
  | MatchLeaf
  | { all: MatchNode[] }
  | { any: MatchNode[] };

export type ModelFamily = {
  id: string;
  match: MatchNode;
  context_window: number;
  max_output: number;
  default_max_output?: number;
  omit_sampling?: boolean;
  always_thinking?: boolean;
  thinking_supported?: boolean;
  /** UI/config effort alias → wire reasoning_effort (always-on thinking families). */
  effort_map?: Record<string, string>;
};

export type ModelCaps = {
  familyId: string | null;
  contextWindow: number;
  maxOutput: number;
  defaultMaxOutput?: number;
  omitSampling: boolean;
  alwaysThinking: boolean;
  thinkingSupported: boolean;
  hasEffortMap: boolean;
  effortMap: Record<string, string> | null;
};

export const MODEL_CATALOG = ${JSON.stringify(catalog, null, 2)} as const;

/** Catalog default context window (unknown / unmatched models). */
export const DEFAULT_CONTEXT_WINDOW_TOKENS: number = ${defaults.context_window};
/** Catalog default max output (unknown / unmatched models). */
export const DEFAULT_MAX_OUTPUT_TOKENS: number = ${defaults.max_output};

/** deepseek_v4 family — from model-catalog.json */
export const DEEPSEEK_V4_CONTEXT_WINDOW_TOKENS: number = ${deepseekV4.context_window};
export const DEEPSEEK_V4_MAX_OUTPUT_TOKENS: number = ${deepseekV4.max_output};

/** kimi_k3 family — from model-catalog.json */
export const KIMI_K3_CONTEXT_TOKENS: number = ${kimiK3.context_window};
export const KIMI_K3_MAX_OUTPUT_TOKENS: number = ${kimiK3.max_output};
export const KIMI_K3_DEFAULT_MAX_TOKENS: number = ${kimiK3.default_max_output ?? kimiK3.max_output};

/** agnes_chat family — from model-catalog.json */
export const AGNES_CHAT_CONTEXT_TOKENS: number = ${agnesChat.context_window};
export const AGNES_CHAT_MAX_OUTPUT_TOKENS: number = ${agnesChat.max_output};

function evalMatch(node: MatchNode, modelLower: string): boolean {
  if ("all" in node && Array.isArray(node.all)) {
    return node.all.every((n) => evalMatch(n, modelLower));
  }
  if ("any" in node && Array.isArray(node.any)) {
    return node.any.some((n) => evalMatch(n, modelLower));
  }
  const leaf = node as MatchLeaf;
  let ok = true;
  let anyPred = false;
  if (leaf.contains != null) {
    anyPred = true;
    ok = ok && modelLower.includes(leaf.contains.toLowerCase());
  }
  if (leaf.starts_with != null) {
    anyPred = true;
    ok = ok && modelLower.startsWith(leaf.starts_with.toLowerCase());
  }
  if (leaf.equals != null) {
    anyPred = true;
    ok = ok && modelLower === leaf.equals.toLowerCase();
  }
  if (leaf.not_contains != null) {
    anyPred = true;
    ok = ok && !modelLower.includes(leaf.not_contains.toLowerCase());
  }
  return anyPred && ok;
}

/** Resolve capability flags for a model id (same rules as zagens-core). */
export function resolveModelCaps(model: string): ModelCaps {
  const lower = model.toLowerCase();
  for (const family of MODEL_CATALOG.families as unknown as ModelFamily[]) {
    if (evalMatch(family.match, lower)) {
      const effortMap =
        family.effort_map && Object.keys(family.effort_map).length > 0
          ? { ...family.effort_map }
          : null;
      return {
        familyId: family.id,
        contextWindow: family.context_window,
        maxOutput: family.max_output,
        defaultMaxOutput: family.default_max_output,
        omitSampling: Boolean(family.omit_sampling),
        alwaysThinking: Boolean(family.always_thinking),
        thinkingSupported: Boolean(family.thinking_supported),
        hasEffortMap: effortMap != null,
        effortMap,
      };
    }
  }
  const d = MODEL_CATALOG.defaults;
  return {
    familyId: null,
    contextWindow: d.context_window,
    maxOutput: d.max_output,
    defaultMaxOutput: (d as { default_max_output?: number }).default_max_output,
    omitSampling: Boolean(d.omit_sampling),
    alwaysThinking: Boolean(d.always_thinking),
    thinkingSupported: Boolean(d.thinking_supported),
    hasEffortMap: false,
    effortMap: null,
  };
}

/**
 * Map UI/config effort through the family's \`effort_map\` (mirrors Rust
 * \`map_model_reasoning_effort\`). Returns null when the family has no map.
 */
export function mapReasoningEffort(model: string, effort: string): string | null {
  const caps = resolveModelCaps(model);
  if (!caps.effortMap) return null;
  const key = effort.trim().toLowerCase() || "default";
  return caps.effortMap[key] ?? caps.effortMap["default"] ?? "max";
}

/** Hide Settings \`off\` when thinking cannot be disabled (always_thinking / effort_map). */
export function hidesEffortOff(model: string): boolean {
  const caps = resolveModelCaps(model);
  return caps.alwaysThinking || caps.hasEffortMap;
}

export function isDeepSeekV4Model(model: string): boolean {
  return resolveModelCaps(model).familyId === "deepseek_v4";
}

export function isKimiK3Model(model: string): boolean {
  return resolveModelCaps(model).familyId === "kimi_k3";
}
`;

fs.mkdirSync(outDir, { recursive: true });
fs.writeFileSync(out, header + body, "utf8");
console.log(`OK: ${path.relative(root, out)}`);
