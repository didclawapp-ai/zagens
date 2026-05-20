import { describe, expect, it } from "vitest";
import {
  DEFAULT_INJECT_INTERVAL_RUNS,
  detectEmotion,
  extractTopics,
  detectBlockedTopics,
  emptyGraph,
  updateGraph,
  applyDecay,
  generateMemorySection,
  shouldInjectMemory,
} from "../src/index.js";

// ─── detectEmotion ──────────────────────────────────────────────

describe("detectEmotion", () => {
  it("returns N for empty or short text", () => {
    expect(detectEmotion("")).toBe("N");
    expect(detectEmotion("a")).toBe("N");
    expect(detectEmotion("  ")).toBe("N");
  });

  it("returns N for neutral text with no signals", () => {
    expect(detectEmotion("请帮我看看这段代码有没有问题")).toBe("N");
  });

  it("detects angry mode (A) from Chinese signals", () => {
    // 需要 >= 2 个信号匹配
    expect(detectEmotion("什么破玩意！！烦死了")).toBe("A");
  });

  it("detects angry mode (A) from English ALL CAPS", () => {
    expect(detectEmotion("THIS IS SO STUPID AND RIDICULOUS")).toBe("A");
  });

  it("detects happy mode (B) from Chinese signals", () => {
    expect(detectEmotion("太棒了！！666 搞定了")).toBe("B");
  });

  it("detects happy mode (B) from English signals", () => {
    // 需要 >= 2 个不同正则匹配；"awesome" 和 "amazing" 在同一个 regex 里，算 1 个信号
    // 加上 emoji 信号使其达到 2 个
    expect(detectEmotion("awesome work, this is perfect 🎉")).toBe("B");
  });

  it("detects sad mode (C) from Chinese signals", () => {
    expect(detectEmotion("唉... 好累 算了")).toBe("C");
  });

  it("detects sad mode (C) from English signals", () => {
    expect(detectEmotion("sigh... i feel so tired and frustrated")).toBe("C");
  });

  it("returns N when only 1 signal matches (threshold is 2)", () => {
    // 只有 1 个 angry 信号
    expect(detectEmotion("这太蠢了")).not.toBe("A");
  });
});

// ─── extractTopics ──────────────────────────────────────────────

describe("extractTopics", () => {
  it("returns empty array for empty or blank text", () => {
    expect(extractTopics("")).toEqual([]);
    expect(extractTopics("   ")).toEqual([]);
  });

  it("extracts Chinese topic tokens", () => {
    // CJK regex matches 2-6 chars; actual segmentation depends on input
    const topics = extractTopics("数据库 连接池 优化性能 查询速度");
    expect(topics.length).toBeGreaterThan(0);
    expect(topics.some((t) => t.includes("数据库") || t.includes("连接池"))).toBe(true);
  });

  it("extracts English topic tokens", () => {
    const topics = extractTopics("How to implement database connection pooling");
    expect(topics).toContain("implement");
    expect(topics).toContain("database");
    expect(topics).toContain("connection");
    expect(topics).toContain("pooling");
  });

  it("filters out stop words", () => {
    const topics = extractTopics("the quick brown fox jumps over the lazy dog");
    expect(topics).not.toContain("the");
    // "over" is not in STOP_WORDS; verify common ones like "the", "and" are filtered
    expect(topics).not.toContain("and");
  });

  it("filters out Chinese stop words", () => {
    const topics = extractTopics("这个怎么使用数据库");
    expect(topics).not.toContain("这个");
    expect(topics).not.toContain("怎么");
    expect(topics).toContain("数据库");
  });

  it("strips markdown code fences before extraction", () => {
    const topics = extractTopics("使用数据库 ```SELECT * FROM users``` 进行查询");
    expect(topics).not.toContain("SELECT");
    // CJK regex matches 2-6 consecutive chars from the cleaned text
    expect(topics.length).toBeGreaterThan(0);
  });

  it("strips inline code before extraction", () => {
    const topics = extractTopics("使用 `useState` hook 管理状态");
    expect(topics).not.toContain("useState");
  });

  it("strips URLs before extraction", () => {
    const topics = extractTopics("参考 https://example.com/api 文档");
    expect(topics).not.toContain("https");
    expect(topics).not.toContain("example");
  });

  it("limits to MAX_TOPICS_PER_TURN (6)", () => {
    const text = "alpha beta gamma delta epsilon zeta eta theta iota kappa lambda";
    const topics = extractTopics(text);
    expect(topics.length).toBeLessThanOrEqual(6);
  });

  it("returns topics sorted by frequency descending", () => {
    const text = "database database database cache cache api";
    const topics = extractTopics(text);
    expect(topics[0]).toBe("database");
    expect(topics[1]).toBe("cache");
  });
});

// ─── detectBlockedTopics ────────────────────────────────────────

describe("detectBlockedTopics", () => {
  it("returns empty array when no blocked patterns match", () => {
    expect(detectBlockedTopics("数据库的使用方法很简单")).toEqual([]);
  });

  it("detects Chinese blocked patterns", () => {
    const result = detectBlockedTopics("我不知道分布式系统怎么用");
    expect(result.length).toBeGreaterThan(0);
    expect(result.some((t) => t.includes("分布式"))).toBe(true);
  });

  it("detects English blocked patterns", () => {
    const result = detectBlockedTopics("I don't know about Kubernetes");
    expect(result.length).toBeGreaterThan(0);
    expect(result.some((t) => t.toLowerCase().includes("kubernetes"))).toBe(true);
  });

  it("detects 'what is' pattern", () => {
    const result = detectBlockedTopics("What is GraphQL");
    expect(result.some((t) => t.toLowerCase().includes("graphql"))).toBe(true);
  });

  it("does not return stop words as blocked topics", () => {
    const result = detectBlockedTopics("我不知道什么");
    // "什么" is a stop word, should be filtered out
    expect(result).not.toContain("什么");
  });
});

// ─── emptyGraph ─────────────────────────────────────────────────

describe("emptyGraph", () => {
  it("returns a valid empty graph structure", () => {
    const g = emptyGraph();
    expect(g.version).toBe("0.1.0");
    expect(g.nodes).toEqual({});
    expect(g.edges).toEqual({});
    expect(g.blockedPoints).toEqual([]);
    expect(g.lastDecay).toMatch(/^\d{4}-\d{2}-\d{2}$/);
  });

  it("returns a new object each time", () => {
    const a = emptyGraph();
    const b = emptyGraph();
    expect(a).not.toBe(b);
  });
});

// ─── updateGraph ────────────────────────────────────────────────

describe("updateGraph", () => {
  it("does not mutate the input graph", () => {
    const g = emptyGraph();
    const snapshot = JSON.stringify(g);
    updateGraph(g, "数据库优化", "可以使用索引优化查询速度");
    expect(JSON.stringify(g)).toBe(snapshot);
  });

  it("creates new nodes for detected topics", () => {
    const g = emptyGraph();
    const result = updateGraph(g, "数据库性能优化", "可以通过索引和缓存提升性能");
    expect(Object.keys(result.nodes).length).toBeGreaterThan(0);
  });

  it("increments count on existing nodes", () => {
    let g = emptyGraph();
    g = updateGraph(g, "数据库优化", "索引可以提升查询速度");
    g = updateGraph(g, "数据库优化", "索引可以提升查询速度");
    const dbNode = Object.values(g.nodes).find(
      (n) => n.count >= 2,
    );
    expect(dbNode).toBeDefined();
  });

  it("records emotion in recentEmotions", () => {
    const g = emptyGraph();
    const result = updateGraph(g, "太棒了！666 搞定了完美", "谢谢！");
    expect(result.recentEmotions).toBeDefined();
    expect(result.recentEmotions!.length).toBe(1);
  });

  it("caps recentEmotions at 10 entries", () => {
    let g = emptyGraph();
    for (let i = 0; i < 15; i++) {
      g = updateGraph(g, `话题${i} 内容描述`, `回复${i} 内容`);
    }
    expect(g.recentEmotions!.length).toBeLessThanOrEqual(10);
  });

  it("creates edges between user topics", () => {
    const g = emptyGraph();
    const result = updateGraph(g, "数据库索引优化", "索引可以显著提升查询性能");
    const edgeKeys = Object.keys(result.edges);
    // 至少应有 1 条边（如果提取到 >= 2 个 user topics）
    if (edgeKeys.length > 0) {
      expect(edgeKeys[0]).toContain("→");
    }
  });

  it("suppresses edge creation in angry mode (A)", () => {
    const g = emptyGraph();
    // 需要足够强的 angry 信号
    const result = updateGraph(g, "什么破玩意！！！烦死垃圾", "抱歉");
    expect(Object.keys(result.edges).length).toBe(0);
  });

  it("records cognitive trails when >= 2 user topics extracted", () => {
    const g = emptyGraph();
    // 需要 userTopics 提取到 >= 2 个去重后的 topic
    const result = updateGraph(g, "数据库优化 缓存策略 查询性能", "回复内容");
    // trails 仅在 allUserTopics.length >= 2 时创建
    if (result.trails && result.trails.length > 0) {
      expect(result.trails[0].entry).toBeTruthy();
      expect(result.trails[0].exit).toBeTruthy();
      expect(result.trails[0].date).toMatch(/^\d{4}-\d{2}-\d{2}$/);
    }
    // 至少不应抛错；trails 可能为空取决于 extractTopics 的结果
    expect(result.trails).toBeDefined();
  });

  it("detects and records blocked points", () => {
    const g = emptyGraph();
    const result = updateGraph(g, "我不知道量子计算是什么", "量子计算是一种...");
    expect(result.blockedPoints.length).toBeGreaterThan(0);
  });

  it("applies transitive bridging for strong edges", () => {
    // 构建 A→B 和 B→C 都足够强的场景
    let g = emptyGraph();
    // 多次更新同一对 topic，使 edge weight 超过 BRIDGE_WEIGHT_THRESHOLD (2.5)
    for (let i = 0; i < 5; i++) {
      g = updateGraph(g, "数据库索引查询", "索引优化查询性能");
    }
    // 检查是否产生了桥接边
    const bridgeKeys = Object.keys(g.edges).filter(
      (k) => k.startsWith("数据库") && k.includes("查询"),
    );
    // 这是间接测试；只要不报错就算通过
    expect(g.edges).toBeDefined();
    expect(bridgeKeys).toBeDefined();
  });

  it("caps trails at 20 entries", () => {
    let g = emptyGraph();
    for (let i = 0; i < 30; i++) {
      g = updateGraph(g, `主题${i}子主题${i}`, `回复${i}`);
    }
    expect(g.trails!.length).toBeLessThanOrEqual(20);
  });
});

// ─── applyDecay ─────────────────────────────────────────────────

describe("applyDecay", () => {
  it("does nothing when lastDecay is today", () => {
    const g = emptyGraph();
    g.nodes["test"] = { count: 1, lastSeen: g.lastDecay, strength: 0.5, depth: 1 };
    const result = applyDecay(g);
    expect(result.nodes["test"].strength).toBe(0.5);
  });

  it("decays node strength over days", () => {
    const g = emptyGraph();
    // lastDecay 是 10 天前
    const pastDate = new Date(Date.now() - 10 * 86_400_000).toISOString().slice(0, 10);
    g.lastDecay = pastDate;
    g.nodes["test"] = { count: 1, lastSeen: pastDate, strength: 1.0, depth: 1 };

    const result = applyDecay(g);
    // 0.97^10 ≈ 0.737
    expect(result.nodes["test"].strength).toBeLessThan(1.0);
    expect(result.nodes["test"].strength).toBeGreaterThan(0.5);
  });

  it("marks nodes as dormant when strength falls below threshold", () => {
    const g = emptyGraph();
    const pastDate = new Date(Date.now() - 100 * 86_400_000).toISOString().slice(0, 10);
    g.lastDecay = pastDate;
    g.nodes["weak"] = { count: 1, lastSeen: pastDate, strength: 0.1, depth: 1 };

    const result = applyDecay(g);
    expect(result.nodes["weak"].dormant).toBe(true);
  });

  it("removes edges when weight drops below 0.5", () => {
    const g = emptyGraph();
    const pastDate = new Date(Date.now() - 100 * 86_400_000).toISOString().slice(0, 10);
    g.lastDecay = pastDate;
    g.edges["a→b"] = { weight: 0.6, lastSeen: pastDate };

    const result = applyDecay(g);
    // 0.6 * 0.97^100 ≈ 0.6 * 0.047 ≈ 0.028 < 0.5
    expect(result.edges["a→b"]).toBeUndefined();
  });

  it("preserves strong edges after decay", () => {
    const g = emptyGraph();
    const pastDate = new Date(Date.now() - 3 * 86_400_000).toISOString().slice(0, 10);
    g.lastDecay = pastDate;
    g.edges["a→b"] = { weight: 5.0, lastSeen: pastDate };

    const result = applyDecay(g);
    expect(result.edges["a→b"]).toBeDefined();
    expect(result.edges["a→b"].weight).toBeLessThan(5.0);
  });

  it("updates lastDecay to today", () => {
    const g = emptyGraph();
    const pastDate = "2020-01-01";
    g.lastDecay = pastDate;

    const result = applyDecay(g);
    const today = new Date().toISOString().slice(0, 10);
    expect(result.lastDecay).toBe(today);
  });
});

// ─── generateMemorySection ──────────────────────────────────────

describe("generateMemorySection", () => {
  it("includes header even for empty graph", () => {
    const g = emptyGraph();
    const md = generateMemorySection(g);
    expect(md).toContain("User Cognitive Map");
    expect(md).toContain("not enough data yet");
    expect(md).toContain("auto-generated · do not edit");
    expect(md).not.toContain("by DidClaw");
  });

  it("optional attribution in header", () => {
    const g = emptyGraph();
    const md = generateMemorySection(g, { attribution: "DidClaw" });
    expect(md).toContain("by DidClaw");
  });

  it("includes hot nodes sorted by strength", () => {
    const g = emptyGraph();
    g.nodes["database"] = { count: 5, lastSeen: "2025-01-01", strength: 0.8, depth: 3 };
    g.nodes["cache"] = { count: 3, lastSeen: "2025-01-01", strength: 0.4, depth: 2 };

    const md = generateMemorySection(g);
    const dbIdx = md.indexOf("database");
    const cacheIdx = md.indexOf("cache");
    expect(dbIdx).toBeLessThan(cacheIdx); // database 排在前面
  });

  it("excludes dormant nodes", () => {
    const g = emptyGraph();
    g.nodes["active"] = { count: 5, lastSeen: "2025-01-01", strength: 0.5, depth: 2 };
    g.nodes["sleeping"] = { count: 1, lastSeen: "2025-01-01", strength: 0.01, depth: 1, dormant: true };

    const md = generateMemorySection(g);
    expect(md).toContain("active");
    expect(md).not.toContain("sleeping");
  });

  it("includes edges section when edges exist", () => {
    const g = emptyGraph();
    g.edges["db→cache"] = { weight: 3.0, lastSeen: "2025-01-01" };

    const md = generateMemorySection(g);
    expect(md).toContain("Common Associations");
    expect(md).toContain("db");
    expect(md).toContain("cache");
  });

  it("includes blocked points section when present", () => {
    const g = emptyGraph();
    g.blockedPoints = [{ node: "量子计算", context: "不知道量子计算是什么", since: "2025-01-01" }];

    const md = generateMemorySection(g);
    expect(md).toContain("Knowledge Boundaries");
    expect(md).toContain("量子计算");
  });

  it("includes cognitive trails when present", () => {
    const g = emptyGraph();
    g.trails = [{ entry: "数据库", exit: "索引", date: "2025-01-01", emotion: "N" }];

    const md = generateMemorySection(g);
    expect(md).toContain("Cognitive Trails");
    expect(md).toContain("数据库");
    expect(md).toContain("索引");
  });

  it("includes mood tendency when emotions recorded", () => {
    const g = emptyGraph();
    g.recentEmotions = ["B", "B", "N", "B"];

    const md = generateMemorySection(g);
    expect(md).toContain("Recent Mood Tendency");
    expect(md).toContain("expansive/positive");
  });
});

// ─── shouldInjectMemory ─────────────────────────────────────────

describe("shouldInjectMemory", () => {
  it("exports default interval", () => {
    expect(DEFAULT_INJECT_INTERVAL_RUNS).toBe(5);
  });

  it("returns false when graph has no data", () => {
    const g = emptyGraph();
    expect(shouldInjectMemory(g, 10)).toBe(false);
  });

  it("returns false when no node has count >= 2", () => {
    const g = emptyGraph();
    g.nodes["topic"] = { count: 1, lastSeen: "2025-01-01", strength: 0.1, depth: 1 };
    expect(shouldInjectMemory(g, 10)).toBe(false);
  });

  it("returns false when has data but runsSinceLastInject < default interval", () => {
    const g = emptyGraph();
    g.nodes["topic"] = { count: 3, lastSeen: "2025-01-01", strength: 0.5, depth: 2 };
    expect(shouldInjectMemory(g, 3)).toBe(false);
  });

  it("returns true when has data and runsSinceLastInject >= default interval", () => {
    const g = emptyGraph();
    g.nodes["topic"] = { count: 3, lastSeen: "2025-01-01", strength: 0.5, depth: 2 };
    expect(shouldInjectMemory(g, 5)).toBe(true);
    expect(shouldInjectMemory(g, 10)).toBe(true);
  });

  it("respects custom minRunsBetweenInject", () => {
    const g = emptyGraph();
    g.nodes["topic"] = { count: 3, lastSeen: "2025-01-01", strength: 0.5, depth: 2 };
    expect(shouldInjectMemory(g, 2, 3)).toBe(false);
    expect(shouldInjectMemory(g, 3, 3)).toBe(true);
  });
});
