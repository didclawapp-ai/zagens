//! k-hop subgraph retrieval for token-efficient injection (B2.3).

use std::collections::{HashMap, HashSet};

use crate::extract::extract_topics;
use crate::graph::{
    BlockedPoint, CognitiveTrail, PheromoneGraph, GRAPH_SCHEMA_VERSION,
};

/// Default BFS depth from seed topics (UNDERLYING §2.3).
pub const DEFAULT_RETRIEVE_K_HOPS: usize = 2;

fn parse_edge_endpoints(key: &str) -> Option<(&str, &str)> {
    let mut parts = key.split('→');
    let a = parts.next()?;
    let b = parts.next()?;
    Some((a, b))
}

fn neighbors(graph: &PheromoneGraph, node: &str) -> Vec<String> {
    let mut out = Vec::new();
    for (key, _) in &graph.edges {
        if let Some((a, b)) = parse_edge_endpoints(key) {
            if a == node && graph.nodes.contains_key(b) {
                out.push(b.to_string());
            } else if b == node && graph.nodes.contains_key(a) {
                out.push(a.to_string());
            }
        }
    }
    out
}

/// Collect seed topics that exist in the graph; fall back to fuzzy overlap with node keys.
fn resolve_seeds(graph: &PheromoneGraph, seeds: &[String]) -> HashSet<String> {
    let mut resolved = HashSet::new();
    for s in seeds {
        if graph.nodes.contains_key(s) {
            resolved.insert(s.clone());
            continue;
        }
        for key in graph.nodes.keys() {
            if key.contains(s.as_str()) || s.contains(key.as_str()) {
                resolved.insert(key.clone());
            }
        }
    }
    resolved
}

/// BFS k-hop neighborhood around `seeds` (undirected over edge endpoints).
#[must_use]
pub fn retrieve_k_hop_subgraph(
    graph: &PheromoneGraph,
    seeds: &[String],
    k: usize,
) -> PheromoneGraph {
    let mut visited = resolve_seeds(graph, seeds);
    if visited.is_empty() {
        return graph.clone();
    }

    let mut frontier: Vec<String> = visited.iter().cloned().collect();
    for _ in 0..k {
        let mut next = Vec::new();
        for node in &frontier {
            for nb in neighbors(graph, node) {
                if visited.insert(nb.clone()) {
                    next.push(nb);
                }
            }
        }
        if next.is_empty() {
            break;
        }
        frontier = next;
    }

    induced_subgraph(graph, &visited)
}

/// Build a subgraph containing only `nodes` and edges between them.
#[must_use]
pub fn induced_subgraph(graph: &PheromoneGraph, nodes: &HashSet<String>) -> PheromoneGraph {
    if nodes.is_empty() {
        return PheromoneGraph {
            version: graph.version.clone(),
            last_decay: graph.last_decay.clone(),
            nodes: HashMap::new(),
            edges: HashMap::new(),
            blocked_points: Vec::new(),
            recent_emotions: graph.recent_emotions.clone(),
            trails: None,
        };
    }

    let nodes_map: HashMap<_, _> = graph
        .nodes
        .iter()
        .filter(|(k, _)| nodes.contains(*k))
        .map(|(k, v)| (k.clone(), v.clone()))
        .collect();

    let edges: HashMap<_, _> = graph
        .edges
        .iter()
        .filter(|(key, _)| {
            parse_edge_endpoints(key)
                .map(|(a, b)| nodes.contains(a) && nodes.contains(b))
                .unwrap_or(false)
        })
        .map(|(k, v)| (k.clone(), v.clone()))
        .collect();

    let blocked_points: Vec<BlockedPoint> = graph
        .blocked_points
        .iter()
        .filter(|b| nodes.contains(&b.node))
        .cloned()
        .collect();

    let trails: Option<Vec<CognitiveTrail>> = graph.trails.as_ref().map(|ts| {
        ts.iter()
            .filter(|t| nodes.contains(&t.entry) || nodes.contains(&t.exit))
            .cloned()
            .collect()
    });

    PheromoneGraph {
        version: GRAPH_SCHEMA_VERSION.to_string(),
        last_decay: graph.last_decay.clone(),
        nodes: nodes_map,
        edges,
        blocked_points,
        recent_emotions: graph.recent_emotions.clone(),
        trails,
    }
}

/// Extract seeds from query text and return a k-hop subgraph.
#[must_use]
pub fn retrieve_for_query(graph: &PheromoneGraph, query: &str, k: usize) -> PheromoneGraph {
    let seeds = extract_topics(query);
    if seeds.is_empty() {
        return graph.clone();
    }
    retrieve_k_hop_subgraph(graph, &seeds, k)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::{empty_graph, update_graph};

    #[test]
    fn k_hop_limits_nodes() {
        let mut g = empty_graph();
        for _ in 0..3 {
            g = update_graph(&g, "讨论 Rust 异步", "async await 很重要");
            g = update_graph(&g, "Tokio runtime 调度", "executor 与 reactor");
            g = update_graph(&g, "内存安全与所有权", "borrow checker");
        }
        let full_count = g.nodes.len();
        assert!(full_count >= 2);
        let seeds = vec!["rust".to_string()];
        let sub = retrieve_k_hop_subgraph(&g, &seeds, 1);
        assert!(sub.nodes.len() <= full_count);
        assert!(!sub.nodes.is_empty());
    }
}
