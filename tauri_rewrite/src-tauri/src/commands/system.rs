use std::sync::Arc;

use tauri::State;
use tokio::sync::RwLock;

use crate::core::state_machine::AppServices;

#[tauri::command]
pub async fn get_version() -> String {
    "2.99".to_string()
}

#[tauri::command]
pub async fn restart_application(
    services: State<'_, Arc<RwLock<AppServices>>>,
) -> Result<(), String> {
    let mut svc = services.write().await;
    svc.entitlements = None;
    svc.client_version = String::new();
    svc.puuid = String::new();
    svc.content = Arc::new(crate::models::content::ContentCache::empty());
    svc.season_id = String::new();
    svc.previous_season_id = None;
    svc.rank.invalidate_cache().await;
    svc.stats.clear_cache().await;
    svc.names.clear_cache().await;
    svc.log("Backend state reset — reconnecting...");
    drop(svc);
    Ok(())
}

#[tauri::command]
pub async fn get_status(
    services: State<'_, Arc<RwLock<AppServices>>>,
) -> Result<serde_json::Value, String> {
    let svc = services.read().await;
    Ok(serde_json::json!({
        "connected": svc.entitlements.is_some(),
        "puuid": svc.puuid,
        "season_id": svc.season_id,
    }))
}
