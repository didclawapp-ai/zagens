#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod commands;
mod sidecar;

use tauri::Manager;

fn main() {
    tauri::Builder::default()
        .plugin(tauri_plugin_shell::init())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_clipboard_manager::init())
        .plugin(tauri_plugin_notification::init())
        .plugin(tauri_plugin_os::init())
        .plugin(tauri_plugin_process::init())
        .setup(|app| {
            let token = uuid::Uuid::new_v4().to_string();
            app.manage(commands::AppContext {
                runtime_port: 7878,
                runtime_token: token.clone(),
            });

            let handle = app.handle().clone();
            let token_for_sidecar = token.clone();
            tauri::async_runtime::spawn(async move {
                if let Err(e) = sidecar::start_and_monitor(&handle, 7878, &token_for_sidecar).await {
                    eprintln!("sidecar error: {e}");
                }
            });

            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::get_runtime_token,
            commands::get_runtime_port,
            commands::get_platform_info,
            commands::get_os_theme,
            commands::get_locale,
        ])
        .run(tauri::generate_context!())
        .expect("error while running DeepSeek Desktop");
}
