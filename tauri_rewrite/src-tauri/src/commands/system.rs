use std::sync::atomic::Ordering;
use std::sync::Arc;

use tauri::{AppHandle, Emitter, State};
use tokio::sync::RwLock;

use crate::core::state_machine::AppServices;

#[tauri::command]
pub async fn clear_all_cache(
    app: AppHandle,
    services: State<'_, Arc<RwLock<AppServices>>>,
) -> Result<(), String> {
    {
        let svc = services.read().await;
        svc.clear_volatile_caches().await;
        svc.loop_reset_requested.store(true, Ordering::Relaxed);
    }
    // Read lock is dropped before emitting so a concurrent backend re-init
    // (which needs the write lock) is never blocked behind the awaits above.
    // No sessionId is carried here: a manual cache-clear does not start a new
    // backend session, so the frontend keeps its existing epoch (re-armed only
    // when backend_ready/cache_cleared from try_initialize provides one).
    let _ = app.emit("cache_cleared", ());
    Ok(())
}

#[tauri::command]
pub async fn restart_application(
    services: State<'_, Arc<RwLock<AppServices>>>,
) -> Result<(), String> {
    let mut svc = services.write().await;
    svc.client.set_entitlements(None);
    svc.puuid = String::new();
    svc.content = Arc::new(crate::models::content::ContentCache::empty());
    svc.season_id = Arc::from("");
    svc.previous_season_id = None;
    svc.log("Backend state reset - reconnecting...");
    svc.restart_requested.store(true, Ordering::Relaxed);
    svc.restart_request.notify_one();
    drop(svc);
    Ok(())
}
