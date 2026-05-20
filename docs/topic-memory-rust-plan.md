# topic-memory-graph → DS Pick Rust 重写方案（v2 优化版）

> 复审日期：2026-05-20 · 基于 repo 代码现状（`engine.rs:1934`、`Cargo.toml`、`config.rs` 双层结构）逐段修正

## 1. 概述

将 `topic-memory-graph`（TypeScript，零依赖，~450 行，源自 DidClaw / LCLAW monorepo）用 Rust 重写，作为 DS Pick 的新 crate `crates/topic-memory`，嵌入 agent 系统提示词注入管线，实现**对话话题图的增量维护、衰减、情绪感知与记忆段生成**。

源码参考：`docs/topic-memory-graph-main/src/`

### 核心价值

- **不调 LLM**：纯启发式（正则 + 计数），零 API 成本
- **流式更新**：每轮对话结束即可更新图，不需要批量/夜间任务
- **情绪调制**：A(愤怒/聚焦)、B(高兴/扩展)、C(沮丧/反刍)、N(中性) 四种模式改变节点创建和边关联策略
- **时间衰减**：每天 ×0.97，节点进入 dormant（结构保留），弱边删除
- **传递桥接**：强 A→B + B→C 自动生成弱 A→C

---

## 2. 架构位置

```
crates/
├── topic-memory/          ← 新增 crate
│   ├── Cargo.toml
│   └── src/
│       ├── lib.rs         # 公共 API 出口
│       ├── graph.rs       # 数据结构 + serde
│       ├── extract.rs     # 话题提取 + 情绪检测 + 盲区检测
│       ├── engine.rs      # update_graph / apply_decay / 传递桥接 / LRU 淘汰
│       ├── render.rs      # generate_memory_section → Markdown
│       ├── inject.rs      # inject_memory_section (HTML 注释替换)
│       └── stopwords.rs   # 中英文停用词表
├── config/src/lib.rs      # TopicMemoryToml（TOML schema）
├── tui/src/
│   ├── config.rs          # TopicMemoryConfig（解析 + 默认值 → EngineConfig 字段）
│   ├── core/engine.rs     # refresh_system_prompt 追加 topic_memory_block
│   ├── memory.rs          # 现有的 user_memory，与 topic_memory 并列
│   └── prompts.rs         # PromptSessionContext 新增 topic_memory_block 字段
└── desktop/src/           # Tauri 端：prompt 组装时消费相同 topic-memory crate
```

### 为什么是独立 crate

- `tui` crate 已经很大（~50 个源文件），独立 crate 保持边界清晰
- `topic-memory` 逻辑自包含，可单独测试（`cargo test -p deepseek-topic-memory`）
- TUI 端和桌面端（Tauri）复用同一份图逻辑；桌面端在 `crates/desktop/src/` 的 prompt 组装中直接调用 `generate_memory_section` / `update_graph`
- 依赖极度克制（仅 serde + regex + chrono + once_cell），不会引入重依赖

---

## 3. `user_memory` vs `topic_memory` 对比

两个 memory 概念互补，需在文档中明确区分：

| | `user_memory` | `topic_memory` |
|---|---|---|
| **写入者** | 用户手动或 `remember` tool | 引擎自动提取 |
| **存储位置** | `~/.deepseek/memory.md` | `~/.deepseek/topic-memory.json` |
| **内容** | 偏好、约定、声明 | 话题关联、情绪、认知轨迹 |
| **生命周期** | 用户显式管理 | 自动衰减到 dormant |
| **注入频率** | 每轮 | 每 N 轮（默认 5） |
| **system prompt 块** | `<user_memory>` | `<topic_memory>` |

---

## 4. 数据模型

### 4.1 图结构（对应 TS `PheromoneGraph`）

```rust
// crates/topic-memory/src/graph.rs

use std::collections::HashMap;
use serde::{Deserialize, Serialize};

pub const GRAPH_SCHEMA_VERSION: &str = "0.1.0";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PheromoneGraph {
    pub version: String,
    pub last_decay: String,           // ISO date YYYY-MM-DD
    pub nodes: HashMap<String, PheromoneNode>,
    pub edges: HashMap<String, PheromoneEdge>,  // key: "A→B"
    pub blocked_points: Vec<BlockedPoint>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub recent_emotions: Option<Vec<EmotionMode>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub trails: Option<Vec<CognitiveTrail>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PheromoneNode {
    pub count: u32,
    pub last_seen: String,
    pub strength: f64,        // 0.0–1.0
    pub depth: f64,           // 1.0–5.0
    #[serde(skip_serializing_if = "Option::is_none")]
    pub blocked: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub dormant: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PheromoneEdge {
    pub weight: f64,
    pub last_seen: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BlockedPoint {
    pub node: String,
    pub context: String,
    pub since: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CognitiveTrail {
    pub entry: String,
    pub exit: String,
    pub date: String,
    pub emotion: EmotionMode,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum EmotionMode {
    #[serde(rename = "A")] Angry,
    #[serde(rename = "B")] Happy,
    #[serde(rename = "C")] Sad,
    #[serde(rename = "N")] Neutral,
}
```

### 4.2 关键差异（TS → Rust）

| TS | Rust | 原因 |
|----|------|------|
| `number` 浮点 | `f64` | 直接对应 |
| `RegExp.test()` | `Regex::is_match()` | Rust 无状态正则 |
| `RegExp.exec()` 循环 | `Regex::captures_iter()` | 一次性迭代器，无 lastIndex 问题 |
| `JSON.parse(JSON.stringify(g))` 深拷贝 | 自动通过 `Clone` derive | Rust 所有权语义天然安全 |
| `Object.entries().sort()` | `Vec::from_iter(...).sort_by()` | HashMap 无序 |
| `Array.slice(-5)` | `vec.iter().rev().take(5)` | 惯用方式 |

---

## 5. 模块设计

### 5.1 `stopwords.rs` — 停用词表

TS 版有 ~200 个中英文停用词硬编码。Rust 版独立文件：

```rust
use std::collections::HashSet;
use once_cell::sync::Lazy;

pub static STOP_WORDS: Lazy<HashSet<&'static str>> = Lazy::new(|| {
    HashSet::from([
        // English
        "the", "a", "an", "is", "are", "was", "were", "be", "been", "being",
        "have", "has", "had", "do", "does", "did", "will", "would", "shall",
        "should", "may", "might", "must", "can", "could", "i", "you", "he",
        "she", "it", "we", "they", "me", "him", "her", "us", "them",
        "my", "your", "his", "its", "our", "their", "this", "that", "these",
        "those", "what", "which", "who", "whom", "how", "when", "where", "why",
        "all", "any", "both", "each", "more", "most", "other", "some", "such",
        "no", "nor", "not", "only", "own", "same", "so", "than", "too", "very",
        "just", "but", "and", "or", "as", "at", "by", "for", "in", "of", "on",
        "to", "up", "with", "from", "into", "about", "like", "also", "then",
        "if", "because", "while", "although", "though", "since", "until",
        "unless", "ok", "okay", "yes", "no", "hi", "hello", "thanks", "thank",
        "please", "sorry", "sure",
        "new", "get", "set", "use", "make", "take", "give", "show", "find",
        "know", "see", "say", "tell", "ask", "try", "run", "add", "put", "let",
        "got", "now", "one", "two", "way", "day", "time", "thing", "things",
        "good", "bad", "big", "small", "need", "want", "look", "work", "help",
        "here", "there", "come", "back", "out", "via", "per", "etc",
        // Chinese
        "的", "了", "在", "是", "我", "你", "他", "她", "它", "们",
        "这", "那", "有", "和", "就", "不", "也", "都", "而", "及",
        "与", "着", "或", "于", "一个", "可以", "什么", "怎么", "如何",
        "可能", "应该", "需要", "我们", "你们", "他们", "因为", "所以",
        "但是", "然后", "如果", "虽然", "对于", "关于", "通过", "进行",
        "使用", "没有", "一些", "这些", "那些", "这个", "那个", "这里",
        "那里", "现在", "时候", "好的", "谢谢", "请问", "您好", "对",
        "嗯", "吗", "呢", "啊", "哦", "哈", "嗯嗯", "一下", "一点",
        "已经", "还是", "只是", "其实", "不是", "还有", "就是", "来说",
        "来看",
        "哪些", "哪里", "哪个", "哪种", "多少", "几个", "几种", "怎样",
        "为何", "为什么", "什么样", "有哪些", "有什么", "是什么", "怎么办",
        "如何做", "可以吗", "好吗", "行吗", "对吗",
        "帮我", "帮你", "告诉", "知道", "觉得", "认为", "感觉", "看看",
        "说说", "想想", "试试", "做到", "做好", "完成", "实现", "开始",
        "继续", "停止", "修改", "更新", "添加", "删除",
        "新", "旧", "大", "小", "多", "少", "快", "慢", "好", "差",
        "高", "低", "上", "下", "左", "右",
    ])
});
```

### 5.2 `extract.rs` — 核心提取

| 函数 | 输入 | 输出 | TS 对应 |
|------|------|------|---------|
| `extract_topics(text: &str) -> Vec<String>` | 用户/助手消息 | 前 6 个高频话题 | 完全相同 |
| `detect_emotion(text: &str) -> EmotionMode` | 用户消息 | 主导情绪 | 完全相同（≥2 信号阈值） |
| `detect_blocked_topics(text: &str) -> Vec<String>` | 用户消息 | 知识盲区话题 | 完全相同 |

**实现要点：**

- **中文提取**：`\p{Han}{2,6}`（Rust regex 支持 Unicode 属性）
- **英文提取**：`[a-zA-Z]{3,}`
- **预处理**：去除 markdown 代码块 `` ```...``` ``、inline code `` `...` ``、URL、标点
- **情绪正则**：6–8 个正则模式 per 情绪模式，用 `Regex::is_match()` 计数，≥2 信号才归类
- **停用词过滤**：查 `STOP_WORDS` 集合
- **全局缓存**：所有 `Regex` 用 `Lazy<Regex>` 静态初始化，避免每轮编译

**情绪信号正则（与 TS 完全对齐）：**

| 模式 | 中文信号 | 英文信号 |
|------|---------|---------|
| A (愤怒) | `！{2,}`、`[草操艹尼玛妈的滚]`、烦死\|气死\|蠢\|傻\|垃圾\|什么破\|搞什么 | `fuck\|damn\|shit\|wtf\|stupid\|idiot`、`[A-Z]{4,}` |
| B (高兴) | `哈{2,}`、`666\|牛[啊哦]?\|太好了\|太棒了\|完美` | `awesome\|great\|excellent\|amazing\|wonderful`、emoji |
| C (沮丧) | `唉\|哎\|呜`、`算了\|没意思\|好累\|烦躁\|郁闷\|难过\|不想\|放弃` | `sigh\|tired\|frustrated\|depressed\|sad\|whatever\|meh` |

### 5.3 `engine.rs` — 图更新与衰减

| 函数 | TS 对应 |
|------|---------|
| `empty_graph() -> PheromoneGraph` | 完全相同 |
| `update_graph(graph, user_text, assistant_text) -> PheromoneGraph` | 完全相同 |
| `apply_decay(graph) -> PheromoneGraph` | 完全相同（**日期门控**：仅在 `today != last_decay` 时执行） |
| `apply_transitive_bridging(g)` (private) | 完全相同 |
| `prune_low_strength_nodes(g)` (private) | Rust 新增（见下文 LRU） |
| `should_inject_memory(graph, runs_since_last_inject, min_runs?) -> bool` | 完全相同 |

**关键常量（与 TS 保持一致）：**

```rust
const DECAY_RATE: f64 = 0.97;
const DORMANT_THRESHOLD: f64 = 0.05;
const STRENGTH_GAIN: f64 = 0.06;
const MAX_TOPICS_PER_TURN: usize = 6;
const MAX_HOT_NODES: usize = 12;
const MAX_HOT_EDGES: usize = 6;
const DEFAULT_INJECT_INTERVAL_RUNS: u32 = 5;
const BRIDGE_WEIGHT_THRESHOLD: f64 = 2.5;
const BRIDGE_INITIAL_WEIGHT: f64 = 0.4;

// Rust 新增：图规模防御
const MAX_NODES: usize = 200;             // 总节点数硬上限
const MAX_BRIDGE_OUT_DEGREE: usize = 20;  // 单节点桥接出度上限（防止 O(N²) 爆炸）
```

**情绪调制策略（与 TS 完全对齐）：**

| 模式 | 节点策略 | 边策略 |
|------|---------|--------|
| A（愤怒） | 首个话题 ×3.0 增益，其余 ×0.2 | 不创建新边 |
| B（高兴） | 所有话题 ×1.5 | 创建边权重 ×1.5，弱对也建边 |
| C（沮丧） | 仅已有节点 ×1.0，不建新节点 | 仅强化已有边 ×1.5 |
| N（中性） | 默认 ×1.0 | 默认 +1.0 |

**传递桥接算法（带出度上限）：**

```
1. 收集所有 weight ≥ 2.5 的强边 A→B
2. 构建邻接表 adj[A] = [B1, B2, ...]
3. 对每条强边 A→B，遍历 adj[B] 中每个 C：
   若 A→C 边不存在，创建 weight=0.4 的弱桥接
4. 若 adj[B] 超过 MAX_BRIDGE_OUT_DEGREE(20)，仅取前 20 个 C
```

**LRU 节点淘汰（Rust 新增）：**

当 `nodes.len() > MAX_NODES(200)` 时，在 `update_graph` 末尾触发：

```
1. 收集所有 strength < DORMANT_THRESHOLD 且 count ≤ 1 的节点
2. 按 last_seen 升序（最久未出现优先淘汰）
3. 移除节点及关联的边、blocked_points（此处点指向已删除节点）
4. 最多淘汰超出部分（即 nodes.len() - MAX_NODES）
```

**性能注意：**
- `apply_transitive_bridging` 复杂度 O(E + N*out_degree²)，在正常图规模下（<100 节点，<200 边，出度 <20）< 1ms
- `update_graph` 对输入 graph 做 `clone()` 保证不可变性。`PheromoneGraph` 的 `HashMap<String, ...>` clone 在 200 节点规模下 O(N+E)，预期 <100μs
- `apply_decay` 为日期门控：仅当 `today != graph.last_decay` 时才执行衰减逻辑，避免每轮空转

---

### 5.4 `render.rs` — Markdown 生成

```rust
pub fn generate_memory_section(
    graph: &PheromoneGraph,
    attribution: Option<&str>,   // Rust 新增：署名行，TS 版无此参数
) -> String
```

输出格式与 TS 完全一致，`attribution` 控制 header 副标题署名：

```markdown
## User Cognitive Map (auto-generated by DS Pick · do not edit this section)

### Frequent Topics
- **database** ████ (depth 3, 12 mentions)
- **cache** ██ (depth 2, 5 mentions)

### Common Associations
- database → indexing
- cache → performance

### Knowledge Boundaries (user indicated uncertainty)
- **quantum computing**: 我不知道量子计算是什么…

### Cognitive Trails (entry → exit per run)
- ✨ **database** → **performance** _(2026-05-20)_

### Recent Mood Tendency
- expansive/positive (B) across last 7 turns

_Updated 2026-05-20 · 5 active topics_
```

渲染规则：
- 前 12 个热节点（`strength ≥ 0.1`，非 dormant），按 strength 降序
- 前 6 条热边（按 weight 降序）
- 最近 5 个 blocked points
- 最近 8 条 cognitive trails（带情绪图标：⚡A / ✨B / 🌧C / ·N）
- 最近 10 轮情绪分布 → 主导模式

### 5.5 `inject.rs` — Markdown 标记注入

```rust
pub struct Markers {
    pub start: &'static str,
    pub end: &'static str,
}

pub const DEFAULT_MARKERS: Markers = Markers {
    start: "<!-- topic-memory-graph-start -->",
    end: "<!-- topic-memory-graph-end -->",
};

pub fn inject_memory_section(
    existing: &str,
    content: &str,
    markers: &Markers,
) -> String
```

逻辑：找到 `start` / `end` 对 → 替换中间内容；未找到 → 追加到末尾。约 20 行。

---

## 6. 外部依赖

### 6.1 需要先注册到 workspace 的依赖

`regex` 当前不在 root `Cargo.toml` 的 `[workspace.dependencies]` 中，需先添加：

```toml
# root Cargo.toml [workspace.dependencies] 追加
regex = "1.11"
once_cell = "1"
```

### 6.2 crate 级 `Cargo.toml`

```toml
[package]
name = "deepseek-topic-memory"
version = "0.1.0"
edition.workspace = true
rust-version.workspace = true

[dependencies]
serde = { workspace = true, features = ["derive"] }
serde_json.workspace = true          # 已在 workspace 中定义（v1.0.149）
regex.workspace = true               # 情绪检测 + 话题提取的正则（需先在 root Cargo.toml 注册）
chrono.workspace = true              # 日期处理 (today_str, days_between)
once_cell.workspace = true           # Lazy<Regex> 全局缓存（需先在 root Cargo.toml 注册）

[dev-dependencies]
tempfile = "3"
```

**不引入的依赖（刻意避开的陷阱）：**

- **无 NLP 库**（不调 LLM，不需要分词器/词性标注）
- **无 SQLite**（图以 JSON 文件持久化，不需要数据库）
- **无 Tokio**（同步逻辑，不需要异步运行时）

---

## 7. 与 DS Pick 的集成点

### 7.1 配置段：双层解析模式

Config 在 DS Pick 中有两层：TOML schema（`crates/config/src/lib.rs`）→ 运行时 config（`crates/tui/src/config.rs`）。新 `[topic_memory]` 遵循相同模式。

**A. TOML schema 层（`crates/config/src/lib.rs`）：**

```rust
/// On-disk schema for the `[topic_memory]` table.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct TopicMemoryToml {
    #[serde(default)]
    pub enabled: Option<bool>,
    #[serde(default)]
    pub graph_path: Option<PathBuf>,
    #[serde(default)]
    pub inject_interval: Option<u32>,
    #[serde(default)]
    pub attribution: Option<String>,
}
```

在 workspace-level `ConfigToml` struct 中（约 L252 `memory: Option<MemoryToml>` 的邻接位置）：

```rust
    #[serde(default)]
    pub topic_memory: Option<TopicMemoryToml>,
```

**B. 运行时解析层（`crates/tui/src/config.rs`）：**

```rust
/// Resolved topic-memory configuration. Default behaviour is **opt-in**
/// (`enabled` defaults to `false`).
#[derive(Debug, Clone, Default, Deserialize)]
pub struct TopicMemoryConfig {
    #[serde(default)]
    pub enabled: Option<bool>,
    #[serde(default)]
    pub graph_path: Option<PathBuf>,
    #[serde(default)]
    pub inject_interval: Option<u32>,
    #[serde(default)]
    pub attribution: Option<String>,
}

impl TopicMemoryConfig {
    #[must_use]
    pub fn is_enabled(&self) -> bool {
        self.enabled.unwrap_or(false)
    }

    #[must_use]
    pub fn inject_interval(&self) -> u32 {
        self.inject_interval.unwrap_or(5)
    }

    /// Resolves the graph path, defaulting to
    /// `~/.deepseek/topic-memory.json` when not set.
    #[must_use]
    pub fn graph_path(&self) -> PathBuf {
        self.graph_path.clone().unwrap_or_else(|| {
            dirs::home_dir()
                .unwrap_or_else(|| PathBuf::from("."))
                .join(".deepseek")
                .join("topic-memory.json")
        })
    }
}
```

在 `tui::config::Config` 中（约 L823 `pub memory: Option<MemoryConfig>` 邻接位置）：

```rust
    #[serde(default)]
    pub topic_memory: Option<TopicMemoryConfig>,
```

**C. `EngineConfig` 扁平字段：**

运行时 config 层将 `TopicMemoryConfig` 降维为 `EngineConfig` 上的 3 个字段：

```rust
// crates/tui/src/core/engine.rs — EngineConfig 新增
pub topic_memory_enabled: bool,
pub topic_memory_graph_path: PathBuf,
pub topic_memory_inject_interval: u32,
pub topic_memory_attribution: Option<String>,
```

Default 值：`topic_memory_enabled: false`，`topic_memory_graph_path` 用 `dirs::home_dir().join(".deepseek/topic-memory.json")`。

**D. 用户 `config.toml` 示例：**

```toml
[topic_memory]
enabled = true
graph_path = "~/.deepseek/topic-memory.json"
inject_interval = 5
attribution = "DS Pick"
```

### 7.2 图存储与加载

在每个 workspace 独立维护 topic graph 文件（路径从 `EngineConfig.topic_memory_graph_path` 获取，**不绑定 `Session.id`**——一个 workspace 内所有 session 共享同一份图）。

```rust
/// 加载图，文件不存在或损坏时返回空图。
fn load_topic_graph(path: &Path) -> PheromoneGraph {
    std::fs::read_to_string(path)
        .ok()
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_else(empty_graph)
}

/// 原子写入：先写临时文件，再 rename，防止写入中途崩溃导致 JSON 损坏。
fn save_topic_graph(graph: &PheromoneGraph, path: &Path) -> io::Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let json = serde_json::to_string_pretty(graph)?;
    let tmp_path = path.with_extension("tmp");
    std::fs::write(&tmp_path, &json)?;
    std::fs::rename(&tmp_path, path)?;
    Ok(())
}
```

### 7.3 Engine 集成（核心改动）

**A. `PromptSessionContext` 新增字段（`crates/tui/src/prompts.rs`）：**

```rust
// 在 PromptSessionContext 中追加（约 L18 邻接）：
pub topic_memory_block: Option<&'a str>,
```

这样 `topic_memory` 块由 `prompts::system_prompt_for_mode_with_context_skills_session_and_approval` 内部统一构造，与 `user_memory_block` 同级处理，**不直接在 `refresh_system_prompt` 中手动调用 `merge_system_prompts`**。

**B. `refresh_system_prompt` 改动（`engine.rs:1934`）：**

```rust
fn refresh_system_prompt(&mut self, mode: AppMode) {
    let user_memory_block =
        crate::memory::compose_block(self.config.memory_enabled, &self.config.memory_path);

    // 话题记忆块：仅当 enabled 且到达注入间隔时生成
    let topic_memory_block = if self.config.topic_memory_enabled
        && should_inject_memory(
            &self.session.topic_graph,
            self.session.topic_graph_runs_since_inject,
            Some(self.config.topic_memory_inject_interval),
        )
    {
        let section = generate_memory_section(
            &self.session.topic_graph,
            self.config.topic_memory_attribution.as_deref(),
        );
        self.session.topic_graph_runs_since_inject = 0;
        Some(section)
    } else {
        None
    };

    let base = prompts::system_prompt_for_mode_with_context_skills_session_and_approval(
        mode,
        &self.config.workspace,
        None,
        Some(&self.config.skills_dir),
        Some(&self.config.instructions),
        prompts::PromptSessionContext {
            user_memory_block: user_memory_block.as_deref(),
            topic_memory_block: topic_memory_block.as_deref(),  // 新增
            goal_objective: self.config.goal_objective.as_deref(),
            locale_tag: &self.config.locale_tag,
            task_type: self.config.task_type,
        },
        self.session.approval_mode,
    );

    let stable_prompt =
        merge_system_prompts(Some(&base), self.session.compaction_summary_prompt.clone());
    let stable_hash = system_prompt_hash(stable_prompt.as_ref());
    if self.session.last_system_prompt_hash != Some(stable_hash) {
        self.session.system_prompt = stable_prompt;
        self.session.last_system_prompt_hash = Some(stable_hash);
    }
}
```

**C. 关于 `system_prompt_hash` 去重：**

topic memory 块每 N 轮注入一次，且内容随图变化而变化，因此 `refresh_system_prompt` 的 hash 去重在注入轮必然命中（这是预期行为，topic memory 的设计本质就是动态内容）。非注入轮不受影响。

**D. 每轮对话结束后的图更新：**

在 `process_turn` 或 `submit_user_message` 的回调中：

```rust
let today = chrono::Utc::now().format("%Y-%m-%d").to_string();

// 日期门控衰减：仅在跨天时执行
if today != self.session.topic_graph.last_decay {
    self.session.topic_graph = apply_decay(&self.session.topic_graph);
}

// 更新话题图（内部含 LRU 淘汰）
self.session.topic_graph = update_graph(
    &self.session.topic_graph,
    &user_text,
    &assistant_text,
);

self.session.topic_graph_runs_since_inject += 1;

// 惰性持久化
let _ = save_topic_graph(
    &self.session.topic_graph,
    &self.config.topic_memory_graph_path,
);
```

**E. Session 启动时的图加载：**

```rust
// Engine::new() 中：
let topic_graph = load_topic_graph(&config.topic_memory_graph_path);
session.topic_graph = topic_graph;
session.topic_graph_runs_since_inject = 0;
```

> `topic_graph` 和 `topic_graph_runs_since_inject` 放在 `Session` 上是因为它们是会话生命周期内的可变状态。但**持久化路径来自 `EngineConfig`**（workspace 级），不依赖 `Session.id`。

### 7.4 Prompt 中记忆块的结构

与现有的 `<user_memory>` 和 `<project_instructions>` 并列，形成三层 project context：

```
System Prompt
├── <project_instructions> AGENTS.md 内容
├── <user_memory> 用户手写的 memory.md
└── <topic_memory> 自动提取的话题图记忆段
```

> 桌面端（Tauri）：`crates/desktop/src/` 的 prompt 组装链路中，在构建 system prompt 时从 `topic-memory` crate 直接调用 `generate_memory_section` / `should_inject_memory`，与 TUI 端共享同一 crate，无需重复实现。

---

## 8. 实施计划

### Phase 1：核心 crate（预计 2–3 天）

优先做 graph + extract + engine → 再 render + inject：

| 步骤 | 内容 | 验证 |
|------|------|------|
| 1.1a | root `Cargo.toml` 注册 `regex` / `once_cell` workspace dep | 编译通过 |
| 1.1b | 创建 `crates/topic-memory/`，配置 `Cargo.toml`，注册到 workspace | `cargo build -p deepseek-topic-memory` |
| 1.2 | 实现 `stopwords.rs`（~200 停用词静态表） | 单元测试：常见停用词被过滤 |
| 1.3 | 实现 `graph.rs`（5 个 struct + serde） | 序列化往返测试 |
| 1.4 | 实现 `extract.rs`（话题提取、情绪检测、盲区检测） | 对应 TS `engine.test.ts` 用例的 Rust 版本 |
| 1.5 | 实现 `engine.rs`（图更新、衰减、桥接、LRU 淘汰） | 对应 TS `engine.test.ts` 用例的 Rust 版本 |
| 1.6 | 实现 `render.rs`（Markdown 生成，含 attribution 参数） | 输出与 TS `generateMemorySection` 对比验证 |
| 1.7 | 实现 `inject.rs`（标记注入） | 对应 TS `inject.test.ts` 用例的 Rust 版本 |
| 1.8 | `lib.rs` 统一导出公共 API | `cargo test -p deepseek-topic-memory` 全部通过 |

### Phase 2：DS Pick 集成（预计 1–2 天）

| 步骤 | 内容 | 验证 |
|------|------|------|
| 2.1 | `crates/config/src/lib.rs` 新增 `TopicMemoryToml`；`crates/tui/src/config.rs` 新增 `TopicMemoryConfig`；`EngineConfig` 新增 4 个扁平字段 | `config.toml` 解析测试 |
| 2.2 | `Session` 新增 `topic_graph` / `topic_graph_runs_since_inject` 字段；Engine 初始化时加载图 | 编译通过 |
| 2.3 | `PromptSessionContext` 新增 `topic_memory_block`；`refresh_system_prompt` 注入 | 单元测试：系统提示词包含记忆段 |
| 2.4 | 每轮对话后调用 `update_graph`（含日期门控 `apply_decay`）+ 惰性持久化 | 集成测试：图在对话中累积 |
| 2.5 | 会话启动/结束时的图加载与持久化 | 文件读写测试（含原子写入验证） |
| 2.6 | 桌面端（Tauri）设置面板增加 `[topic_memory]` 开关；prompt 组装链路接入 | UI 配置项可用 |

### Phase 3：打磨（预计 1 天）

| 步骤 | 内容 |
|------|------|
| 3.1 | 情绪正则调优（基于实际 DS Pick 对话日志批量验证，而非直觉调参） |
| 3.2 | 停用词增补（消除噪音话题） |
| 3.3 | 性能 profile：`update_graph` 不应超过 5ms；桥接出度上限验证 |
| 3.4 | 文档：README、CHANGELOG 条目；`user_memory` vs `topic_memory` 对比说明 |

---

## 9. 测试策略

### 9.1 单元测试（crate 级）

每个模块独立测试，对标 TS 版 `test/engine.test.ts` 和 `test/inject.test.ts`：

| 测试组 | 用例数 | 覆盖 |
|--------|--------|------|
| `detect_emotion` | 8+ | A/B/C/N 四种模式，单信号不触发，中英混合 |
| `extract_topics` | 10+ | 中/英文提取，停用词过滤，markdown 剥离，频率排序，上限 6 |
| `detect_blocked_topics` | 5+ | 中/英文知识盲区模式 |
| `empty_graph` | 2 | 结构验证，每次返回新实例 |
| `update_graph` | 9+ | 不可变性，节点创建/累加，情绪记录，边创建/抑制，轨迹记录，盲区检测，桥接 |
| `apply_decay` | 5 | 当天无衰减，多天衰减数值，dormant 阈值，边删除阈值，last_decay 更新 |
| `prune_low_strength_nodes` | 4 | LRU 淘汰：超过 MAX_NODES 时触发、仅淘汰低 strength 节点、关联边同步删除 |
| `generate_memory_section` | 7+ | 空图 header，attribution（Rust 新增参数），节点排序，dormant 过滤，边输出，盲区输出，轨迹输出，情绪输出 |
| `should_inject_memory` | 5 | 无数据拦截，count 阈值，间隔阈值，自定义间隔 |
| `inject_memory_section` | 4 | 追加，替换，别名 marker，空文件 |

总计约 59 个测试用例（比初版增 4 个 LRU 淘汰测试）。

### 9.2 集成测试（workspace 级）

- `config.toml` 解析 `[topic_memory]` 段（双层：TOML → `TopicMemoryConfig` → `EngineConfig` 字段）
- system prompt 包含 `<topic_memory>` 块
- 多轮对话后图节点和边累积
- 衰减仅在跨天时生效（日期门控验证）
- LRU 淘汰在节点超限时触发

---

## 10. 风险与缓解

| 风险 | 影响 | 缓解 |
|------|------|------|
| 正则性能（每轮编译） | 首次调用卡顿 | `Lazy<Regex>` 全局缓存，仅在首次编译，后续 O(1) 查找 |
| 停用词不足导致噪音话题 | 话题列表充斥"我们/可以/需要" | 参考真实日志迭代增补；Phase 3 用 DS Pick 对话样本调优 |
| 长对话图膨胀 | 100+ 节点后桥接 O(N²) | `MAX_NODES=200` 硬上限 + LRU 淘汰低 strength 节点；桥接出度上限 `MAX_BRIDGE_OUT_DEGREE=20` |
| JSON 文件写入竞争 | 崩溃时可能留下损坏 JSON | 原子写入（先写 `.tmp` → `rename`），避免中途崩溃导致文件损坏 |
| 与 user_memory 功能重叠 | 用户困惑两个 memory 的定位 | §3 提供对比表；文档明确：`user_memory` = 手动声明，`topic_memory` = 自动提取 |
| `Regex::captures_iter` vs `RegExp.exec` 差异 | 边缘 case 结果不同 | TS 版 `exec` 的状态依赖是已知缺陷；Rust 无状态行为更正确 |
| system_prompt hash 去重被 topic memory 动态内容击穿 | 每轮重新组装 prompt | 这是预期行为——topic memory 本质就是动态内容；仅注入轮受影响，非注入轮维持去重 |
| 多 session 共享同 workspace 的图文件 | 并发写入覆盖 | 当前 engine 单 session 运行，暂不设锁；未来多 session 时加 `fs2` file lock |

---

## 11. 与 TS 版本的兼容性

- **图 JSON 格式**：`version: "0.1.0"`，字段名完全对齐，可用 JS 工具读取 Rust 生成的 JSON
- **Markdown 输出格式**：HTML 注释标记 `<!-- topic-memory-graph-start -->` / `<!-- ...end -->` 完全一致
- **情绪模式枚举**：A/B/C/N 值相同（serde rename 保证）
- **Rust 新增功能（不影响兼容性）**：
  - `generate_memory_section` 的 `attribution` 参数（TS 版无此参数，默认 `None` 时行为与 TS 一致）
  - LRU 节点淘汰（TS 版无此机制，但淘汰仅发生在节点超限时，正常使用不受影响）
  - 桥接出度上限（防御性措施，正常规模下不触发）
- **不兼容点**：TS 版 `RegExp.exec()` 的状态性在某些边缘情况下产生不同结果（Rust `captures_iter` 无状态）。这属于 bug 修复，不影响实际行为。

---

## 12. 总结

这个方案将 ~450 行 TypeScript 零依赖库移植为 ~550 行 Rust crate（含新增 LRU 淘汰 ~50 行 + 原子 IO ~20 行 + 出度上限 ~10 行）。核心逻辑 1:1 映射，利用 Rust 所有权消除手动深拷贝，通过 `Lazy<Regex>` 解决性能问题。

**与初版方案的关键差异：**

- 配置走双层解析（`TopicMemoryToml` → `TopicMemoryConfig` → `EngineConfig` 扁平字段），与现有 `[memory]` 模式一致
- topic_memory_block 通过 `PromptSessionContext` 注入而非手动 `merge_system_prompts`
- `apply_decay` 改为日期门控（仅在跨天时执行），避免每轮空转
- 新增 LRU 节点淘汰（`MAX_NODES=200`）+ 桥接出度上限（`MAX_BRIDGE_OUT_DEGREE=20`）作为图膨胀防御
- 持久化采用原子写入（tmp + rename），消除 JSON 损坏风险
- `topic_graph_path` 为 workspace 级配置，不绑定 `Session.id`
- 桌面端（Tauri）集成路径已标注
