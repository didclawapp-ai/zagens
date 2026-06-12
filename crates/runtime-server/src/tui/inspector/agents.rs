//! Sub-agent list from cached engine events.

#[derive(Debug, Clone, Default)]
pub struct AgentEntry {
    pub id: String,
    pub status: String,
}

pub fn render_panel(agents: &[AgentEntry], height: usize) -> Vec<String> {
    if agents.is_empty() {
        return vec!["(no subagents this turn)".to_string()];
    }
    let mut lines: Vec<String> = agents
        .iter()
        .map(|a| format!("{}  {}", a.id, a.status))
        .collect();
    if lines.len() > height.max(4) {
        let skip = lines.len() - height.max(4);
        lines.drain(0..skip);
    }
    lines
}
