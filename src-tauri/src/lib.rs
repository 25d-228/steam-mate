mod error;
mod games;
mod steam;

// Learn more about Tauri commands at https://tauri.app/develop/calling-rust/
#[tauri::command]
fn greet(name: &str) -> String {
    format!("Hello, {}! You've been greeted from Rust!", name)
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .invoke_handler(tauri::generate_handler![
            greet,
            steam::commands::steam_get_install_path,
            steam::commands::steam_list_accounts,
            steam::commands::steam_clear_login,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
