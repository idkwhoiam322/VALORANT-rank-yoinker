mod api;
mod commands;
mod core;
mod models;
mod services;

use std::sync::Arc;
use tokio::sync::RwLock;

use crate::core::state_machine::MainLoop;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let _ = env_logger::try_init();

    let root = std::env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(|p| p.to_path_buf()))
        .unwrap_or_else(|| std::path::PathBuf::from("."));

    let pd_url = "https://pd.na.a.pvp.net".to_string();
    let glz_url = "https://glz-na-1.na.a.pvp.net".to_string();

    let main_loop = MainLoop::new(root.clone(), pd_url, glz_url);
    let services: Arc<RwLock<crate::core::state_machine::AppServices>> = main_loop.services.clone();

    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .manage(services)
        .invoke_handler(tauri::generate_handler![
            commands::config::get_config,
            commands::config::set_config,
            commands::config::get_gui_log_tail,
            commands::system::get_version,
            commands::system::restart_application,
            commands::system::get_status,
        ])
        .setup(|app| {
            let handle = app.handle().clone();
            tauri::async_runtime::spawn(async move {
                main_loop.run(handle).await;
            });
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
