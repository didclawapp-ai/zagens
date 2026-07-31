//! H3 registry-surface adapter — stable API for stage-gated tool exposure.

use crate::long_horizon::stage_gate::{StageGateBlocked, StageGateSession};

/// Filter and gate tool exposure for harness stage contracts.
pub trait RegistrySurface {
    fn is_active(&self) -> bool;
    fn allowed_tool_names(&self) -> Option<Vec<String>>;
    fn check_tool(&self, tool_name: &str) -> Result<(), StageGateBlocked>;
    fn filter_tools<T>(&self, tools: Vec<T>, name: impl Fn(&T) -> &str) -> Vec<T>;
}

impl RegistrySurface for StageGateSession {
    fn is_active(&self) -> bool {
        StageGateSession::is_active(self)
    }

    fn allowed_tool_names(&self) -> Option<Vec<String>> {
        StageGateSession::allowed_tool_names(self)
    }

    fn check_tool(&self, tool_name: &str) -> Result<(), StageGateBlocked> {
        StageGateSession::check_tool(self, tool_name)
    }

    fn filter_tools<T>(&self, tools: Vec<T>, name: impl Fn(&T) -> &str) -> Vec<T> {
        StageGateSession::filter_tool_catalog(self, tools, name)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use zagens_core::long_horizon::HarnessContract;

    fn sample_contract() -> HarnessContract {
        let raw = include_str!("../../../../fixtures/harness/python-csv-skill-manifest.toml");
        HarnessContract::parse_toml(raw).expect("fixture manifest")
    }

    #[test]
    fn registry_surface_filters_tools_by_stage() {
        let mut session = StageGateSession::default();
        session.load_contract(sample_contract(), true);
        assert!(session.is_active());

        let tools = vec!["read_file", "write_file", "list_dir"];
        let filtered = session.filter_tools(tools, |n| n);
        assert!(filtered.contains(&"read_file"));
        assert!(filtered.contains(&"list_dir"));
        assert!(
            !filtered.contains(&"write_file"),
            "write_file blocked in inspect stage"
        );
    }

    #[test]
    fn python_pipeline_blocks_write_before_analyze() {
        let raw = include_str!("../../../../fixtures/harness/python-csv-skill-manifest.toml");
        let contract = HarnessContract::parse_toml(raw).expect("python fixture");
        let mut session = StageGateSession::default();
        session.load_contract(contract, true);

        assert!(session.check_tool("read_file").is_ok());
        assert!(session.check_tool("write_file").is_err());
        assert!(session.check_tool("exec_shell").is_err());
    }
}
