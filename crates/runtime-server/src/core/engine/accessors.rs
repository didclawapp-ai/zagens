//! Downcast accessors for tui concrete handles on the core [`Engine`].

use super::runtime_ext::EngineRuntimeExt;
use super::types::EngineConfigExt;
use super::Engine;

impl Engine {
    pub(in crate::core::engine) fn runtime_ext(&self) -> &EngineRuntimeExt {
        self.ext
            .as_any()
            .downcast_ref()
            .expect("tui builder stores EngineRuntimeExt in Engine::ext")
    }

    pub(in crate::core::engine) fn runtime_ext_mut(&mut self) -> &mut EngineRuntimeExt {
        self.ext
            .as_any_mut()
            .downcast_mut()
            .expect("tui builder stores EngineRuntimeExt in Engine::ext")
    }

    pub(in crate::core::engine) fn config_ext(&self) -> &EngineConfigExt {
        &self.runtime_ext().config_ext
    }
}
