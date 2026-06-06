//! Graph update, decay, and bridging.

use std::collections::HashSet;

use chrono::NaiveDate;

use crate::extract::{detect_blocked_topics, detect_emotion, extract_topics};
use crate::graph::{
    BlockedPoint, CognitiveTrail, EmotionMode, GRAPH_SCHEMA_VERSION, PheromoneEdge, PheromoneGraph,
    PheromoneNode,
};

const DECAY_RATE: f64 = 0.97;
const DORMANT_THRESHOLD: f64 = 0.05;
const STRENGTH_GAIN: f64 = 0.06;
const MAX_TOPICS_PER_TURN: usize = 6;
const MAX_HOT_NODES: usize = 12;
const BRIDGE_WEIGHT_THRESHOLD: f64 = 2.5;
const BRIDGE_INITIAL_WEIGHT: f64 = 0.4;

/// Default: inject after this many completed runs.
pub const DEFAULT_INJECT_INTERVAL_RUNS: u32 = 5;

#[must_use]
pub fn today_str() -> String {
    NaiveDate::from(chrono::Local::now().date_naive())
        .format("%Y-%m-%d")
        .to_string()
}

fn days_between(iso_a: &str, iso_b: &str) -> i64 {
    let parse = |s: &str| NaiveDate::parse_from_str(s, "%Y-%m-%d").ok();
    match (parse(iso_a), parse(iso_b)) {
        (Some(a), Some(b)) => (b - a).num_days().unsigned_abs() as i64,
        _ => 0,
    }
}

#[must_use]
pub fn empty_graph() -> PheromoneGraph {
    PheromoneGraph {
        version: GRAPH_SCHEMA_VERSION.to_string(),
        last_decay: today_str(),
        nodes: Default::default(),
        edges: Default::default(),
        blocked_points: Vec::new(),
        recent_emotions: None,
        trails: None,
    }
}

fn node_gain_multiplier(
    graph: &PheromoneGraph,
    emotion: EmotionMode,
    topic: &str,
    idx: usize,
) -> f64 {
    match emotion {
        EmotionMode::Angry => {
            if idx == 0 {
                3.0
            } else {
                0.2
            }
        }
        EmotionMode::Happy => 1.5,
        EmotionMode::Sad => {
            if graph.nodes.contains_key(topic) {
                1.0
            } else {
                0.0
            }
        }
        EmotionMode::Neutral => 1.0,
    }
}

fn apply_transitive_bridging(graph: &mut PheromoneGraph, today: &str) {
    let strong: Vec<(String, String)> = graph
        .edges
        .iter()
        .filter(|(_, e)| e.weight >= BRIDGE_WEIGHT_THRESHOLD)
        .filter_map(|(key, _)| {
            let mut parts = key.split('→');
            Some((parts.next()?.to_string(), parts.next()?.to_string()))
        })
        .collect();

    let mut adj: std::collections::HashMap<String, Vec<String>> = std::collections::HashMap::new();
    for (a, b) in &strong {
        adj.entry(a.clone()).or_default().push(b.clone());
    }

    for (a, b) in strong {
        for c in adj.get(&b).into_iter().flatten() {
            if c == &a {
                continue;
            }
            let bridge_key = format!("{a}→{c}");
            graph
                .edges
                .entry(bridge_key)
                .or_insert_with(|| PheromoneEdge {
                    weight: BRIDGE_INITIAL_WEIGHT,
                    last_seen: today.to_string(),
                });
        }
    }
}

/// Update graph after one conversation turn.
#[must_use]
pub fn update_graph(
    graph: &PheromoneGraph,
    user_text: &str,
    assistant_text: &str,
) -> PheromoneGraph {
    let today = today_str();
    let mut g = graph.clone();
    let emotion = detect_emotion(user_text);

    let user_topics = extract_topics(user_text);
    let assistant_topics = extract_topics(assistant_text);
    let mut all_topics: Vec<String> = user_topics
        .iter()
        .chain(assistant_topics.iter())
        .cloned()
        .collect::<HashSet<_>>()
        .into_iter()
        .collect();
    all_topics.truncate(MAX_TOPICS_PER_TURN);

    for (idx, topic) in all_topics.iter().enumerate() {
        let mult = node_gain_multiplier(&g, emotion, topic, idx);
        if mult == 0.0 {
            continue;
        }
        let gain = STRENGTH_GAIN * mult;
        if let Some(existing) = g.nodes.get_mut(topic) {
            existing.count += 1;
            existing.strength = (existing.strength + gain).min(1.0);
            existing.last_seen = today.clone();
            existing.dormant = Some(false);
            if user_topics.contains(topic) && assistant_topics.contains(topic) {
                existing.depth = (existing.depth + 0.2).min(5.0);
            }
        } else {
            g.nodes.insert(
                topic.clone(),
                PheromoneNode {
                    count: 1,
                    last_seen: today.clone(),
                    strength: gain,
                    depth: if user_topics.contains(topic) {
                        2.0
                    } else {
                        1.0
                    },
                    blocked: None,
                    dormant: None,
                },
            );
        }
    }

    for i in 0..user_topics.len() {
        for j in (i + 1)..user_topics.len() {
            let key = format!("{}→{}", user_topics[i], user_topics[j]);
            match emotion {
                EmotionMode::Angry => continue,
                EmotionMode::Sad => {
                    if let Some(e) = g.edges.get_mut(&key) {
                        e.weight += 1.5;
                        e.last_seen = today.clone();
                    }
                    continue;
                }
                EmotionMode::Happy | EmotionMode::Neutral => {
                    if let Some(e) = g.edges.get_mut(&key) {
                        e.weight += if emotion == EmotionMode::Happy {
                            1.5
                        } else {
                            1.0
                        };
                        e.last_seen = today.clone();
                    } else {
                        g.edges.insert(
                            key,
                            PheromoneEdge {
                                weight: 1.0,
                                last_seen: today.clone(),
                            },
                        );
                    }
                }
            }
        }
    }

    let emotions = g.recent_emotions.get_or_insert_with(Vec::new);
    emotions.push(emotion);
    if emotions.len() > 10 {
        emotions.remove(0);
    }

    let unique_user: Vec<String> = user_topics
        .into_iter()
        .collect::<HashSet<_>>()
        .into_iter()
        .collect();
    if unique_user.len() >= 2 {
        let trails = g.trails.get_or_insert_with(Vec::new);
        trails.push(CognitiveTrail {
            entry: unique_user.first().cloned().unwrap_or_default(),
            exit: unique_user.last().cloned().unwrap_or_default(),
            date: today.clone(),
            emotion,
        });
        if trails.len() > 20 {
            trails.remove(0);
        }
    }

    apply_transitive_bridging(&mut g, &today);

    for b_topic in detect_blocked_topics(user_text) {
        if !g.blocked_points.iter().any(|b| b.node == b_topic) {
            g.blocked_points.push(BlockedPoint {
                node: b_topic.clone(),
                context: user_text
                    .chars()
                    .take(80)
                    .collect::<String>()
                    .trim()
                    .to_string(),
                since: today.clone(),
            });
            if let Some(n) = g.nodes.get_mut(&b_topic) {
                n.blocked = Some(true);
            }
        }
    }

    g
}

/// Apply time-based decay (at most once per calendar day).
#[must_use]
pub fn apply_decay(graph: &PheromoneGraph) -> PheromoneGraph {
    let today = today_str();
    let mut g = graph.clone();
    let days = days_between(&g.last_decay, &today);
    if days == 0 {
        return g;
    }
    let factor = DECAY_RATE.powi(days as i32);

    for node in g.nodes.values_mut() {
        node.strength *= factor;
        if node.strength < DORMANT_THRESHOLD {
            node.dormant = Some(true);
        }
    }

    g.edges.retain(|_, e| {
        e.weight *= factor;
        e.weight >= 0.5
    });

    g.last_decay = today;
    g
}

pub struct GenerateMemorySectionOptions<'a> {
    pub attribution: Option<&'a str>,
}

/// Markdown section for system prompt injection.
#[must_use]
pub fn generate_memory_section(
    graph: &PheromoneGraph,
    options: Option<GenerateMemorySectionOptions<'_>>,
) -> String {
    let mut hot_nodes: Vec<_> = graph
        .nodes
        .iter()
        .filter(|(_, n)| !n.dormant.unwrap_or(false) && n.strength >= 0.1)
        .collect();
    hot_nodes.sort_by(|a, b| {
        b.1.strength
            .partial_cmp(&a.1.strength)
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    hot_nodes.truncate(MAX_HOT_NODES);

    let mut hot_edges: Vec<_> = graph.edges.iter().collect();
    hot_edges.sort_by(|a, b| {
        b.1.weight
            .partial_cmp(&a.1.weight)
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    hot_edges.truncate(6);

    let header_tail = options
        .and_then(|o| o.attribution)
        .map(|a| format!(" (auto-generated by {a} · do not edit this section)"))
        .unwrap_or_else(|| " (auto-generated · do not edit this section)".to_string());

    let mut lines = vec![
        format!("## User Cognitive Map{header_tail}"),
        String::new(),
        "### Frequent Topics".to_string(),
    ];

    if hot_nodes.is_empty() {
        lines.push("- (not enough data yet)".to_string());
    } else {
        for (topic, n) in hot_nodes {
            let bar_len = (n.strength * 5.0).round() as usize;
            let bar = "█".repeat(bar_len);
            lines.push(format!(
                "- **{topic}** {bar} (depth {}, {} mentions)",
                n.depth.round() as i32,
                n.count
            ));
        }
    }

    if !hot_edges.is_empty() {
        lines.push(String::new());
        lines.push("### Common Associations".to_string());
        for (edge, _) in hot_edges {
            lines.push(format!("- {}", edge.replace('→', " → ")));
        }
    }

    let active_blocked: Vec<_> = graph.blocked_points.iter().rev().take(5).collect();
    if !active_blocked.is_empty() {
        lines.push(String::new());
        lines.push("### Knowledge Boundaries (user indicated uncertainty)".to_string());
        for b in active_blocked {
            let ctx: String = b.context.chars().take(60).collect();
            lines.push(format!("- **{}**: {ctx}…", b.node));
        }
    }

    if let Some(trails) = &graph.trails {
        let recent: Vec<_> = trails.iter().rev().take(8).collect();
        if !recent.is_empty() {
            lines.push(String::new());
            lines.push("### Cognitive Trails (entry → exit per run)".to_string());
            for tr in recent {
                let icon = match tr.emotion {
                    EmotionMode::Angry => "⚡",
                    EmotionMode::Happy => "✨",
                    EmotionMode::Sad => "🌧",
                    EmotionMode::Neutral => "·",
                };
                lines.push(format!(
                    "- {icon} **{}** → **{}** _({})_",
                    tr.entry, tr.exit, tr.date
                ));
            }
        }
    }

    if let Some(emotions) = &graph.recent_emotions {
        if !emotions.is_empty() {
            let mut counts = [0u32; 4];
            for e in emotions {
                match e {
                    EmotionMode::Angry => counts[0] += 1,
                    EmotionMode::Happy => counts[1] += 1,
                    EmotionMode::Sad => counts[2] += 1,
                    EmotionMode::Neutral => counts[3] += 1,
                }
            }
            let dominant = [
                (EmotionMode::Angry, counts[0]),
                (EmotionMode::Happy, counts[1]),
                (EmotionMode::Sad, counts[2]),
                (EmotionMode::Neutral, counts[3]),
            ]
            .into_iter()
            .max_by_key(|(_, c)| *c)
            .map(|(m, _)| m)
            .unwrap_or(EmotionMode::Neutral);
            let label = match dominant {
                EmotionMode::Angry => "focused/intense (A)",
                EmotionMode::Happy => "expansive/positive (B)",
                EmotionMode::Sad => "ruminant/low-energy (C)",
                EmotionMode::Neutral => "neutral (N)",
            };
            lines.push(String::new());
            lines.push("### Recent Mood Tendency".to_string());
            lines.push(format!("- {label} across last {} turns", emotions.len()));
        }
    }

    lines.push(String::new());
    lines.push(format!(
        "_Updated {} · {} active topics_",
        today_str(),
        graph
            .nodes
            .values()
            .filter(|n| !n.dormant.unwrap_or(false) && n.strength >= 0.1)
            .count()
    ));

    lines.join("\n")
}

/// Whether enough runs have passed to inject memory into the prompt.
#[must_use]
pub fn should_inject_memory(
    graph: &PheromoneGraph,
    runs_since_last_inject: u32,
    min_runs_between_inject: u32,
) -> bool {
    let has_data = graph.nodes.values().any(|n| n.count >= 2);
    has_data && runs_since_last_inject >= min_runs_between_inject
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn update_and_inject_flow() {
        let mut g = empty_graph();
        for _ in 0..3 {
            g = update_graph(&g, "我们在讨论 Rust 异步编程", "好的，async await 很重要");
        }
        assert!(g.nodes.values().any(|n| n.count >= 2));
        let section = generate_memory_section(&g, None);
        assert!(section.contains("User Cognitive Map"));
        assert!(should_inject_memory(
            &g,
            DEFAULT_INJECT_INTERVAL_RUNS,
            DEFAULT_INJECT_INTERVAL_RUNS
        ));
    }
}
