/**
 * Cognitive memory graph (“pheromone” style)
 *
 * Maintains a dynamic map of conversation topics: frequency, associations, decay,
 * and optional mood/trail signals. Hosts typically serialize the graph to JSON and
 * inject `generateMemorySection()` output into AGENTS.md (or similar).
 */

export interface PheromoneNode {
  count: number;
  lastSeen: string; // ISO date string YYYY-MM-DD
  strength: number; // 0–1; 1 = very familiar
  depth: number; // estimated cognitive depth 1–5
  blocked?: boolean; // user hit a knowledge wall here
  dormant?: boolean; // strength fell below threshold
}

export interface PheromoneEdge {
  weight: number;
  lastSeen: string;
}

export interface BlockedPoint {
  node: string;
  context: string;
  since: string;
}

export interface CognitiveTrail {
  entry: string; // first topic of the run
  exit: string; // last topic of the run
  date: string; // ISO date
  emotion: EmotionMode;
}

export interface PheromoneGraph {
  version: string;
  lastDecay: string; // ISO date YYYY-MM-DD
  nodes: Record<string, PheromoneNode>;
  edges: Record<string, PheromoneEdge>; // "A→B"
  blockedPoints: BlockedPoint[];
  recentEmotions?: EmotionMode[]; // last N emotion readings
  trails?: CognitiveTrail[]; // entry→exit records per run
}

export const GRAPH_SCHEMA_VERSION = "0.1.0";

const DECAY_RATE = 0.97;
const DORMANT_THRESHOLD = 0.05;
const STRENGTH_GAIN = 0.06;
const MAX_TOPICS_PER_TURN = 6;
const MAX_HOT_NODES = 12;
const MAX_HOT_EDGES = 6;
/** Default: inject after this many completed runs (host may pass a different threshold). */
export const DEFAULT_INJECT_INTERVAL_RUNS = 5;
const BRIDGE_WEIGHT_THRESHOLD = 2.5; // min edge weight for transitive bridging
const BRIDGE_INITIAL_WEIGHT = 0.4; // new bridge edge starts weak

// ── Emotion detection ────────────────────────────────────────────────────────

/** A = angry/focused, B = happy/expansive, C = sad/ruminant, N = neutral */
export type EmotionMode = "A" | "B" | "C" | "N";

/** Regex-based emotion signals. Designed for mixed Chinese/English chat text. */
const EMOTION_SIGNALS: Record<Exclude<EmotionMode, "N">, RegExp[]> = {
  // Angry: isolating mode — narrow focus, high intensity
  A: [
    /！{2,}/, // multiple !!
    /[草操艹尼玛妈的滚]/, // common angry chars
    /(?:烦死|气死|蠢|傻|垃圾|什么破|搞什么|凭什么|为什么非要)/,
    /(?:fuck|damn|shit|wtf|stupid|idiot|ridiculous|annoying|hate)/i,
    /[A-Z]{4,}/, // ALL CAPS WORD
  ],
  // Happy: expansive mode — more associations, lower threshold
  B: [
    /哈{2,}/, // 哈哈哈
    /(?:666|牛[啊哦!！]?|太好了|太棒了|完美|太爽|嘿嘿|耶|撒花)/,
    /(?:awesome|great|excellent|amazing|wonderful|perfect|haha|lol|yay|nice)/i,
    /[😊🎉✓👍🎊]/u,
    /(?:可以了|正好|搞定了|成了|终于)/,
  ],
  // Sad/郁闷: ruminant mode — loops on existing nodes, few new ones
  C: [
    /(?:唉|哎|呜|唔)[^哈]*/,
    /(?:算了|没意思|好累|烦躁|郁闷|难过|不想|放弃|搞不定|不知道咋办)/,
    /\.{3,}|…{2,}/, // many ellipsis
    /(?:sigh|tired|frustrated|depressed|sad|whatever|meh|hopeless)/i,
    /(?:怎么办|没有用|没用|失败了|又失败)/,
  ],
};

/**
 * Detect the dominant emotion mode from a user message.
 * Returns the first mode that accumulates ≥2 signal matches, or "N".
 */
export function detectEmotion(text: string): EmotionMode {
  if (!text || text.trim().length < 2) return "N";
  const scores: Record<Exclude<EmotionMode, "N">, number> = { A: 0, B: 0, C: 0 };
  for (const mode of ["A", "B", "C"] as const) {
    for (const re of EMOTION_SIGNALS[mode]) {
      if (re.test(text)) scores[mode]++;
    }
  }
  // Need at least 2 matching signals to classify; otherwise neutral
  const winner = (["A", "B", "C"] as const).find((m) => scores[m] >= 2);
  return winner ?? "N";
}

// Common stop words to filter out of topic extraction
const STOP_WORDS = new Set([
  // English — common verbs, articles, generic nouns
  "the",
  "a",
  "an",
  "is",
  "are",
  "was",
  "were",
  "be",
  "been",
  "being",
  "have",
  "has",
  "had",
  "do",
  "does",
  "did",
  "will",
  "would",
  "shall",
  "should",
  "may",
  "might",
  "must",
  "can",
  "could",
  "i",
  "you",
  "he",
  "she",
  "it",
  "we",
  "they",
  "me",
  "him",
  "her",
  "us",
  "them",
  "my",
  "your",
  "his",
  "its",
  "our",
  "their",
  "this",
  "that",
  "these",
  "those",
  "what",
  "which",
  "who",
  "whom",
  "how",
  "when",
  "where",
  "why",
  "all",
  "any",
  "both",
  "each",
  "more",
  "most",
  "other",
  "some",
  "such",
  "no",
  "nor",
  "not",
  "only",
  "own",
  "same",
  "so",
  "than",
  "too",
  "very",
  "just",
  "but",
  "and",
  "or",
  "as",
  "at",
  "by",
  "for",
  "in",
  "of",
  "on",
  "to",
  "up",
  "with",
  "from",
  "into",
  "about",
  "like",
  "also",
  "then",
  "than",
  "if",
  "but",
  "because",
  "while",
  "although",
  "though",
  "since",
  "until",
  "unless",
  "ok",
  "okay",
  "yes",
  "no",
  "hi",
  "hello",
  "thanks",
  "thank",
  "please",
  "sorry",
  "sure",
  // Generic English verbs and words that pollute topics
  "new",
  "get",
  "set",
  "use",
  "make",
  "take",
  "give",
  "show",
  "find",
  "know",
  "see",
  "say",
  "tell",
  "ask",
  "try",
  "run",
  "add",
  "put",
  "let",
  "got",
  "now",
  "one",
  "two",
  "way",
  "day",
  "time",
  "thing",
  "things",
  "good",
  "bad",
  "big",
  "small",
  "need",
  "want",
  "look",
  "work",
  "help",
  "here",
  "there",
  "come",
  "back",
  "out",
  "has",
  "its",
  "via",
  "per",
  "etc",
  // Chinese — stop words + common fragments
  "的",
  "了",
  "在",
  "是",
  "我",
  "你",
  "他",
  "她",
  "它",
  "们",
  "这",
  "那",
  "有",
  "和",
  "就",
  "不",
  "也",
  "都",
  "而",
  "及",
  "与",
  "着",
  "或",
  "于",
  "一个",
  "可以",
  "什么",
  "怎么",
  "如何",
  "可能",
  "应该",
  "需要",
  "我们",
  "你们",
  "他们",
  "因为",
  "所以",
  "但是",
  "然后",
  "如果",
  "虽然",
  "对于",
  "关于",
  "通过",
  "进行",
  "使用",
  "没有",
  "一些",
  "这些",
  "那些",
  "这个",
  "那个",
  "这里",
  "那里",
  "现在",
  "时候",
  "好的",
  "谢谢",
  "请问",
  "您好",
  "对",
  "嗯",
  "吗",
  "呢",
  "啊",
  "哦",
  "哈",
  "嗯嗯",
  "一下",
  "一点",
  "已经",
  "还是",
  "只是",
  "其实",
  "不是",
  "还有",
  "就是",
  "来说",
  "来看",
  // Chinese question/quantity fragments
  "哪些",
  "哪里",
  "哪个",
  "哪种",
  "多少",
  "几个",
  "几种",
  "怎样",
  "为何",
  "为什么",
  "什么样",
  "有哪些",
  "有什么",
  "是什么",
  "怎么办",
  "如何做",
  "可以吗",
  "好吗",
  "行吗",
  "对吗",
  // Chinese generic action words
  "帮我",
  "帮你",
  "告诉",
  "知道",
  "觉得",
  "认为",
  "感觉",
  "看看",
  "说说",
  "想想",
  "试试",
  "做到",
  "做好",
  "完成",
  "实现",
  "开始",
  "继续",
  "停止",
  "修改",
  "更新",
  "添加",
  "删除",
  // Single common chars that slip through
  "新",
  "旧",
  "大",
  "小",
  "多",
  "少",
  "快",
  "慢",
  "好",
  "差",
  "高",
  "低",
  "上",
  "下",
  "左",
  "右",
]);

function todayStr(): string {
  return new Date().toISOString().slice(0, 10);
}

function daysBetween(isoA: string, isoB: string): number {
  const a = new Date(isoA).getTime();
  const b = new Date(isoB).getTime();
  return Math.max(0, Math.round(Math.abs(b - a) / 86_400_000));
}

/**
 * Extract meaningful topic tokens from a piece of text.
 * Uses simple tokenisation — no external NLP needed for MVP.
 */
export function extractTopics(text: string): string[] {
  if (!text || text.trim().length === 0) return [];

  // Normalise: remove markdown syntax, code fences, URLs
  const cleaned = text
    .replace(/```[\s\S]*?```/g, " ")
    .replace(/`[^`]+`/g, " ")
    .replace(/https?:\/\/\S+/g, " ")
    .replace(/[#*_~>|[\]()]/g, " ")
    .replace(/\s+/g, " ")
    .trim();

  const freq: Record<string, number> = {};

  // Chinese segments: consecutive CJK characters (1–6 chars is a good phrase range)
  const cnMatches = cleaned.match(/[\u4e00-\u9fff\u3400-\u4dbf]{2,6}/g) ?? [];
  for (const w of cnMatches) {
    if (!STOP_WORDS.has(w)) freq[w] = (freq[w] ?? 0) + 1;
  }

  // English words: alpha sequences ≥ 3 chars
  const enMatches = cleaned.match(/[a-zA-Z]{3,}/g) ?? [];
  for (const w of enMatches) {
    const lw = w.toLowerCase();
    if (!STOP_WORDS.has(lw) && lw.length >= 3) {
      freq[lw] = (freq[lw] ?? 0) + 1;
    }
  }

  // Sort by frequency descending, return top N
  return Object.entries(freq)
    .sort((a, b) => b[1] - a[1])
    .slice(0, MAX_TOPICS_PER_TURN)
    .map(([w]) => w);
}

/** Detect potential blocked points: user expressing confusion or lack of knowledge */
export function detectBlockedTopics(userText: string): string[] {
  const BLOCK_PATTERNS = [
    /不(?:知道|懂|了解|明白|清楚)(.{2,10})/g,
    /(?:不太|完全不|没有)(?:了解|明白|理解)(.{2,10})/g,
    /(?:i don'?t know|i'?m not sure about|don'?t understand)\s+(.{3,30})/gi,
    /(?:what is|what are|explain)\s+(.{3,30})/gi,
  ];
  const blocked: string[] = [];
  for (const re of BLOCK_PATTERNS) {
    let m: RegExpExecArray | null;
    while ((m = re.exec(userText)) !== null) {
      const topic = m[1].trim().replace(/[？?。，,！!]$/, "");
      if (topic && !STOP_WORDS.has(topic.toLowerCase())) {
        blocked.push(topic);
      }
    }
  }
  return blocked;
}

export function emptyGraph(): PheromoneGraph {
  return {
    version: GRAPH_SCHEMA_VERSION,
    lastDecay: todayStr(),
    nodes: {},
    edges: {},
    blockedPoints: [],
  };
}

/**
 * Update the graph after a completed conversation turn.
 * Emotion mode shapes how nodes and edges are updated:
 *   A (angry)  — isolating: focus node gets ×3 boost, edge creation suppressed
 *   B (happy)  — expansive: all gains boosted, weak edges also created
 *   C (sad)    — ruminant:  existing edges reinforced, new node creation suppressed
 *   N (neutral)— default linear update
 */
export function updateGraph(
  graph: PheromoneGraph,
  userText: string,
  assistantText: string,
): PheromoneGraph {
  const today = todayStr();
  const g: PheromoneGraph = JSON.parse(JSON.stringify(graph)) as PheromoneGraph;
  const emotion = detectEmotion(userText);

  const userTopics = extractTopics(userText);
  const assistantTopics = extractTopics(assistantText);
  const allTopics = [...new Set([...userTopics, ...assistantTopics])].slice(0, MAX_TOPICS_PER_TURN);

  // Emotion-driven multipliers
  // A: first user topic gets ×3, rest suppressed; B: all ×1.5; C: existing only, ×1.0
  const nodeGainMultiplier = (topic: string, idx: number): number => {
    if (emotion === "A") return idx === 0 ? 3.0 : 0.2; // isolate on first topic
    if (emotion === "B") return 1.5; // expansive
    if (emotion === "C") return g.nodes[topic] ? 1.0 : 0; // ruminant: no new nodes
    return 1.0;
  };

  // Update nodes
  allTopics.forEach((topic, idx) => {
    const mult = nodeGainMultiplier(topic, idx);
    if (mult === 0) return; // C mode suppresses new node creation

    const existing = g.nodes[topic];
    const gain = STRENGTH_GAIN * mult;
    if (existing) {
      existing.count += 1;
      existing.strength = Math.min(1.0, existing.strength + gain);
      existing.lastSeen = today;
      existing.dormant = false;
      if (userTopics.includes(topic) && assistantTopics.includes(topic)) {
        existing.depth = Math.min(5, existing.depth + 0.2);
      }
    } else {
      g.nodes[topic] = {
        count: 1,
        lastSeen: today,
        strength: gain,
        depth: userTopics.includes(topic) ? 2 : 1,
      };
    }
  });

  // Update edges
  // A: suppress new edges (focused, not associating)
  // B: create edges even for weak pairs
  // C: reinforce existing edges ×1.5, no new ones
  for (let i = 0; i < userTopics.length; i++) {
    for (let j = i + 1; j < userTopics.length; j++) {
      const key = `${userTopics[i]}→${userTopics[j]}`;
      const e = g.edges[key];
      if (emotion === "A") continue; // no new associations when angry
      if (emotion === "C") {
        if (e) {
          e.weight += 1.5;
          e.lastSeen = today;
        } // reinforce only existing
        continue;
      }
      // N and B: create or update
      if (e) {
        e.weight += emotion === "B" ? 1.5 : 1;
        e.lastSeen = today;
      } else {
        g.edges[key] = { weight: 1, lastSeen: today };
      }
    }
  }

  // Record emotion (keep last 10)
  if (!g.recentEmotions) g.recentEmotions = [];
  g.recentEmotions.push(emotion);
  if (g.recentEmotions.length > 10) g.recentEmotions.shift();

  // Record cognitive trail: entry = first user topic, exit = last user topic
  const allUserTopics = [...new Set(userTopics)];
  if (allUserTopics.length >= 2) {
    if (!g.trails) g.trails = [];
    g.trails.push({
      entry: allUserTopics[0],
      exit: allUserTopics[allUserTopics.length - 1],
      date: today,
      emotion,
    });
    if (g.trails.length > 20) g.trails.shift(); // keep last 20 trails
  }

  // Transitive bridging: build A→C from A→B + B→C
  applyTransitiveBridging(g);

  // Detect blocked points from user text
  const blocked = detectBlockedTopics(userText);
  for (const bTopic of blocked) {
    if (!g.blockedPoints.find((b) => b.node === bTopic)) {
      g.blockedPoints.push({
        node: bTopic,
        context: userText.slice(0, 80).trim(),
        since: today,
      });
      if (g.nodes[bTopic]) g.nodes[bTopic].blocked = true;
    }
  }

  return g;
}

/**
 * Transitive bridging: if A→B and B→C are both strong, create a weak A→C.
 * Simulates the "chain association" cognitive pattern.
 */
function applyTransitiveBridging(g: PheromoneGraph): void {
  const today = todayStr();
  // Collect strong directed edges
  const strong: [string, string][] = [];
  for (const [key, e] of Object.entries(g.edges)) {
    if (e.weight >= BRIDGE_WEIGHT_THRESHOLD) {
      const [a, b] = key.split("→");
      if (a && b) strong.push([a, b]);
    }
  }
  // Build adjacency map for fast B lookup
  const adj: Record<string, string[]> = {};
  for (const [a, b] of strong) {
    if (!adj[a]) adj[a] = [];
    adj[a].push(b);
  }
  // A→B and B→C → create A→C if not already exists
  for (const [a, b] of strong) {
    for (const c of adj[b] ?? []) {
      if (c === a) continue;
      const bridgeKey = `${a}→${c}`;
      if (!g.edges[bridgeKey]) {
        g.edges[bridgeKey] = { weight: BRIDGE_INITIAL_WEIGHT, lastSeen: today };
      }
    }
  }
}

/**
 * Apply time-based decay to all nodes and edges.
 * Should be called at most once per day.
 */
export function applyDecay(graph: PheromoneGraph): PheromoneGraph {
  const today = todayStr();
  const g: PheromoneGraph = JSON.parse(JSON.stringify(graph)) as PheromoneGraph;
  const daysSinceLast = daysBetween(g.lastDecay, today);

  if (daysSinceLast === 0) return g;

  const factor = Math.pow(DECAY_RATE, daysSinceLast);

  for (const key in g.nodes) {
    const n = g.nodes[key];
    n.strength = n.strength * factor;
    if (n.strength < DORMANT_THRESHOLD) {
      n.dormant = true;
    }
  }

  for (const key in g.edges) {
    const e = g.edges[key];
    e.weight = e.weight * factor;
    if (e.weight < 0.5) {
      delete g.edges[key];
    }
  }

  g.lastDecay = today;
  return g;
}

export interface GenerateMemorySectionOptions {
  /** If set, header becomes “… by {attribution} · do not edit …” */
  attribution?: string;
}

/**
 * Generate the markdown section to inject into AGENTS.md (or similar).
 */
export function generateMemorySection(
  graph: PheromoneGraph,
  options?: GenerateMemorySectionOptions,
): string {
  const hotNodes = Object.entries(graph.nodes)
    .filter(([, n]) => !n.dormant && n.strength >= 0.1)
    .sort((a, b) => b[1].strength - a[1].strength)
    .slice(0, MAX_HOT_NODES);

  const hotEdges = Object.entries(graph.edges)
    .sort((a, b) => b[1].weight - a[1].weight)
    .slice(0, MAX_HOT_EDGES);

  const activeBlocked = graph.blockedPoints.slice(-5);

  const headerTail = options?.attribution
    ? ` (auto-generated by ${options.attribution} · do not edit this section)`
    : " (auto-generated · do not edit this section)";

  const lines: string[] = [`## User Cognitive Map${headerTail}`, "", "### Frequent Topics"];

  if (hotNodes.length === 0) {
    lines.push("- (not enough data yet)");
  } else {
    for (const [topic, n] of hotNodes) {
      const bar = "█".repeat(Math.round(n.strength * 5));
      lines.push(`- **${topic}** ${bar} (depth ${Math.round(n.depth)}, ${n.count} mentions)`);
    }
  }

  if (hotEdges.length > 0) {
    lines.push("", "### Common Associations");
    for (const [edge] of hotEdges) {
      lines.push(`- ${edge.replace("→", " → ")}`);
    }
  }

  if (activeBlocked.length > 0) {
    lines.push("", "### Knowledge Boundaries (user indicated uncertainty)");
    for (const b of activeBlocked) {
      lines.push(`- **${b.node}**: ${b.context.slice(0, 60)}…`);
    }
  }

  // Cognitive trails
  const trails = (graph.trails ?? []).slice(-8);
  if (trails.length > 0) {
    lines.push("", "### Cognitive Trails (entry → exit per run)");
    for (const tr of trails) {
      const moodIcon: Record<EmotionMode, string> = { A: "⚡", B: "✨", C: "🌧", N: "·" };
      lines.push(`- ${moodIcon[tr.emotion]} **${tr.entry}** → **${tr.exit}** _(${tr.date})_`);
    }
  }

  // Emotion summary
  const emotions = graph.recentEmotions ?? [];
  if (emotions.length > 0) {
    const counts = { A: 0, B: 0, C: 0, N: 0 };
    for (const e of emotions) counts[e]++;
    const dominant = (["A", "B", "C", "N"] as EmotionMode[]).sort(
      (a, b) => counts[b as keyof typeof counts] - counts[a as keyof typeof counts],
    )[0];
    const moodLabel: Record<EmotionMode, string> = {
      A: "focused/intense (A)",
      B: "expansive/positive (B)",
      C: "ruminant/low-energy (C)",
      N: "neutral (N)",
    };
    lines.push("", `### Recent Mood Tendency`, `- ${moodLabel[dominant]} across last ${emotions.length} turns`);
  }

  lines.push(
    "",
    `_Updated ${new Date().toISOString().slice(0, 10)} · ${hotNodes.length} active topics_`,
  );

  return lines.join("\n");
}

/** Returns true if the graph has accumulated enough runs to warrant an injection. */
export function shouldInjectMemory(
  graph: PheromoneGraph,
  runsSinceLastInject: number,
  minRunsBetweenInject: number = DEFAULT_INJECT_INTERVAL_RUNS,
): boolean {
  const hasData = Object.values(graph.nodes).some((n) => n.count >= 2);
  return hasData && runsSinceLastInject >= minRunsBetweenInject;
}
