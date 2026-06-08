//! Engine turn port (P2 PR3) — shell delegates turn start through core.
//!
//! The live `Engine` / `turn_loop` live in `zagens-core`; runtime-server
//! provides the host adapter. This trait is the stable boundary for `RuntimeThreadManager`.

use async_trait::async_trait;

use super::start_turn::StartTurnParams;

/// Minimal surface `RuntimeThreadManager` needs to drive a turn.
#[async_trait]
pub trait TurnEnginePort: Send + Sync {
    async fn start_turn(&self, params: StartTurnParams) -> anyhow::Result<()>;

    fn cancel_active_turn(&self);
}
