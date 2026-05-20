#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod commands;
mod export_path;
mod runtime_proxy;
mod sidecar;
mod terminal;
mod workspace_defaults;

use std::sync::Arc;

use tauri::{
    menu::{MenuBuilder, MenuItemBuilder},
    tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent},
    Manager, WindowEvent,
};
use tokio::sync::Notify;

fn main() {
    let shutdown = Arc::new(Notify::new());

    tauri::Builder::default()
        .plugin(tauri_plugin_shell::init())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_clipboard_manager::init())
        .plugin(tauri_plugin_notification::init())
        .plugin(tauri_plugin_os::init())
        .plugin(tauri_plugin_process::init())
        .on_window_event(|window, event| {
            if let WindowEvent::CloseRequested { api, .. } = event {
                api.prevent_close();
                let _ = window.hide();
            }
        })
        .setup(move |app| {
            let token = uuid::Uuid::new_v4().to_string();
            let sidecar_restart = Arc::new(Notify::new());
            app.manage(commands::AppContext {
                runtime_port: 7878,
                runtime_token: token.clone(),
                sidecar_restart: sidecar_restart.clone(),
                shutdown: shutdown.clone(),
            });
            app.manage(terminal::TerminalManager::default());

            // ── System tray ──
            let tray_image = app
                .default_window_icon()
                .cloned()
                .expect("no default icon configured in tauri.conf.json bundle.icon");

            let show_item = MenuItemBuilder::with_id("show", "显示 DS Pick").build(app)?;
            let quit_item = MenuItemBuilder::with_id("quit", "退出").build(app)?;

            let tray_menu = MenuBuilder::new(app)
                .item(&show_item)
                .separator()
                .item(&quit_item)
                .build()?;

            let _tray = TrayIconBuilder::new()
                .icon(tray_image)
                .tooltip("DS Pick")
                .menu(&tray_menu)
                .on_tray_icon_event(|tray: &tauri::tray::TrayIcon, event| {
                    if let TrayIconEvent::Click {
                        button: MouseButton::Left,
                        button_state: MouseButtonState::Up,
                        ..
                    } = event
                    {
                        if let Some(w) = tray.app_handle().get_webview_window("main") {
                            let _ = w.show();
                            let _ = w.set_focus();
                        }
                    }
                })
                .on_menu_event(|app: &tauri::AppHandle, event: tauri::menu::MenuEvent| match event.id().as_ref() {
                    "show" => {
                        if let Some(w) = app.get_webview_window("main") {
                            let _ = w.show();
                            let _ = w.set_focus();
                        }
                    }
                    "quit" => {
                        if let Some(ctx) = app.try_state::<commands::AppContext>() {
                            ctx.shutdown.notify_one();
                        }
                        std::thread::sleep(std::time::Duration::from_millis(300));
                        app.exit(0);
                    }
                    _ => {}
                })
                .build(app)?;

            // ── Sidecar ──
            let handle = app.handle().clone();
            let token_for_sidecar = token.clone();
            let shutdown_for_sidecar = shutdown.clone();
            tauri::async_runtime::spawn(async move {
                if let Err(e) = sidecar::start_and_monitor(
                    &handle,
                    7878,
                    &token_for_sidecar,
                    sidecar_restart,
                    shutdown_for_sidecar,
                )
                .await
                {
                    eprintln!("sidecar error: {e}");
                }
            });

            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::get_runtime_port,
            runtime_proxy::runtime_http,
            runtime_proxy::runtime_post_stream,
            runtime_proxy::runtime_get_sse,
            commands::get_platform_info,
            commands::get_os_theme,
            commands::get_locale,
            commands::get_api_key_status,
            commands::save_deepseek_api_key,
            commands::get_vision_bridge_status,
            commands::save_vision_bridge,
            commands::clear_vision_bridge,
            commands::vision_transcribe_image,
            commands::read_thread_workspace_binary,
            commands::read_workspace_binary_at_root,
            commands::open_in_shell,
            commands::open_with_system_app,
            commands::export_thread_json,
            commands::export_session_json,
            commands::restart_sidecar,
            commands::get_system_settings,
            commands::save_system_settings,
            commands::default_composer_workspace,
            commands::read_pick_rules,
            commands::save_pick_rules,
            commands::rebuild_symbol_index,
            commands::get_symbol_index_info,
            commands::delete_symbol_index,
            terminal::spawn_terminal,
            terminal::write_terminal,
            terminal::resize_terminal,
            terminal::kill_terminal,
        ])
        .run(tauri::generate_context!())
        .expect("error while running DS Pick");
}