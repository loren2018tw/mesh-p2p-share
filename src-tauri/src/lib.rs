use std::sync::Mutex;
use tauri::State;

mod server;

struct AppState {
    shared_file: Mutex<Option<String>>,
}

#[tauri::command]
fn share_file(path: String, state: State<'_, AppState>) -> Result<(), String> {
    let mut file = state.shared_file.lock().map_err(|e| e.to_string())?;
    *file = Some(path.clone());
    println!("Shared file set to: {}", path);
    Ok(())
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_opener::init())
        .manage(AppState {
            shared_file: Mutex::new(None),
        })
        .invoke_handler(tauri::generate_handler![share_file])
        .setup(|_app| {
            tauri::async_runtime::spawn(async move {
                server::run_server().await;
            });
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
