#![forbid(unsafe_code)]
#![deny(warnings)]
#![deny(clippy::all, clippy::cargo)]

mod api;
mod commands;
mod core;
mod models;
mod services;

use std::sync::Arc;

use log::{error, warn};
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
            commands::config::get_gui_log_tail,
            commands::config::get_heartbeat_log,
            commands::config::open_log_file,
            commands::config::open_heartbeat_file,
            commands::config::log_frontend,
            commands::system::clear_all_cache,
            commands::system::restart_application,
        ])
        .setup(|app| {
            let handle = app.handle().clone();
            // Capture the join handle so a panic or early return from the main
            // loop is surfaced in the logs instead of being silently discarded
            // (the frontend would otherwise show stale data forever).
            let main_task = tauri::async_runtime::spawn(async move { main_loop.run(handle).await });
            tauri::async_runtime::spawn(async move {
                // Surfaces a panic or cancellation of the main loop in the logs,
                // which the previous bare `spawn` silently discarded.
                // A finished-but-erroring loop would otherwise leave the frontend
                // showing stale data forever.
                if let Err(e) = main_task.await {
                    error!("main loop task failed (panic/cancelled): {e:?}");
                } else {
                    warn!("main loop task ended without error");
                }
            });
            Ok(())
        })
        .run(tauri::generate_context!())
        .unwrap_or_else(|e| {
            log::error!("error while running tauri application: {e}");
            std::process::exit(1);
        });
}
