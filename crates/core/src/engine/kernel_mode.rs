//! `[kernel] machine` — shared between core turn loop and runtime config.

/// Resolved turn-machine mode (Phase 3b closure).
///
/// Configured via `[kernel] machine` in `config.toml` (default `v3`).
/// `"legacy"` is accepted for parse compatibility but maps to `V3` (removed in batch 5 closure).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum KernelMachineMode {
    /// Turn machine + effect interpreter control IO (default and only production mode).
    #[default]
    V3,
}

impl KernelMachineMode {
    #[must_use]
    pub fn parse(value: Option<&str>) -> Self {
        match value.map(str::trim).map(str::to_ascii_lowercase).as_deref() {
            Some("legacy") | Some("v3") | Some("shadow") | None => Self::V3,
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

    /// Whether `"shadow"` was supplied (deprecated; behaviour is v3 without shadow bake).
    #[must_use]
    pub fn config_used_deprecated_shadow(value: Option<&str>) -> bool {
        matches!(
            value.map(str::trim).map(str::to_ascii_lowercase).as_deref(),
            Some("shadow")
        )
    }

    #[must_use]
    pub fn as_str(self) -> &'static str {
        "v3"
    }

    /// Unified replay coherence + SQLite persist checks at turn end.
    #[must_use]
    pub fn uses_replay_verification(self) -> bool {
        matches!(self, Self::V3)
    }

    /// v3 effect-interpreter turn loop (production default).
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
            KernelMachineMode::V3
        );
        assert!(KernelMachineMode::config_used_deprecated_shadow(Some(
            "shadow"
        )));
        assert_eq!(KernelMachineMode::parse(Some("v3")), KernelMachineMode::V3);
        assert_eq!(
            KernelMachineMode::parse(Some("unknown")),
            KernelMachineMode::V3
        );
        assert!(KernelMachineMode::V3.uses_replay_verification());
        assert!(KernelMachineMode::V3.uses_v3_turn_loop());
    }
}
