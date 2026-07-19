use std::sync::Arc;

use tauri::State;
use tokio::sync::RwLock;

use crate::core::state_machine::AppServices;

#[tauri::command]
pub async fn get_gui_log_tail(
    services: State<'_, Arc<RwLock<AppServices>>>,
) -> Result<String, String> {
    let svc = services.read().await;
    Ok(svc.logger.get_tail(50))
}

#[tauri::command]
pub async fn get_heartbeat_log(
    services: State<'_, Arc<RwLock<AppServices>>>,
) -> Result<String, String> {
    let svc = services.read().await;
    let path = &svc.heartbeat_log_path;
    if path.exists() {
        std::fs::read_to_string(path).map_err(|e| format!("Failed to read heartbeat log: {}", e))
    } else {
        Ok(String::new())
    }
}

#[tauri::command]
pub async fn open_log_file(services: State<'_, Arc<RwLock<AppServices>>>) -> Result<(), String> {
    let svc = services.read().await;
    svc.logger.open_log_file()
}

#[tauri::command]
pub async fn open_heartbeat_file(
    services: State<'_, Arc<RwLock<AppServices>>>,
    app_handle: tauri::AppHandle,
) -> Result<(), String> {
    let svc = services.read().await;
    let path = &svc.heartbeat_log_path;
    if path.exists() {
        use tauri_plugin_opener::OpenerExt;
        app_handle
            .opener()
            .open_path(path.to_string_lossy().to_string(), None::<&str>)
            .map_err(|e| format!("Failed to open heartbeat file: {e}"))
    } else {
        Ok(())
    }
}
