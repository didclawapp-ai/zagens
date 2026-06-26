//! Per-thread config overlay CRUD (C scheme).

use anyhow::{Context, Result, anyhow, bail};
use chrono::Utc;
use serde_json::json;

use super::ThreadConfigOverlay;
use super::manager::RuntimeThreadManager;
use super::types::ThreadRecord;

impl<P, R> RuntimeThreadManager<P, R>
where
    P: Send + Sync + Clone + 'static,
    R: Send + Sync + Clone + 'static,
{
    /// Apply a partial overlay patch and schedule engine refresh (next turn / idle unload).
    pub async fn patch_thread_config_overlay(
        &self,
        thread_id: &str,
        patch: ThreadConfigOverlay,
    ) -> Result<ThreadRecord> {
        let mut thread = self
            .store
            .load_thread(thread_id)
            .with_context(|| format!("load thread {thread_id}"))?;
        let mut overlay = thread.config_overlay.take().unwrap_or_default();
        overlay.merge_from(patch);
        thread.config_overlay = if overlay.is_empty() {
            None
        } else {
            Some(overlay)
        };
        thread.updated_at = Utc::now();
        {
            let store = self.store.clone();
            let thread_clone = thread.clone();
            tokio::task::spawn_blocking(move || store.save_thread(&thread_clone))
                .await
                .map_err(|e| anyhow!("save thread panicked: {e}"))??;
        }
        self.schedule_config_engine_refresh(thread_id).await?;
        self.emit_event(
            thread_id,
            None,
            None,
            "thread.config_updated",
            json!({ "thread_id": thread_id }),
        )
        .await?;
        Ok(thread)
    }

    /// Remove one top-level overlay field; inherit global base for that section.
    pub async fn clear_thread_config_field(
        &self,
        thread_id: &str,
        field: &str,
    ) -> Result<ThreadRecord> {
        let mut thread = self
            .store
            .load_thread(thread_id)
            .with_context(|| format!("load thread {thread_id}"))?;
        let mut overlay = thread.config_overlay.take().unwrap_or_default();
        if !overlay.clear_field(field) {
            bail!("unknown config overlay field: {field}");
        }
        thread.config_overlay = if overlay.is_empty() {
            None
        } else {
            Some(overlay)
        };
        thread.updated_at = Utc::now();
        {
            let store = self.store.clone();
            let thread_clone = thread.clone();
            tokio::task::spawn_blocking(move || store.save_thread(&thread_clone))
                .await
                .map_err(|e| anyhow!("save thread panicked: {e}"))??;
        }
        self.schedule_config_engine_refresh(thread_id).await?;
        self.emit_event(
            thread_id,
            None,
            None,
            "thread.config_updated",
            json!({ "thread_id": thread_id, "cleared": field }),
        )
        .await?;
        Ok(thread)
    }

    async fn schedule_config_engine_refresh(&self, thread_id: &str) -> Result<()> {
        match self.unload_idle_thread_engine(thread_id).await {
            Ok(()) => Ok(()),
            Err(err) => {
                let msg = err.to_string();
                if msg.contains("active turn") {
                    let mut active = self.active.lock().await;
                    if let Some(state) = active.engines.get_mut(thread_id) {
                        state.pending_config_refresh = true;
                    }
                    Ok(())
                } else {
                    Err(err)
                }
            }
        }
    }
}
