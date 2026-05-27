//! `supersedes` transitive closure for coverage stats.

use std::collections::HashSet;

use crate::scratchpad::schema::NoteLine;

/// Ids excluded from P2 reports: every `supersedes` target, plus transitive closure.
#[must_use]
pub fn compute_superseded_ids(notes: &[NoteLine]) -> HashSet<String> {
    let mut edges: Vec<(String, String)> = Vec::new();
    for note in notes {
        if let Some(target) = note.supersedes.as_ref().filter(|t| !t.is_empty()) {
            edges.push((note.id.clone(), target.clone()));
        }
    }

    let mut superseded: HashSet<String> = edges.iter().map(|(_, old)| old.clone()).collect();
    let mut changed = true;
    while changed {
        changed = false;
        for (new_id, old_id) in &edges {
            if superseded.contains(new_id) && superseded.insert(old_id.clone()) {
                changed = true;
            }
        }
    }
    superseded
}
