use std::sync::Arc;

use tauri::{AppHandle, Emitter, State};
use tokio::sync::RwLock;

use crate::core::state_machine::AppServices;

#[tauri::command]
pub async fn clear_all_cache(
    app: AppHandle,
    services: State<'_, Arc<RwLock<AppServices>>>,
) -> Result<(), String> {
    let svc = services.read().await;
    svc.clear_volatile_caches().await;
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
    svc.restart_request.notify_one();
    drop(svc);
    Ok(())
}


