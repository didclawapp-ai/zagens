//! v3 memory-plane read queries — routes [`Effect::QueryMemory`] through the interpreter.

use zagens_core::engine::turn_loop::memory_plane_query_policy::MemoryPlaneQueryLayer;

use super::*;

impl Engine {
    /// Resolve a symbolic memory-plane query (v3 effect plan; IO stub until compiler/topic wiring).
    pub(in crate::core::engine) async fn run_query_memory_effect(
        &mut self,
        layer: MemoryPlaneQueryLayer,
        query_key: &str,
    ) {
        if self.effect_replay_anchor_only() {
            tracing::info!(
                target: "kernel_v3",
                layer = layer.as_str(),
                query_key,
                "replay anchor-only: skipping QueryMemory IO"
            );
            return;
        }
        if !self.runtime_ext().kernel_machine_mode.uses_v3_turn_loop() {
            return;
        }
        tracing::info!(
            target: "kernel_v3",
            turn_id = ?self.runtime_ext().kernel_active_turn_id,
            step = self.runtime_ext().kernel_active_step,
            layer = layer.as_str(),
            query_key,
            "v3 memory-plane: QueryMemory (effect plan)"
        );
    }
}
