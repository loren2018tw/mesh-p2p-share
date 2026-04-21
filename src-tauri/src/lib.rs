use std::sync::{Arc, Mutex};
use tauri::State;

mod server;

struct AppState {
    shared_file: Mutex<Option<String>>,
    service_url: Arc<Mutex<Option<String>>>,
}

#[tauri::command]
fn share_file(path: String, state: State<'_, AppState>) -> Result<(), String> {
    let mut file = state.shared_file.lock().map_err(|e| e.to_string())?;
    *file = Some(path.clone());
    println!("Shared file set to: {}", path);
    Ok(())
}

#[tauri::command]
fn get_service_url(state: State<'_, AppState>) -> Result<String, String> {
    let url = state.service_url.lock().map_err(|e| e.to_string())?;
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
    let app_state = AppState {
        shared_file: Mutex::new(None),
        service_url: Arc::new(Mutex::new(None)),
    };
    let url_state = Arc::clone(&app_state.service_url);

    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_opener::init())
        .manage(app_state)
        .invoke_handler(tauri::generate_handler![
            share_file,
            get_service_url,
            get_file_size
        ])
        .setup(move |_app| {
            tauri::async_runtime::spawn(async move {
                server::run_server(url_state).await;
            });
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
