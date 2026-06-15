//! Kernel log-driven resume — restore turn frame on engine load (Phase 3b 6e).

use zagens_core::engine::KernelTurnHost;
use zagens_core::engine::turn_machine::KernelResumeHints;

use super::Engine;

impl Engine {
    pub(in crate::core::engine) fn apply_kernel_resume_hints(&mut self, hints: &KernelResumeHints) {
        KernelTurnHost::apply_kernel_resume_hints(self, hints);
    }
}
