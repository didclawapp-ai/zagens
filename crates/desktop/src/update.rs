//! In-app update checks via `tauri-plugin-updater` (manifest at zagens.com/download/latest.json).

use serde::Serialize;
use tauri::{AppHandle, Emitter};
use tauri_plugin_updater::UpdaterExt;

pub const DOWNLOAD_PAGE_URL: &str = "https://zagens.com/download";

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateStatusResponse {
    /// Updater plugin and pubkey are configured; check may still fail on network/manifest errors.
    pub ready: bool,
    pub current_version: String,
    /// `not_configured` | `up_to_date` | `available` | `error`
    pub status: String,
    pub available_version: Option<String>,
    pub notes: Option<String>,
    pub download_page_url: String,
    pub error: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateProgressPayload {
    pub downloaded: usize,
    pub total: Option<u64>,
}

fn base_response(current_version: String) -> UpdateStatusResponse {
    UpdateStatusResponse {
        ready: false,
        current_version,
        status: "not_configured".to_string(),
        available_version: None,
        notes: None,
        download_page_url: DOWNLOAD_PAGE_URL.to_string(),
        error: None,
    }
}

#[tauri::command]
pub async fn get_update_status(app: AppHandle) -> UpdateStatusResponse {
    let current_version = app.package_info().version.to_string();
    let mut out = base_response(current_version);

    let updater = match app.updater() {
        Ok(u) => u,
        Err(e) => {
            out.error = Some(format!("updater 未配置: {e}"));
            return out;
        }
    };

    out.ready = true;

    match updater.check().await {
        Ok(Some(update)) => {
            out.status = "available".to_string();
            out.available_version = Some(update.version);
            out.notes = update.body;
        }
        Ok(None) => {
            out.status = "up_to_date".to_string();
        }
        Err(e) => {
            out.status = "error".to_string();
            out.error = Some(humanize_update_error(&e.to_string()));
        }
    }

    out
}

#[tauri::command]
pub async fn install_app_update(app: AppHandle) -> Result<(), String> {
    let updater = app.updater().map_err(|e| format!("updater 未配置: {e}"))?;
    let Some(update) = updater
        .check()
        .await
        .map_err(|e| humanize_update_error(&e.to_string()))?
    else {
        return Err("当前已是最新版本".to_string());
    };

    let mut downloaded = 0usize;
    update
        .download_and_install(
            |chunk_length, content_length| {
                downloaded = downloaded.saturating_add(chunk_length);
                let _ = app.emit(
                    "zagens://app-update-progress",
                    UpdateProgressPayload {
                        downloaded,
                        total: content_length,
                    },
                );
            },
            || {},
        )
        .await
        .map_err(|e| humanize_update_error(&e.to_string()))?;

    Ok(())
}

fn humanize_update_error(raw: &str) -> String {
    let lower = raw.to_lowercase();
    if lower.contains("signature") || lower.contains("minisign") {
        return format!(
            "{raw}（更新包签名无效或 latest.json 中 signature 为空；请确认发布流程已用 TAURI_SIGNING_PRIVATE_KEY 签名）"
        );
    }
    if lower.contains("url") && (lower.contains("empty") || lower.contains("invalid")) {
        return format!(
            "{raw}（latest.json 中该平台 download URL 无效；请运行 website sync:manifest 并部署安装包）"
        );
    }
    raw.to_string()
}
