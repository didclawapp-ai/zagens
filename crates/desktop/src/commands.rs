use serde::Serialize;

#[derive(Clone)]
pub struct AppContext {
    pub runtime_port: u16,
    pub runtime_token: String,
}

#[derive(Debug, Serialize)]
pub struct PlatformInfo {
    pub os: String,
    pub arch: String,
    pub version: String,
}

#[tauri::command]
pub async fn get_runtime_token(ctx: tauri::State<'_, AppContext>) -> Result<String, String> {
    Ok(ctx.runtime_token.clone())
}

#[tauri::command]
pub async fn get_runtime_port(ctx: tauri::State<'_, AppContext>) -> Result<u16, String> {
    Ok(ctx.runtime_port)
}

#[tauri::command]
pub async fn get_platform_info() -> Result<PlatformInfo, String> {
    Ok(PlatformInfo {
        os: std::env::consts::OS.to_string(),
        arch: std::env::consts::ARCH.to_string(),
        version: env!("CARGO_PKG_VERSION").to_string(),
    })
}

#[tauri::command]
pub async fn get_os_theme() -> Result<String, String> {
    Ok("dark".to_string())
}

#[tauri::command]
pub async fn get_locale() -> Result<String, String> {
    Ok("zh-CN".to_string())
}
