use std::sync::Arc;

use tauri::State;
use tokio::sync::RwLock;

use crate::core::state_machine::AppServices;
use crate::services::config::AppConfig;

#[tauri::command]
pub async fn get_config(
    services: State<'_, Arc<RwLock<AppServices>>>,
) -> Result<AppConfig, String> {
    let svc = services.read().await;
    Ok(svc.config.get().clone())
}

#[tauri::command]
pub async fn set_config(
    services: State<'_, Arc<RwLock<AppServices>>>,
    config: AppConfig,
) -> Result<(), String> {
    let mut svc = services.write().await;
    svc.config.set(config);
    svc.log("Config updated via UI");
    Ok(())
}

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
