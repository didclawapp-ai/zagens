//! `[kernel] machine` — shared between core turn loop and runtime config.

/// Resolved turn-machine mode (Phase 3b closure).
///
/// `[kernel] machine` in `config.toml` is accepted for forward compatibility but
/// has no effect — `V3` is the only turn machine at runtime. Any configured value
/// (including the historical `"legacy"` / `"shadow"`) maps to `V3`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum KernelMachineMode {
    /// Turn machine + effect interpreter control IO (default and only production mode).
    #[default]
    V3,
}

impl KernelMachineMode {
    /// Resolve the configured `[kernel] machine` value. Always `V3`; the argument is
    /// accepted only so call sites can pass through the raw config string unchanged.
    #[must_use]
    pub fn parse(_value: Option<&str>) -> Self {
        Self::V3
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
        // Historical / unknown values all collapse to V3 (forward-compat parse).
        for value in ["legacy", "SHADOW", "shadow", "v3", "unknown"] {
            assert_eq!(KernelMachineMode::parse(Some(value)), KernelMachineMode::V3);
        }
        assert_eq!(KernelMachineMode::V3.as_str(), "v3");
        assert!(KernelMachineMode::V3.uses_replay_verification());
        assert!(KernelMachineMode::V3.uses_v3_turn_loop());
    }
}
