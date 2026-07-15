#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod browser;
mod commands;
mod custom_providers;
mod deep_link;
mod disk_guard;
mod export_path;
mod model_providers;
mod runtime_proxy;
mod sidecar;
mod terminal;
mod update;
mod window_registry;
mod workspace_defaults;

use std::sync::Arc;
use std::sync::atomic::AtomicBool;

use tauri::{
    Manager, WindowEvent,
    menu::{MenuBuilder, MenuItemBuilder},
    tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent},
};
use tauri_plugin_deep_link::DeepLinkExt;
use tokio::sync::Notify;
use window_registry::WindowRegistry;
use zagens_config::ConfigStore;

fn focus_last_or_main(app: &tauri::AppHandle) {
    let registry = app.state::<WindowRegistry>();
    let label = registry.last_focused_label();
    if window_registry::focus_window(app, &label).is_err()
        && let Some(w) = app.get_webview_window("main")
    {
        let _ = w.show();
        let _ = w.set_focus();
    }
}

fn build_tray_menu(app: &tauri::AppHandle) -> Result<tauri::menu::Menu<tauri::Wry>, String> {
    let show_item = MenuItemBuilder::with_id("show", "显示 Zagens")
        .build(app)
        .map_err(|e| e.to_string())?;
    let new_window_item = MenuItemBuilder::with_id("new_window", "新建窗口")
        .build(app)
        .map_err(|e| e.to_string())?;
    let quit_item = MenuItemBuilder::with_id("quit", "退出")
        .build(app)
        .map_err(|e| e.to_string())?;
    MenuBuilder::new(app)
        .item(&show_item)
        .item(&new_window_item)
        .separator()
        .item(&quit_item)
        .build()
        .map_err(|e| e.to_string())
}

fn main() {
    let shutdown = Arc::new(Notify::new());

    let mut builder = tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_clipboard_manager::init())
        .plugin(tauri_plugin_notification::init())
        .plugin(tauri_plugin_os::init())
        .plugin(tauri_plugin_process::init())
        .plugin(tauri_plugin_updater::Builder::new().build())
        .plugin(tauri_plugin_deep_link::init())
        .manage(WindowRegistry::new())
        .manage(browser::BrowserHosts::new())
        .manage(browser::BrowserBridgeUrl::default());

    builder = builder.plugin(tauri_plugin_single_instance::init(|app, argv, _cwd| {
        let app = app.clone();
        tauri::async_runtime::spawn(async move {
            if let Some(link) =
                zagens_config::find_open_url_in_args(argv.iter().map(String::as_str))
            {
                let _ = deep_link::handle_open_deep_link(&app, link).await;
                return;
            }
            let ws = window_registry::parse_workspace_from_args(&argv);
            let _ = window_registry::open_or_focus_workspace(&app, ws).await;
        });
    }));

    builder
        .on_window_event(|window, event| {
            let app = window.app_handle();
            let registry = app.state::<WindowRegistry>();
            let terminal = app.state::<terminal::TerminalManager>();

            match event {
                WindowEvent::Focused(true) => {
                    registry.set_last_focused(window.label());
                }
                WindowEvent::CloseRequested { api, .. } => {
                    let label = window.label().to_string();
                    if label.starts_with("browser-") {
                        // Browser pane host window — not an agent workbench window.
                        return;
                    }
                    let browser_hosts = app.state::<browser::BrowserHosts>();
                    browser::destroy_for_parent(&app, &browser_hosts, &label);
                    if let Some(wv) = app.get_webview_window(&label) {
                        window_registry::handle_close_requested(&wv, api, &registry, &terminal);
                    }
                }
                WindowEvent::Destroyed => {
                    let label = window.label().to_string();
                    if label.starts_with("browser-") {
                        // If the windowed BrowserHost was closed by the user, drop the record.
                        let browser_hosts = app.state::<browser::BrowserHosts>();
                        browser_hosts.forget_host_label(&label);
                        return;
                    }
                    let browser_hosts = app.state::<browser::BrowserHosts>();
                    browser::destroy_for_parent(&app, &browser_hosts, &label);
                    registry.unregister(&label);
                    terminal.kill_all_for_window(&label);
                }
                _ => {}
            }
        })
        .setup(move |app| {
            if let Err(e) = ConfigStore::ensure_default_on_disk(None) {
                eprintln!("[zagens] warning: failed to create default config.toml: {e}");
            }

            let token = uuid::Uuid::new_v4().to_string();
            let sidecar_restart = Arc::new(Notify::new());
            let sidecar_restart_force = Arc::new(AtomicBool::new(false));
            // Watch channel publishes the runtime port. Initial value `0` means "not ready yet";
            // the sidecar supervisor writes the real bound port after parsing `DS_PICK_READY`.
            // IPC handlers either `await` (get_runtime_port) or fast-fail (require_port).
            let (runtime_port_tx, runtime_port_rx) = tokio::sync::watch::channel::<u16>(0);
            app.manage(commands::AppContext {
                runtime_port: runtime_port_rx,
                runtime_token: token.clone(),
                sidecar_restart: sidecar_restart.clone(),
                sidecar_restart_force: sidecar_restart_force.clone(),
                shutdown: shutdown.clone(),
                probe_cache: Arc::new(std::sync::RwLock::new(std::collections::HashMap::new())),
            });
            app.manage(terminal::TerminalManager::default());

            let handle = app.handle().clone();
            let token_for_sidecar = token.clone();
            let shutdown_for_sidecar = shutdown.clone();
            // Start browser bridge then sidecar so ZAGENS_BROWSER_BRIDGE_URL is set before spawn.
            tauri::async_runtime::spawn(async move {
                match browser::start_browser_bridge(handle.clone(), token_for_sidecar.clone()).await
                {
                    Ok(url) => {
                        if let Some(slot) = handle.try_state::<browser::BrowserBridgeUrl>() {
                            slot.set(url);
                        }
                    }
                    Err(e) => {
                        eprintln!("[zagens] browser bridge failed to start: {e}");
                    }
                }
                if let Err(e) = sidecar::start_and_monitor(
                    &handle,
                    7878,
                    runtime_port_tx,
                    &token_for_sidecar,
                    sidecar_restart.clone(),
                    sidecar_restart_force,
                    shutdown_for_sidecar,
                )
                .await
                {
                    eprintln!("sidecar error: {e}");
                }
            });

            let registry = app.state::<WindowRegistry>();
            let default_ws =
                workspace_defaults::default_composer_workspace().unwrap_or_else(|_| String::new());
            let _ = registry.register("main", &default_ws);
            registry.set_last_focused("main");
            if let Some(main) = app.get_webview_window("main") {
                let title = window_registry::window_title_for_workspace(&default_ws);
                let _ = main.set_title(&title);
            }

            let startup_args: Vec<String> = std::env::args().collect();
            if let Some(link) =
                zagens_config::find_open_url_in_args(startup_args.iter().map(String::as_str))
            {
                let app_handle = app.handle().clone();
                tauri::async_runtime::spawn(async move {
                    let _ = deep_link::handle_open_deep_link(&app_handle, link).await;
                });
            }

            let handle_for_deep_link = app.handle().clone();
            app.deep_link().on_open_url(move |event| {
                for url in event.urls() {
                    if let Ok(link) = zagens_config::parse_open_url(url.as_ref()) {
                        let app = handle_for_deep_link.clone();
                        tauri::async_runtime::spawn(async move {
                            let _ = deep_link::handle_open_deep_link(&app, link).await;
                        });
                    }
                }
            });

            let tray_image = app
                .default_window_icon()
                .cloned()
                .expect("no default icon configured in tauri.conf.json bundle.icon");

            let tray_menu = build_tray_menu(app.handle())?;

            let _tray = TrayIconBuilder::new()
                .icon(tray_image)
                .tooltip("Zagens")
                .menu(&tray_menu)
                .on_tray_icon_event(|tray: &tauri::tray::TrayIcon, event| {
                    if let TrayIconEvent::Click {
                        button: MouseButton::Left,
                        button_state: MouseButtonState::Up,
                        ..
                    } = event
                    {
                        focus_last_or_main(tray.app_handle());
                    }
                })
                .on_menu_event(
                    |app: &tauri::AppHandle, event: tauri::menu::MenuEvent| match event
                        .id()
                        .as_ref()
                    {
                        "show" => focus_last_or_main(app),
                        "new_window" => {
                            let app = app.clone();
                            tauri::async_runtime::spawn(async move {
                                let _ = window_registry::create_agent_window_impl(&app, None).await;
                            });
                        }
                        "quit" => {
                            if let Some(ctx) = app.try_state::<commands::AppContext>() {
                                ctx.shutdown.notify_one();
                            }
                            std::thread::sleep(std::time::Duration::from_millis(300));
                            app.exit(0);
                        }
                        id if id.starts_with("focus:") => {
                            let label = id.trim_start_matches("focus:");
                            let _ =
                                window_registry::focus_agent_window(app.clone(), label.to_string());
                        }
                        _ => {}
                    },
                )
                .build(app)?;

            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::get_runtime_port,
            runtime_proxy::runtime_http,
            runtime_proxy::runtime_post_stream,
            runtime_proxy::runtime_get_sse,
            runtime_proxy::runtime_cancel_sse,
            commands::get_platform_info,
            commands::get_windows_sandbox_status,
            commands::get_sandbox_platforms_overview,
            commands::get_sandbox_onboarding_state,
            commands::get_sandbox_settings,
            commands::save_sandbox_settings,
            commands::initialize_windows_sandbox,
            commands::get_os_theme,
            commands::get_locale,
            commands::set_app_locale,
            commands::get_lht_composer_mode,
            commands::set_lht_composer_mode,
            commands::get_lht_strict,
            commands::set_lht_strict,
            commands::get_desktop_shell_prefs,
            commands::save_desktop_shell_prefs,
            commands::get_api_key_status,
            commands::save_deepseek_api_key,
            commands::clear_deepseek_api_key,
            commands::add_custom_model_provider,
            commands::rename_custom_model_provider,
            commands::get_model_providers_status,
            commands::save_model_provider_credentials,
            commands::clear_model_provider_credentials,
            commands::activate_model_provider,
            commands::probe_model_provider,
            commands::list_catalog_models,
            commands::set_catalog_model,
            commands::list_openrouter_models,
            commands::set_openrouter_model,
            commands::list_sensenova_models,
            commands::set_sensenova_model,
            commands::list_nvidia_nim_models,
            commands::set_nvidia_nim_model,
            commands::list_agnes_models,
            commands::set_agnes_model,
            commands::get_vision_bridge_status,
            commands::save_vision_bridge,
            commands::clear_vision_bridge,
            commands::vision_transcribe_image,
            commands::read_thread_workspace_binary,
            commands::read_workspace_binary_at_root,
            commands::open_in_shell,
            commands::open_with_system_app,
            commands::open_external_url,
            commands::export_thread_json,
            commands::export_thread_trace_report,
            commands::export_thread_trace_compare,
            commands::export_session_json,
            commands::restart_sidecar,
            commands::force_sidecar_restart_now,
            commands::preview_lht_preset,
            commands::get_system_settings,
            commands::save_system_settings,
            commands::get_lht_settings,
            commands::save_lht_settings,
            commands::apply_lht_preset,
            commands::get_hooks_settings,
            commands::save_hooks_settings,
            commands::default_composer_workspace,
            commands::read_pick_rules,
            commands::save_pick_rules,
            commands::rebuild_symbol_index,
            commands::get_symbol_index_info,
            commands::delete_symbol_index,
            commands::get_storage_pressure,
            terminal::spawn_terminal,
            terminal::write_terminal,
            terminal::resize_terminal,
            terminal::kill_terminal,
            window_registry::get_window_label,
            window_registry::get_window_workspace,
            window_registry::create_agent_window,
            window_registry::list_agent_windows,
            window_registry::focus_agent_window,
            window_registry::register_window_thread,
            window_registry::thread_owned_by_window,
            window_registry::close_current_window,
            deep_link::take_pending_deep_link,
            update::get_update_status,
            update::install_app_update,
            browser::browser_create,
            browser::browser_destroy,
            browser::browser_navigate,
            browser::browser_get_state,
            browser::browser_set_bounds,
            browser::browser_snapshot,
            browser::browser_focus_content,
        ])
        .run(tauri::generate_context!())
        .expect("error while running Zagens");
}
