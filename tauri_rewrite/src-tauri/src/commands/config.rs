use std::io::{Read, Seek, SeekFrom};
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

fn tail_log(path: &std::path::Path, n: usize) -> Result<String, String> {
    let mut file =
        std::fs::File::open(path).map_err(|e| format!("Failed to open heartbeat log: {}", e))?;
    let file_size = file
        .metadata()
        .map_err(|e| format!("Failed to read metadata: {}", e))?
        .len();
    if file_size == 0 {
        return Ok(String::new());
    }
    const CHUNK_SIZE: u64 = 4096;
    let read_size = std::cmp::min(CHUNK_SIZE, file_size);
    file.seek(SeekFrom::End(-(read_size as i64)))
        .map_err(|e| format!("Failed to seek: {}", e))?;
    let mut buffer = vec![0u8; read_size as usize];
    file.read_exact(&mut buffer)
        .map_err(|e| format!("Failed to read: {}", e))?;

    let content = String::from_utf8_lossy(&buffer);
    let line_count = content.lines().count();

    if line_count > n || read_size >= file_size {
        let tail: Vec<&str> = content.lines().rev().take(n).collect();
        return Ok(tail.into_iter().rev().collect::<Vec<_>>().join("\n"));
    }

    let mut full = Vec::new();
    file.seek(SeekFrom::Start(0))
        .map_err(|e| format!("Failed to seek: {}", e))?;
    file.read_to_end(&mut full)
        .map_err(|e| format!("Failed to read: {}", e))?;
    let full_content = String::from_utf8_lossy(&full);
    let tail: Vec<&str> = full_content.lines().rev().take(n).collect();
    Ok(tail.into_iter().rev().collect::<Vec<_>>().join("\n"))
}

#[tauri::command]
pub async fn get_heartbeat_log(
    services: State<'_, Arc<RwLock<AppServices>>>,
) -> Result<String, String> {
    let svc = services.read().await;
    let path = &svc.heartbeat_log_path;
    if path.exists() {
        tail_log(path, 100)
    } else {
        Ok(String::new())
    }
}

#[tauri::command]
pub async fn log_frontend(
    services: State<'_, Arc<RwLock<AppServices>>>,
    msg: String,
    level: String,
) -> Result<(), String> {
    let svc = services.read().await;
    svc.log(&format!("[FRONTEND][{level}] {msg}"));
    Ok(())
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
