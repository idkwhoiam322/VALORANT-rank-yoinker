use std::sync::Arc;

use tauri::{AppHandle, Emitter, State};
use tokio::sync::RwLock;

use crate::core::state_machine::AppServices;

#[tauri::command]
pub async fn clear_all_cache(
    app: AppHandle,
    services: State<'_, Arc<RwLock<AppServices>>>,
) -> Result<(), String> {
    let svc = services.write().await;
    svc.rank.invalidate_cache().await;
    svc.stats.clear_cache().await;
    svc.names.clear_cache().await;
    svc.clear_match_player_cache();
    drop(svc);
    let _ = app.emit("cache_cleared", ());
    Ok(())
}

#[tauri::command]
pub async fn restart_application(
    services: State<'_, Arc<RwLock<AppServices>>>,
) -> Result<(), String> {
    let mut svc = services.write().await;
    *svc.entitlements.lock().unwrap() = None;
    svc.client_version = String::new();
    svc.puuid = String::new();
    svc.content = Arc::new(crate::models::content::ContentCache::empty());
    svc.season_id = Arc::from("");
    svc.previous_season_id = None;
    svc.log("Backend state reset - reconnecting...");
    svc.auth_retry.notify_one();
    drop(svc);
    Ok(())
}


