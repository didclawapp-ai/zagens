//! Compiler source tracking for v3 memory-plane queries (Phase 3b batch 4 / 8g).

use super::runtime_ext::EngineRuntimeExt;

/// Reset per-step compiler source markers before pre-`CallModel` queries run.
pub(in crate::core::engine) fn clear_memory_query_compiler_sources(ext: &mut EngineRuntimeExt) {
    ext.kernel_memory_query_sources.clear();
}

/// Record a compiler source satisfied by a memory query when projection material is present.
pub(in crate::core::engine) fn record_memory_query_compiler_source(
    ext: &mut EngineRuntimeExt,
    compiler_source: &str,
    material_present: bool,
) {
    if material_present {
        ext.kernel_memory_query_sources
            .insert(compiler_source.to_string());
    }
}
