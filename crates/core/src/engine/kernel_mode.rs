//! `[kernel] machine` kill switch — shared between core turn loop and runtime config.

/// Resolved turn-machine mode (Phase 3b).
///
/// Configured via `[kernel] machine` in `config.toml` (default `v3`).
/// `"legacy"` is accepted for parse compatibility but maps to `V3` (removed in batch 5 closure).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum KernelMachineMode {
    /// v3 turn loop + extra effect/guard/memory shadow bake at turn end.
    Shadow,
    /// Turn machine + effect interpreter control IO (default).
    #[default]
    V3,
}

impl KernelMachineMode {
    #[must_use]
    pub fn parse(value: Option<&str>) -> Self {
        match value.map(str::trim).map(str::to_ascii_lowercase).as_deref() {
            Some("legacy") | Some("v3") | None => Self::V3,
            Some("shadow") => Self::Shadow,
            Some(_) => Self::V3,
        }
    }

    /// Whether the config string `"legacy"` was supplied (deprecated; behaviour is v3).
    #[must_use]
    pub fn config_used_deprecated_legacy(value: Option<&str>) -> bool {
        matches!(
            value.map(str::trim).map(str::to_ascii_lowercase).as_deref(),
            Some("legacy")
        )
    }

    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
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

    /// v3 effect-interpreter turn loop (production default and shadow bake).
    #[must_use]
    pub fn uses_v3_turn_loop(self) -> bool {
        matches!(self, Self::V3 | Self::Shadow)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_kernel_machine_mode() {
        assert_eq!(KernelMachineMode::parse(None), KernelMachineMode::V3);
        assert_eq!(KernelMachineMode::default(), KernelMachineMode::V3);
        assert_eq!(
            KernelMachineMode::parse(Some("legacy")),
            KernelMachineMode::V3
        );
        assert!(KernelMachineMode::config_used_deprecated_legacy(Some(
            "legacy"
        )));
        assert_eq!(
            KernelMachineMode::parse(Some("SHADOW")),
            KernelMachineMode::Shadow
        );
        assert_eq!(KernelMachineMode::parse(Some("v3")), KernelMachineMode::V3);
        assert_eq!(
            KernelMachineMode::parse(Some("unknown")),
            KernelMachineMode::V3
        );
        assert!(KernelMachineMode::Shadow.uses_replay_verification());
        assert!(KernelMachineMode::V3.uses_replay_verification());
        assert!(KernelMachineMode::V3.uses_v3_turn_loop());
        assert!(KernelMachineMode::Shadow.uses_v3_turn_loop());
    }
}
