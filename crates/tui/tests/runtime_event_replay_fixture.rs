//! A5.5 minimal runtime event replay fixture — validates monotonic seq and lifecycle ordering.

use std::io::{BufRead, BufReader};
use std::path::PathBuf;

use chrono::{DateTime, Utc};
use serde::Deserialize;
use serde_json::Value;

#[derive(Debug, Deserialize)]
struct FixtureEvent {
    schema_version: u32,
    seq: u64,
    timestamp: DateTime<Utc>,
    thread_id: String,
    turn_id: Option<String>,
    event: String,
    payload: Value,
}

fn fixture_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/runtime_turn_minimal.jsonl")
}

fn load_fixture() -> Vec<FixtureEvent> {
    let file = std::fs::File::open(fixture_path()).expect("open runtime_turn_minimal.jsonl");
    let reader = BufReader::new(file);
    reader
        .lines()
        .map(|line| {
            let line = line.expect("read line");
            if line.trim().is_empty() {
                return None;
            }
            Some(serde_json::from_str(&line).expect("parse fixture event"))
        })
        .filter_map(|e| e)
        .collect()
}

#[test]
fn runtime_turn_minimal_fixture_has_monotonic_seq_and_lifecycle() {
    let events = load_fixture();
    assert!(
        events.len() >= 3,
        "fixture should cover at least started → item → completed"
    );

    let mut prev_seq = 0u64;
    for ev in &events {
        assert_eq!(ev.schema_version, 2);
        assert!(ev.seq > prev_seq, "seq must be strictly increasing");
        prev_seq = ev.seq;
        assert_eq!(ev.thread_id, "thr_fixture");
    }

    let names: Vec<&str> = events.iter().map(|e| e.event.as_str()).collect();
    assert_eq!(names.first(), Some(&"turn.started"));
    assert!(names.iter().any(|e| *e == "item.completed"));
    assert_eq!(names.last(), Some(&"turn.completed"));
}
