use serde::Serialize;
use std::collections::HashMap;
use std::sync::Arc;
use tauri::State;
use tokio::sync::{mpsc, Mutex as TokioMutex, RwLock};

mod p2p;
mod server;

/// 給前端用的檔案摘要資訊（不含敏感路徑）
#[derive(Debug, Clone, Serialize)]
pub struct FileListItem {
    pub file_id: String,
    pub file_name: String,
    pub total_size: u64,
    pub chunk_count: u32,
}

/// 各端點的 WebSocket 發送通道 (key = endpoint_id)
pub type WsSenders = Arc<RwLock<HashMap<String, mpsc::UnboundedSender<server::ServerMessage>>>>;

pub struct AppState {
    pub service_url: Arc<TokioMutex<Option<String>>>,
    pub p2p_state: p2p::SharedState,
    pub ws_senders: WsSenders,
}

/// 廣播檔案清單給所有已連線的下載端
async fn broadcast_file_list(p2p_state: &p2p::SharedState, ws_senders: &WsSenders) {
    let files = {
        let s = p2p_state.read().await;
        s.shared_files
            .iter()
            .map(|f| server::FileListEntry {
                file_id: f.file_id.clone(),
                file_name: f.file_name.clone(),
                total_size: f.total_size,
                chunk_count: f.chunk_count,
            })
            .collect()
    };
    let msg = server::ServerMessage::FileList { files };
    let senders = ws_senders.read().await;
    for tx in senders.values() {
        let _ = tx.send(msg.clone());
    }
}

#[tauri::command]
async fn share_file(path: String, state: State<'_, AppState>) -> Result<FileListItem, String> {
    let info = p2p::add_shared_file(&state.p2p_state, &path).await?;
    let result = FileListItem {
        file_id: info.file_id,
        file_name: info.file_name,
        total_size: info.total_size,
        chunk_count: info.chunk_count,
    };
    // 廣播更新後的檔案清單給所有下載端
    broadcast_file_list(&state.p2p_state, &state.ws_senders).await;
    Ok(result)
}

#[tauri::command]
async fn remove_shared_file(path: String, state: State<'_, AppState>) -> Result<(), String> {
    p2p::remove_shared_file(&state.p2p_state, &path).await;
    // 廣播更新後的檔案清單給所有下載端
    broadcast_file_list(&state.p2p_state, &state.ws_senders).await;
    Ok(())
}

#[tauri::command]
async fn get_app_version() -> Result<String, String> {
    Ok(env!("CARGO_PKG_VERSION").to_string())
}

#[tauri::command]
async fn get_service_url(state: State<'_, AppState>) -> Result<String, String> {
    let url = state.service_url.lock().await;
    match url.clone() {
        Some(u) => Ok(u),
        None => Err("Service is not ready yet".into()),
    }
}

#[tauri::command]
fn get_file_size(path: String) -> Result<u64, String> {
    std::fs::metadata(path)
        .map(|metadata| metadata.len())
        .map_err(|e| e.to_string())
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let host_id = uuid::Uuid::new_v4().to_string();
    let p2p_state = Arc::new(RwLock::new(p2p::P2PState::new(host_id)));

    let ws_senders: WsSenders = Arc::new(RwLock::new(HashMap::new()));

    let app_state = AppState {
        service_url: Arc::new(TokioMutex::new(None)),
        p2p_state: p2p_state.clone(),
        ws_senders: ws_senders.clone(),
    };
    let url_state = Arc::clone(&app_state.service_url);

    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_opener::init())
        .manage(app_state)
        .invoke_handler(tauri::generate_handler![
            share_file,
            remove_shared_file,
            get_service_url,
            get_file_size,
            get_app_version
        ])
        .setup(move |app| {
            let app_handle = app.handle().clone();
            let p2p_clone = p2p_state.clone();
            let ws_senders_clone = ws_senders.clone();
            tauri::async_runtime::spawn(async move {
                server::run_server(app_handle, url_state, p2p_clone, ws_senders_clone).await;
            });
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
