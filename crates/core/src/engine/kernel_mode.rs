//! `[kernel] machine` kill switch — shared between core turn loop and runtime config.

/// Resolved turn-machine mode (Phase 3b).
///
/// Configured via `[kernel] machine` in `config.toml` (default `legacy`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum KernelMachineMode {
    /// Existing turn loop controls IO (default).
    #[default]
    Legacy,
    /// Event log drives [`ReplayTurnMachine`](super::turn_machine::ReplayTurnMachine) sanity checks.
    Shadow,
    /// Turn machine + effect interpreter control IO (partial — see `turn_loop::v3_driver`).
    V3,
}

impl KernelMachineMode {
    #[must_use]
    pub fn parse(value: Option<&str>) -> Self {
        match value.map(str::trim).map(str::to_ascii_lowercase).as_deref() {
            Some("shadow") => Self::Shadow,
            Some("v3") => Self::V3,
            _ => Self::Legacy,
        }
    }

    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Legacy => "legacy",
            Self::Shadow => "shadow",
            Self::V3 => "v3",
        }
    }

    #[must_use]
    pub fn uses_effect_replay_shadow(self) -> bool {
        matches!(self, Self::Shadow)
    }

    /// Unified replay coherence + SQLite persist checks (shadow bake and v3 observability).
    #[must_use]
    pub fn uses_replay_verification(self) -> bool {
        matches!(self, Self::Shadow | Self::V3)
    }

    #[must_use]
    pub fn uses_v3_turn_loop(self) -> bool {
        matches!(self, Self::V3)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_kernel_machine_mode() {
        assert_eq!(KernelMachineMode::parse(None), KernelMachineMode::Legacy);
        assert_eq!(
            KernelMachineMode::parse(Some("SHADOW")),
            KernelMachineMode::Shadow
        );
        assert_eq!(KernelMachineMode::parse(Some("v3")), KernelMachineMode::V3);
        assert_eq!(
            KernelMachineMode::parse(Some("unknown")),
            KernelMachineMode::Legacy
        );
        assert!(KernelMachineMode::Shadow.uses_replay_verification());
        assert!(KernelMachineMode::V3.uses_replay_verification());
        assert!(!KernelMachineMode::Legacy.uses_replay_verification());
    }
}
