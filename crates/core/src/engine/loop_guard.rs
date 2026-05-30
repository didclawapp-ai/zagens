//! Pure-data guardrails for repeated tool-call loops (P2 PR4 → `deepseek-core`).

use std::collections::HashMap;
use std::collections::hash_map::DefaultHasher;
use std::fmt::Write as _;
use std::hash::{Hash, Hasher};

use serde_json::Value;

const IDENTICAL_CALL_BLOCK_THRESHOLD: u32 = 3;
const FAILURE_WARN_THRESHOLD: u32 = 3;
const FAILURE_HALT_THRESHOLD: u32 = 8;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AttemptDecision {
    Proceed,
    Block(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum OutcomeDecision {
    Continue,
    Warn(String),
    Halt(String),
}

#[derive(Debug, Default)]
pub struct LoopGuard {
    call_counts: HashMap<(String, u64), u32>,
    failure_counts: HashMap<String, u32>,
}

impl LoopGuard {
    pub fn record_attempt(&mut self, tool: &str, args: &Value) -> AttemptDecision {
        let key = (tool.to_string(), hash_args(args));
        let count = self.call_counts.entry(key).or_insert(0);
        *count = count.saturating_add(1);
        if *count >= IDENTICAL_CALL_BLOCK_THRESHOLD {
            return AttemptDecision::Block(format!(
                "Blocked: this exact call (`{tool}` with these arguments) has already run {count} times this turn. Stop retrying it unchanged. Either change the arguments or pick a different tool."
            ));
        }
        AttemptDecision::Proceed
    }

    pub fn record_outcome(&mut self, tool: &str, ok: bool) -> OutcomeDecision {
        let failures = self.failure_counts.entry(tool.to_string()).or_insert(0);
        if ok {
            *failures = 0;
            return OutcomeDecision::Continue;
        }

        *failures = failures.saturating_add(1);
        if *failures >= FAILURE_HALT_THRESHOLD {
            return OutcomeDecision::Halt(format!(
                "Stop retrying `{tool}` - it has failed {failures} consecutive times. Choose a different approach."
            ));
        }
        if *failures == FAILURE_WARN_THRESHOLD {
            return OutcomeDecision::Warn(format!(
                "Tool `{tool}` has failed {failures} consecutive times this turn."
            ));
        }
        OutcomeDecision::Continue
    }

    /// Clear consecutive-failure counters so a granted continuation (e.g. a
    /// long-horizon "change approach" nudge issued after a [`OutcomeDecision::Halt`])
    /// doesn't immediately re-halt on the same tool. Identical-call counts are
    /// left intact, so blindly repeating the *exact* same call is still blocked.
    pub fn reset_failures(&mut self) {
        self.failure_counts.clear();
    }
}

fn hash_args(args: &Value) -> u64 {
    let mut canonical = String::new();
    write_canonical_json(args, &mut canonical);
    let mut hasher = DefaultHasher::new();
    canonical.hash(&mut hasher);
    hasher.finish()
}

fn write_canonical_json(value: &Value, out: &mut String) {
    match value {
        Value::Null => out.push_str("null"),
        Value::Bool(value) => out.push_str(if *value { "true" } else { "false" }),
        Value::Number(value) => {
            let _ = write!(out, "{value}");
        }
        Value::String(value) => {
            out.push_str(&serde_json::to_string(value).expect("serializing string cannot fail"));
        }
        Value::Array(values) => {
            out.push('[');
            for (idx, item) in values.iter().enumerate() {
                if idx > 0 {
                    out.push(',');
                }
                write_canonical_json(item, out);
            }
            out.push(']');
        }
        Value::Object(values) => {
            out.push('{');
            let mut entries = values.iter().collect::<Vec<_>>();
            entries.sort_by(|a, b| a.0.cmp(b.0));
            for (idx, (key, item)) in entries.into_iter().enumerate() {
                if idx > 0 {
                    out.push(',');
                }
                out.push_str(&serde_json::to_string(key).expect("serializing key cannot fail"));
                out.push(':');
                write_canonical_json(item, out);
            }
            out.push('}');
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn third_identical_tool_call_is_blocked() {
        let mut guard = LoopGuard::default();
        let args = json!({"path": "src/main.rs"});

        assert_eq!(
            guard.record_attempt("read_file", &args),
            AttemptDecision::Proceed
        );
        assert_eq!(
            guard.record_attempt("read_file", &args),
            AttemptDecision::Proceed
        );

        let AttemptDecision::Block(message) = guard.record_attempt("read_file", &args) else {
            panic!("third identical call should be blocked");
        };
        assert!(message.contains("read_file"));
        assert!(message.contains("already run 3 times"));
    }

    #[test]
    fn paginated_reads_are_not_false_positives() {
        let mut guard = LoopGuard::default();

        for offset in [0, 100, 200] {
            assert_eq!(
                guard.record_attempt(
                    "read_file",
                    &json!({"path": "src/main.rs", "offset": offset})
                ),
                AttemptDecision::Proceed
            );
        }
    }

    #[test]
    fn tool_failure_counter_warns_at_three_and_halts_at_eight() {
        let mut guard = LoopGuard::default();

        assert_eq!(
            guard.record_outcome("grep_files", false),
            OutcomeDecision::Continue
        );
        assert_eq!(
            guard.record_outcome("grep_files", false),
            OutcomeDecision::Continue
        );
        assert!(matches!(
            guard.record_outcome("grep_files", false),
            OutcomeDecision::Warn(message) if message.contains("failed 3 consecutive times")
        ));

        for _ in 4..8 {
            assert_eq!(
                guard.record_outcome("grep_files", false),
                OutcomeDecision::Continue
            );
        }
        assert!(matches!(
            guard.record_outcome("grep_files", false),
            OutcomeDecision::Halt(message) if message.contains("failed 8 consecutive times")
        ));
    }

    #[test]
    fn successful_tool_call_resets_failure_counter() {
        let mut guard = LoopGuard::default();

        assert_eq!(
            guard.record_outcome("grep_files", false),
            OutcomeDecision::Continue
        );
        assert_eq!(
            guard.record_outcome("grep_files", false),
            OutcomeDecision::Continue
        );
        assert_eq!(
            guard.record_outcome("grep_files", true),
            OutcomeDecision::Continue
        );
        assert_eq!(
            guard.record_outcome("grep_files", false),
            OutcomeDecision::Continue
        );
    }

    #[test]
    fn reset_failures_clears_halt_so_a_continuation_does_not_immediately_rehalt() {
        let mut guard = LoopGuard::default();
        // Drive to the halt threshold (8 consecutive failures); intermediate
        // decisions include a Warn at 3, which we don't assert here.
        for _ in 0..7 {
            let _ = guard.record_outcome("apply_patch", false);
        }
        // Eighth consecutive failure halts.
        assert!(matches!(
            guard.record_outcome("apply_patch", false),
            OutcomeDecision::Halt(_)
        ));
        // A granted "change approach" continuation resets the counters …
        guard.reset_failures();
        // … so the next failure starts the count over instead of re-halting.
        assert_eq!(
            guard.record_outcome("apply_patch", false),
            OutcomeDecision::Continue
        );
    }

    #[test]
    fn reset_failures_leaves_identical_call_blocking_intact() {
        let mut guard = LoopGuard::default();
        let args = json!({"path": "src/main.rs"});
        assert_eq!(guard.record_attempt("read_file", &args), AttemptDecision::Proceed);
        assert_eq!(guard.record_attempt("read_file", &args), AttemptDecision::Proceed);
        guard.reset_failures();
        // Identical-call counter is independent of the failure counter, so the
        // third unchanged call is still blocked after a failure reset.
        assert!(matches!(
            guard.record_attempt("read_file", &args),
            AttemptDecision::Block(_)
        ));
    }

    #[test]
    fn argument_hash_is_independent_of_object_key_order() {
        let mut guard = LoopGuard::default();

        assert_eq!(
            guard.record_attempt("read_file", &json!({"path": "a", "offset": 0})),
            AttemptDecision::Proceed
        );
        assert_eq!(
            guard.record_attempt("read_file", &json!({"offset": 0, "path": "a"})),
            AttemptDecision::Proceed
        );
        assert!(matches!(
            guard.record_attempt("read_file", &json!({"path": "a", "offset": 0})),
            AttemptDecision::Block(_)
        ));
    }
}
