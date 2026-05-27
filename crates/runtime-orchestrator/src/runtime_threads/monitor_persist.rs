//! Blocking persist helpers for the turn monitor (D16 E1-b phase 3).

use anyhow::{Result, anyhow};

use super::manager::RuntimeThreadManager;
use super::types::TurnItemRecord;

impl<P, R> RuntimeThreadManager<P, R>
where
    P: Send + Sync + Clone + 'static,
    R: Send + Sync + Clone + 'static,
{
    pub(crate) async fn save_item_and_attach_blocking(
        &self,
        item: &TurnItemRecord,
        turn_id: &str,
    ) -> Result<()> {
        let store = self.store.clone();
        let item = item.clone();
        let turn_id = turn_id.to_string();
        tokio::task::spawn_blocking(move || -> Result<()> {
            store.save_item(&item)?;
            let mut turn = store.load_turn(&turn_id)?;
            if !turn.item_ids.iter().any(|id| id == &item.id) {
                turn.item_ids.push(item.id.clone());
                store.save_turn(&turn)?;
            }
            Ok(())
        })
        .await
        .map_err(|e| anyhow!("save_item_and_attach panicked: {e}"))?
    }

    pub(crate) async fn update_and_save_item_blocking(
        &self,
        item_id: &str,
        update_fn: impl FnOnce(&mut TurnItemRecord),
    ) -> Result<TurnItemRecord> {
        let store = self.store.clone();
        let item_id = item_id.to_string();
        let mut item = tokio::task::spawn_blocking(move || store.load_item(&item_id))
            .await
            .map_err(|e| anyhow!("load_item panicked: {e}"))??;
        update_fn(&mut item);
        let store = self.store.clone();
        let item_clone = item.clone();
        tokio::task::spawn_blocking(move || store.save_item(&item_clone))
            .await
            .map_err(|e| anyhow!("save_item panicked: {e}"))??;
        Ok(item)
    }
}
