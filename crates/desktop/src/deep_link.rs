//! Desktop handling for `zagens://open?...` deep links.

use tauri::{AppHandle, Emitter, Manager};
use zagens_config::DeepLinkOpen;

use crate::window_registry;

pub const OPEN_REQUEST_EVENT: &str = "zagens://open-request";

pub async fn handle_open_deep_link(app: &AppHandle, link: DeepLinkOpen) -> Result<String, String> {
    let ws = link.workspace_display();
    let label = window_registry::open_or_focus_workspace(app, Some(ws)).await?;
    let registry = app.state::<window_registry::WindowRegistry>();
    let _ = registry.stash_pending_deep_link(&label, link.clone());
    emit_open_request(app, &label, &link);
    window_registry::focus_window(app, &label)?;
    Ok(label)
}

pub fn emit_open_request(app: &AppHandle, label: &str, link: &DeepLinkOpen) {
    let payload = deep_link_payload(link);
    let _ = app.emit_to(label, OPEN_REQUEST_EVENT, payload);
}

#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DeepLinkOpenPayload {
    pub workspace: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub prompt: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub task_type: Option<String>,
}

pub fn deep_link_payload(link: &DeepLinkOpen) -> DeepLinkOpenPayload {
    DeepLinkOpenPayload {
        workspace: link.workspace_display(),
        prompt: link.prompt.clone(),
        task_type: link.task_type.clone(),
    }
}

#[tauri::command]
pub fn take_pending_deep_link(
    window: tauri::WebviewWindow,
    registry: tauri::State<'_, window_registry::WindowRegistry>,
) -> Option<DeepLinkOpenPayload> {
    registry
        .take_pending_deep_link(window.label())
        .map(|link| deep_link_payload(&link))
}
