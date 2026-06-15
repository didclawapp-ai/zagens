//! TopicMemory kernel event double-write (Phase 3b batch 4 / 8g).

use zagens_core::engine::kernel_event::KernelEvent;
use zagens_core::engine::token_estimate::estimate_text_tokens;
use zagens_core::engine::turn_machine::emit_kernel_event;

use super::Engine;

impl Engine {
    /// Record episodic topic-memory injection in the kernel log when a block is composed.
    pub(in crate::core::engine) fn record_topic_memory_injected(&mut self, block: &str) {
        let ext = self.runtime_ext();
        let Some(turn_id) = ext.kernel_active_turn_id.clone() else {
            return;
        };
        let step_idx = ext.kernel_active_step;
        let block_token_est = estimate_text_tokens(block) as u32;
        emit_kernel_event(
            self,
            KernelEvent::TopicMemoryInjected {
                turn_id,
                step_idx,
                block_token_est,
            },
        );
    }
}
