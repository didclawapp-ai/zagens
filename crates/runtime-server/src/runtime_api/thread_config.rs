//! `GET/PUT/DELETE /v1/threads/{id}/config` — per-session config overlay (C scheme).

use axum::Json;
use axum::extract::{Path, State};
use axum::http::StatusCode;

use zagens_runtime_orchestrator::runtime_threads::{ThreadConfigOverlay, ThreadConfigResponse};

use crate::config::{config_effective_view, resolve_effective_config, resolve_lht_composer_mode};

use super::{ApiError, RuntimeApiState, map_thread_err};

fn settings_composer_mode() -> Option<String> {
    zagens_config::read_lht_composer_mode_setting()
        .ok()
        .map(|m| match m {
            zagens_config::LhtComposerMode::Strict => "strict".to_string(),
            zagens_config::LhtComposerMode::Off => "off".to_string(),
            zagens_config::LhtComposerMode::Auto => "auto".to_string(),
        })
}

fn build_config_response(
    base: &crate::config::Config,
    thread: &zagens_runtime_orchestrator::runtime_threads::ThreadRecord,
) -> ThreadConfigResponse {
    let settings_composer = settings_composer_mode();
    // Global baseline view (overlay = None): UI compares against this to label fields
    // as inherited vs session-overridden.
    let base_composer = resolve_lht_composer_mode(None, settings_composer.as_deref());
    let base_view = config_effective_view(base, base_composer);

    let effective_cfg = resolve_effective_config(base, thread.config_overlay.as_ref());
    let composer =
        resolve_lht_composer_mode(thread.config_overlay.as_ref(), settings_composer.as_deref());
    ThreadConfigResponse {
        base: base_view,
        overlay: thread.config_overlay.clone(),
        effective: config_effective_view(&effective_cfg, composer),
    }
}

pub(crate) async fn get_thread_config(
    State(state): State<RuntimeApiState>,
    Path(thread_id): Path<String>,
) -> Result<Json<ThreadConfigResponse>, ApiError> {
    let thread = state
        .runtime_threads
        .load_thread_sync(&thread_id)
        .map_err(map_thread_err)?;
    Ok(Json(build_config_response(&state.config, &thread)))
}

pub(crate) async fn put_thread_config(
    State(state): State<RuntimeApiState>,
    Path(thread_id): Path<String>,
    Json(patch): Json<ThreadConfigOverlay>,
) -> Result<Json<ThreadConfigResponse>, ApiError> {
    let thread = state
        .runtime_threads
        .patch_thread_config_overlay(&thread_id, patch)
        .await
        .map_err(map_thread_err)?;
    Ok(Json(build_config_response(&state.config, &thread)))
}

pub(crate) async fn delete_thread_config_field(
    State(state): State<RuntimeApiState>,
    Path((thread_id, field)): Path<(String, String)>,
) -> Result<(StatusCode, Json<ThreadConfigResponse>), ApiError> {
    let thread = state
        .runtime_threads
        .clear_thread_config_field(&thread_id, &field)
        .await
        .map_err(map_thread_err)?;
    Ok((
        StatusCode::OK,
        Json(build_config_response(&state.config, &thread)),
    ))
}
