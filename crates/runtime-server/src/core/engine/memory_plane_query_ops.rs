//! v3 memory-plane read queries — routes [`Effect::QueryMemory`] through the interpreter.

use zagens_core::engine::kernel_event::KernelEvent;
use zagens_core::engine::turn_loop::memory_plane_compiler_policy::{
    compiler_source_for_query_key, query_key_has_projection_material,
};
use zagens_core::engine::turn_loop::memory_plane_query_policy::MemoryPlaneQueryLayer;
use zagens_core::engine::turn_machine::{TurnKernelProjection, emit_kernel_event};

use super::*;

impl Engine {
    /// Resolve a symbolic memory-plane query (v3 effect plan; compiler source mapping + event double-write).
    pub(in crate::core::engine) async fn run_query_memory_effect(
        &mut self,
        layer: MemoryPlaneQueryLayer,
        query_key: &str,
    ) {
        let ext = self.runtime_ext();
        let turn_id = ext
            .kernel_active_turn_id
            .clone()
            .unwrap_or_else(|| "effect-interpreter".to_string());
        let step_idx = ext.kernel_active_step;
        let compiler_source = compiler_source_for_query_key(query_key);
        let projection =
            TurnKernelProjection::from_events(self.runtime_ext().kernel_turn_events.turn_events());
        let material_present = query_key_has_projection_material(&projection, query_key);

        if self.effect_replay_anchor_only() {
            tracing::info!(
                target: "kernel_v3",
                layer = layer.as_str(),
                query_key,
                compiler_source,
                material_present,
                "replay anchor-only: skipping QueryMemory IO"
            );
            return;
        }
        if !self.runtime_ext().kernel_machine_mode.uses_v3_turn_loop() {
            return;
        }
        tracing::info!(
            target: "kernel_v3",
            turn_id = ?turn_id,
            step = step_idx,
            layer = layer.as_str(),
            query_key,
            compiler_source,
            material_present,
            "v3 memory-plane: QueryMemory (compiler source mapped)"
        );
        emit_kernel_event(
            self,
            KernelEvent::MemoryPlaneQueried {
                turn_id,
                step_idx,
                layer: layer.as_str().to_string(),
                query_key: query_key.to_string(),
                compiler_source: compiler_source.to_string(),
            },
        );
    }
}
