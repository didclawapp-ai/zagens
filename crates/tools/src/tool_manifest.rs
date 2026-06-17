//! Tool capability manifests (kernel-v2 M2).
//!
//! Static footprint declarations replace string heuristics in M3+ policy
//! code. Built-in tools derive conservative manifests from existing
//! `ToolCapability` metadata until each family is refined.

use serde::{Deserialize, Serialize};

use crate::ToolCapability;

/// Where a footprint declaration came from (proposal §8.1.2).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FootprintProvenance {
    /// Built-in tool: code is the declaration.
    BuiltIn,
    /// User explicitly trusted via config / allow flow.
    UserConfig,
    /// MCP server self-declaration — untrusted; may only tighten policy.
    McpSelfDeclared,
}

/// Process / privilege class for tools that spawn subprocesses.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SpawnClass {
    #[default]
    None,
    Sandboxed,
    Privileged,
}

/// Resource access summary (M2 minimal form; M3+ may split Fs/Net/Proc).
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct ResourceSet {
    pub workspace_read: bool,
    pub network_read: bool,
    pub workspace_write: bool,
    pub network_write: bool,
}

impl ResourceSet {
    /// True when this set declares no write-side effects (proposal §8.1).
    #[must_use]
    pub fn is_empty(&self) -> bool {
        !self.workspace_write && !self.network_write
    }
}

/// Declared resource footprint for scheduling / policy.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Footprint {
    pub reads: ResourceSet,
    pub writes: ResourceSet,
    pub spawns: SpawnClass,
}

impl Footprint {
    /// Whether the tool may mutate durable workspace state.
    #[must_use]
    pub fn writes_workspace_state(&self) -> bool {
        self.writes.workspace_write
    }
}

/// Per-tool manifest consumed by PolicyEngine (M3) and DAG scheduler (M4).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ToolManifest {
    pub name: String,
    pub footprint: Footprint,
    pub provenance: FootprintProvenance,
}

impl ToolManifest {
    /// Conservative manifest from legacy capability metadata + M1 union predicate.
    ///
    /// `unified_writes_state` should be `zagens_core::engine::tool_writes_state(name)`
    /// for built-in tools so loop-guard and policy stay aligned until footprints
    /// are hand-refined per family.
    #[must_use]
    pub fn derive_conservative(
        name: &str,
        caps: &[ToolCapability],
        unified_writes_state: bool,
        provenance: FootprintProvenance,
    ) -> Self {
        Self {
            name: name.to_string(),
            footprint: derive_conservative_footprint(caps, unified_writes_state),
            provenance,
        }
    }
}

/// Derive a conservative footprint from capability flags and the M1 union bit.
#[must_use]
pub fn derive_conservative_footprint(
    caps: &[ToolCapability],
    unified_writes_state: bool,
) -> Footprint {
    let read_only = caps.contains(&ToolCapability::ReadOnly);
    let writes_files = caps.contains(&ToolCapability::WritesFiles);
    let executes = caps.contains(&ToolCapability::ExecutesCode);
    let network = caps.contains(&ToolCapability::Network);

    let writes = ResourceSet {
        workspace_write: unified_writes_state || writes_files || executes,
        network_write: network && !read_only,
        ..Default::default()
    };

    let reads = ResourceSet {
        workspace_read: read_only || writes_files || executes || !network,
        network_read: network,
        ..Default::default()
    };

    let spawns = if executes {
        SpawnClass::Sandboxed
    } else {
        SpawnClass::None
    };

    Footprint {
        reads,
        writes,
        spawns,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn read_only_file_tool_has_empty_writes() {
        let fp = derive_conservative_footprint(&[ToolCapability::ReadOnly], false);
        assert!(fp.writes.is_empty());
        assert!(fp.reads.workspace_read);
        assert!(!fp.writes_workspace_state());
    }

    #[test]
    fn unified_writes_state_union_covers_exec_shell() {
        let caps = vec![ToolCapability::ExecutesCode, ToolCapability::Sandboxable];
        let without = derive_conservative_footprint(&caps, false);
        assert!(without.writes_workspace_state());
        let with_union = derive_conservative_footprint(&caps, true);
        assert!(with_union.writes_workspace_state());
        assert_eq!(with_union.spawns, SpawnClass::Sandboxed);
    }

    #[test]
    fn write_file_footprint_is_workspace_write() {
        let fp = derive_conservative_footprint(&[ToolCapability::WritesFiles], true);
        assert!(fp.writes.workspace_write);
        assert!(!fp.writes.is_empty());
    }

    #[test]
    fn mcp_network_tool_defaults_network_write_when_not_read_only() {
        let fp = derive_conservative_footprint(
            &[ToolCapability::Network, ToolCapability::RequiresApproval],
            false,
        );
        assert!(fp.reads.network_read);
        assert!(fp.writes.network_write);
    }
}
