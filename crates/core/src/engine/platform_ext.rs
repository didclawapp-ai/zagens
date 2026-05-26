//! Platform extension port for tui-specific op dispatch (M8).

use async_trait::async_trait;

use crate::engine::op::Op;
use crate::engine::runtime::Engine;

/// Tui/desktop hooks invoked from the core op loop.
#[async_trait]
pub trait EnginePlatformExt<P, R>: Send + Sync {
    fn as_any(&self) -> &dyn std::any::Any;
    fn as_any_mut(&mut self) -> &mut dyn std::any::Any;

    /// Dispatch an engine operation that requires platform-specific handling.
    async fn dispatch_op(&mut self, engine: &mut Engine<P, R>, op: Op);

    /// Shutdown hook after the op receiver closes (e.g. MCP pool teardown).
    async fn on_shutdown(&mut self, engine: &mut Engine<P, R>);
}
