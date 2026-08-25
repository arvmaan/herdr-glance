#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod commands;

use commands::AppState;
use tauri::Manager;
use tauri_plugin_window_state::StateFlags;

fn main() {
    let has_saved_config = herdr_glance_core::config_exists();
    let (config, configured, startup_warning) = match herdr_glance_core::load_config() {
        Ok(config) => (config, has_saved_config, None),
        Err(error) => (
            herdr_glance_core::ConnectionConfig::default(),
            false,
            Some(error.to_string()),
        ),
    };

    tauri::Builder::default()
        .plugin(
            tauri_plugin_window_state::Builder::default()
                .with_state_flags(StateFlags::POSITION)
                .build(),
        )
        .manage(AppState::new(config, configured, startup_warning))
        .setup(|app| {
            if let Some(window) = app.get_webview_window("main") {
                let _ = window.set_always_on_top(true);
                #[cfg(target_os = "macos")]
                let _ = window.set_visible_on_all_workspaces(true);
            }
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::get_connection,
            commands::save_connection,
            commands::list_agents,
            commands::focus_agent,
            commands::test_connection,
            commands::resize_window,
        ])
        .run(tauri::generate_context!())
        .expect("error running Herdr Glance");
}
