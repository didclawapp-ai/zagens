//! T5 tool-sequence mining (Phase 4.3) — data-driven composite-tool candidacy from T1.

use std::collections::HashMap;

use serde::Serialize;
use zagens_core::engine::kernel_event::{KernelEvent, KernelEventEnvelope};

/// Minimum share of tool turns that must contain a pattern before T5 ships it.
pub const T5_MIN_TURN_SHARE_PCT: f64 = 5.0;

/// Canonical explore subsequence mined for `explore_codebase`.
pub const EXPLORE_SUBSEQUENCE: &[&str] = &["glob_files", "grep_files", "read_file"];

/// Canonical edit+verify subsequence mined for `edit_and_check`.
pub const EDIT_CHECK_SUBSEQUENCE: &[&str] = &["edit_file", "run_tests"];

/// Alternate edit+verify path (cargo test via shell).
pub const EDIT_SHELL_CHECK_SUBSEQUENCE: &[&str] = &["edit_file", "exec_shell"];

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct ToolSequenceStat {
    pub pattern: String,
    pub turn_hits: u64,
    pub turn_share_pct: f64,
    pub t5_eligible: bool,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct ToolSequenceReport {
    pub turns_with_tools: u64,
    pub threshold_pct: f64,
    pub top_patterns: Vec<ToolSequenceStat>,
    pub t5_candidates: Vec<ToolSequenceStat>,
}

#[must_use]
pub fn mine_tool_sequences(envelopes: &[KernelEventEnvelope]) -> ToolSequenceReport {
    let mut turn_tools: HashMap<String, Vec<String>> = HashMap::new();

    for envelope in envelopes {
        if let KernelEvent::ToolCallFinished {
            turn_id, tool_name, ..
        } = &envelope.event
        {
            let name = normalize_tool_name(&tool_name);
            turn_tools.entry(turn_id.clone()).or_default().push(name);
        }
    }

    let turns_with_tools = turn_tools.len() as u64;
    if turns_with_tools == 0 {
        return ToolSequenceReport {
            turns_with_tools: 0,
            threshold_pct: T5_MIN_TURN_SHARE_PCT,
            top_patterns: Vec::new(),
            t5_candidates: Vec::new(),
        };
    }

    let patterns = [
        EXPLORE_SUBSEQUENCE,
        &["grep_files", "read_file"],
        EDIT_CHECK_SUBSEQUENCE,
        EDIT_SHELL_CHECK_SUBSEQUENCE,
        &["write_file", "run_tests"],
        &["edit_file", "assert_tests_pass"],
        &["glob_files", "read_file"],
        &["list_dir", "read_file"],
    ];

    let mut stats: Vec<ToolSequenceStat> = patterns
        .iter()
        .map(|p| {
            let hits = count_subsequence_hits(&turn_tools, p);
            let share = (hits as f64 / turns_with_tools as f64 * 100.0 * 100.0).round() / 100.0;
            ToolSequenceStat {
                pattern: p.join("→"),
                turn_hits: hits,
                turn_share_pct: share,
                t5_eligible: share >= T5_MIN_TURN_SHARE_PCT,
            }
        })
        .collect();

    stats.extend(mine_adjacent_bigrams(&turn_tools, turns_with_tools));
    stats.sort_by(|a, b| {
        b.turn_share_pct
            .partial_cmp(&a.turn_share_pct)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| a.pattern.cmp(&b.pattern))
    });
    stats.truncate(15);

    let t5_candidates: Vec<ToolSequenceStat> =
        stats.iter().filter(|s| s.t5_eligible).cloned().collect();

    ToolSequenceReport {
        turns_with_tools,
        threshold_pct: T5_MIN_TURN_SHARE_PCT,
        top_patterns: stats,
        t5_candidates,
    }
}

fn normalize_tool_name(name: &str) -> String {
    if name.trim().is_empty() {
        "<unknown>".to_string()
    } else {
        name.to_string()
    }
}

fn count_subsequence_hits(turn_tools: &HashMap<String, Vec<String>>, pattern: &[&str]) -> u64 {
    if pattern.is_empty() {
        return 0;
    }
    turn_tools
        .values()
        .filter(|tools| contains_subsequence(tools, pattern))
        .count() as u64
}

fn contains_subsequence(tools: &[String], pattern: &[&str]) -> bool {
    let mut pat_idx = 0;
    for tool in tools {
        if tool == pattern[pat_idx] {
            pat_idx += 1;
            if pat_idx == pattern.len() {
                return true;
            }
        }
    }
    false
}

fn mine_adjacent_bigrams(
    turn_tools: &HashMap<String, Vec<String>>,
    turns_with_tools: u64,
) -> Vec<ToolSequenceStat> {
    let mut counts: HashMap<(String, String), u64> = HashMap::new();
    for tools in turn_tools.values() {
        let mut seen_in_turn = HashMap::<(String, String), ()>::new();
        for pair in tools.windows(2) {
            let key = (pair[0].clone(), pair[1].clone());
            if seen_in_turn.insert(key.clone(), ()).is_none() {
                *counts.entry(key).or_default() += 1;
            }
        }
    }

    let mut stats: Vec<ToolSequenceStat> = counts
        .into_iter()
        .map(|((a, b), hits)| {
            let share = (hits as f64 / turns_with_tools as f64 * 100.0 * 100.0).round() / 100.0;
            ToolSequenceStat {
                pattern: format!("{a}→{b}"),
                turn_hits: hits,
                turn_share_pct: share,
                t5_eligible: share >= T5_MIN_TURN_SHARE_PCT,
            }
        })
        .filter(|s| s.turn_hits >= 2)
        .collect();
    stats.sort_by(|a, b| {
        b.turn_share_pct
            .partial_cmp(&a.turn_share_pct)
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    stats.truncate(5);
    stats
}

#[cfg(test)]
mod tests {
    use super::*;
    use rusqlite::Connection;
    use zagens_core::engine::kernel_event::{KernelEvent, ToolOutcome};
    use zagens_core::turn::TurnLoopMode;
    use zagens_runtime_adapters::persist::kernel_event_log::{
        KernelEventLog, ensure_kernel_events_table,
    };

    fn append_tool(log: &mut KernelEventLog<'_>, turn_id: &str, tool: &str) {
        log.append(KernelEvent::ToolCallFinished {
            turn_id: turn_id.to_string(),
            call_id: format!("call-{tool}-{}", turn_id),
            tool_name: tool.to_string(),
            outcome: ToolOutcome::Success,
            duration_ms: 1,
            wrote_state: false,
            result_preview: String::new(),
            session_content: String::new(),
        })
        .expect("append");
    }

    #[test]
    fn detects_explore_subsequence_share() {
        let dir = tempfile::tempdir().expect("tempdir");
        let db_path = dir.path().join("sessions.db");
        let conn = Connection::open(&db_path).expect("open");
        ensure_kernel_events_table(&conn).expect("migrate");
        let mut log = KernelEventLog::new(&conn);

        for i in 0..20 {
            let tid = format!("turn-{i}");
            log.append(KernelEvent::TurnStarted {
                turn_id: tid.clone(),
                mode: TurnLoopMode::Agent,
                input_text: "x".into(),
                max_steps: 5,
            })
            .expect("turn");
            if i < 2 {
                append_tool(&mut log, &tid, "glob_files");
                append_tool(&mut log, &tid, "grep_files");
                append_tool(&mut log, &tid, "read_file");
            } else {
                append_tool(&mut log, &tid, "read_file");
            }
        }

        let envelopes = log
            .load_events_by_kinds(&["tool_call_finished"])
            .expect("load");
        let report = mine_tool_sequences(&envelopes);
        assert_eq!(report.turns_with_tools, 20);
        let explore = report
            .top_patterns
            .iter()
            .find(|s| s.pattern == "glob_files→grep_files→read_file")
            .expect("explore pattern");
        assert_eq!(explore.turn_hits, 2);
        assert!((explore.turn_share_pct - 10.0).abs() < 0.01);
        assert!(explore.t5_eligible);
    }
}
