/* eslint-disable */
/**
 * AUTO-GENERATED from crates/shared-defs/model-catalog.json
 * Do not edit by hand — run `just model-catalog` (or `node scripts/generate-model-catalog-ts.mjs`).
 */

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

export const MODEL_CATALOG = {
  "schema_version": 1,
  "defaults": {
    "context_window": 128000,
    "max_output": 65536,
    "omit_sampling": false,
    "always_thinking": false,
    "thinking_supported": false
  },
  "families": [
    {
      "id": "deepseek_v4",
      "match": {
        "all": [
          {
            "contains": "deepseek"
          },
          {
            "any": [
              {
                "contains": "v4-pro"
              },
              {
                "contains": "v4-flash"
              },
              {
                "contains": "v4pro"
              },
              {
                "contains": "v4flash"
              },
              {
                "all": [
                  {
                    "contains": "v4"
                  },
                  {
                    "not_contains": "v3"
                  }
                ]
              }
            ]
          }
        ]
      },
      "context_window": 1000000,
      "max_output": 393216,
      "thinking_supported": true
    },
    {
      "id": "kimi_k3",
      "match": {
        "any": [
          {
            "contains": "kimi-k3"
          },
          {
            "starts_with": "kimi-k"
          }
        ]
      },
      "context_window": 1000000,
      "max_output": 1048576,
      "default_max_output": 131072,
      "omit_sampling": true,
      "always_thinking": true,
      "thinking_supported": true,
      "effort_map": {
        "low": "low",
        "minimal": "low",
        "high": "high",
        "medium": "high",
        "mid": "high",
        "default": "high",
        "xhigh": "max",
        "max": "max",
        "highest": "max",
        "off": "max",
        "disabled": "max",
        "none": "max",
        "false": "max"
      }
    },
    {
      "id": "agnes_chat",
      "match": {
        "all": [
          {
            "contains": "agnes"
          },
          {
            "not_contains": "image"
          },
          {
            "not_contains": "video"
          },
          {
            "not_contains": "embed"
          }
        ]
      },
      "context_window": 256000,
      "max_output": 65536
    },
    {
      "id": "deepseek_legacy",
      "match": {
        "contains": "deepseek"
      },
      "context_window": 128000,
      "max_output": 65536
    },
    {
      "id": "claude",
      "match": {
        "contains": "claude"
      },
      "context_window": 200000,
      "max_output": 65536
    },
    {
      "id": "qwen",
      "match": {
        "any": [
          {
            "contains": "qwen"
          },
          {
            "contains": "qwq"
          }
        ]
      },
      "context_window": 128000,
      "max_output": 65536
    },
    {
      "id": "llama3",
      "match": {
        "any": [
          {
            "contains": "llama-3"
          },
          {
            "contains": "llama3"
          },
          {
            "contains": "llama_3"
          }
        ]
      },
      "context_window": 128000,
      "max_output": 65536
    },
    {
      "id": "llama_legacy",
      "match": {
        "contains": "llama"
      },
      "context_window": 4096,
      "max_output": 65536
    },
    {
      "id": "mistral",
      "match": {
        "any": [
          {
            "contains": "mixtral"
          },
          {
            "contains": "mistral"
          }
        ]
      },
      "context_window": 32000,
      "max_output": 65536
    },
    {
      "id": "gemma",
      "match": {
        "contains": "gemma"
      },
      "context_window": 8192,
      "max_output": 65536
    },
    {
      "id": "phi",
      "match": {
        "any": [
          {
            "contains": "phi-3"
          },
          {
            "contains": "phi3"
          },
          {
            "contains": "phi-4"
          },
          {
            "contains": "phi4"
          }
        ]
      },
      "context_window": 128000,
      "max_output": 65536
    },
    {
      "id": "gpt4",
      "match": {
        "any": [
          {
            "contains": "gpt-4"
          },
          {
            "contains": "gpt4"
          }
        ]
      },
      "context_window": 128000,
      "max_output": 65536
    },
    {
      "id": "yi",
      "match": {
        "any": [
          {
            "contains": "/yi-"
          },
          {
            "starts_with": "yi-"
          }
        ]
      },
      "context_window": 200000,
      "max_output": 65536
    }
  ]
} as const;

/** Catalog default context window (unknown / unmatched models). */
export const DEFAULT_CONTEXT_WINDOW_TOKENS: number = 128000;
/** Catalog default max output (unknown / unmatched models). */
export const DEFAULT_MAX_OUTPUT_TOKENS: number = 65536;

/** deepseek_v4 family — from model-catalog.json */
export const DEEPSEEK_V4_CONTEXT_WINDOW_TOKENS: number = 1000000;
export const DEEPSEEK_V4_MAX_OUTPUT_TOKENS: number = 393216;

/** kimi_k3 family — from model-catalog.json */
export const KIMI_K3_CONTEXT_TOKENS: number = 1000000;
export const KIMI_K3_MAX_OUTPUT_TOKENS: number = 1048576;
export const KIMI_K3_DEFAULT_MAX_TOKENS: number = 131072;

/** agnes_chat family — from model-catalog.json */
export const AGNES_CHAT_CONTEXT_TOKENS: number = 256000;
export const AGNES_CHAT_MAX_OUTPUT_TOKENS: number = 65536;

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
 * Map UI/config effort through the family's `effort_map` (mirrors Rust
 * `map_model_reasoning_effort`). Returns null when the family has no map.
 */
export function mapReasoningEffort(model: string, effort: string): string | null {
  const caps = resolveModelCaps(model);
  if (!caps.effortMap) return null;
  const key = effort.trim().toLowerCase() || "default";
  return caps.effortMap[key] ?? caps.effortMap["default"] ?? "max";
}

/** Hide Settings `off` when thinking cannot be disabled (always_thinking / effort_map). */
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
