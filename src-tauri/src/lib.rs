mod error;
mod games;
// The Steam module reads the Windows registry (winreg) unconditionally, so it
// only compiles on Windows. Gate it here rather than advertise a non-Windows
// build that cannot link.
#[cfg(windows)]
mod steam;
// The system tray drives the Steam switch flow and shows/hides the window;
// Windows-only like the steam module it depends on.
#[cfg(windows)]
mod tray;

// Learn more about Tauri commands at https://tauri.app/develop/calling-rust/
#[tauri::command]
fn greet(name: &str) -> String {
    format!("Hello, {}! You've been greeted from Rust!", name)
}

/// Configure plugins shared by every platform.
///
/// Kept separate from command registration because `tauri::generate_handler!`
/// can't take a `#[cfg]` per item — the platform difference lives entirely in
/// [`run`] below, while plugins are identical across targets.
fn base_builder() -> tauri::Builder<tauri::Wry> {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
}

/// Build, configure, and run the Tauri application.
///
/// `tauri::generate_handler!` expands to a single closure and can't host a
/// `#[cfg]` on individual commands, so the handler is registered in one of two
/// conditionally-compiled bodies. The Steam and Master Duel commands depend on
/// Windows-only registry/junction APIs (`winreg`/`junction`), so the Windows
/// body registers them and the non-Windows body omits them; only the
/// cross-platform `list_supported_games` (which returns an empty list off
/// Windows) plus `greet` are shared via [`base_builder`].
#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    #[cfg(windows)]
    let builder = base_builder()
        // Build the system tray once setup runs, then hide-to-tray on close so
        // the resident tray keeps the quick-switch menu alive; only the tray's
        // "Exit" item quits the app.
        .setup(|app| {
            tray::build(app.handle())?;
            Ok(())
        })
        .on_window_event(|window, event| {
            if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                // Don't quit: hide to the tray instead.
                api.prevent_close();
                let _ = window.hide();
            }
        })
        .invoke_handler(tauri::generate_handler![
            greet,
            steam::commands::steam_get_install_path,
            steam::commands::steam_list_accounts,
            steam::commands::steam_clear_login,
            steam::commands::steam_switch_account,
            steam::commands::steam_forget_account,
            steam::commands::steam_forget_accounts,
            steam::commands::steam_get_avatar,
            games::list_supported_games,
            games::master_duel::commands::md_list_accounts,
            games::master_duel::commands::md_link_account,
            games::master_duel::commands::md_unlink_account,
            games::master_duel::commands::md_save_metadata,
            games::master_duel::commands::md_assign_steam,
            games::master_duel::commands::md_link_all,
            games::master_duel::commands::md_unlink_all,
            games::master_duel::commands::md_delete_account,
            games::master_duel::commands::md_delete_accounts,
            games::master_duel::commands::md_is_running,
            games::master_duel::commands::md_export_accounts,
            games::master_duel::commands::md_export_to_file,
            games::master_duel::commands::md_cache_size,
            games::master_duel::commands::md_cache_exists,
            games::master_duel::commands::md_seed_candidates,
            games::master_duel::commands::md_create_cache,
            games::master_duel::commands::md_reveal_cache,
            games::master_duel::commands::md_install_path,
            tray::tray_refresh,
        ]);

    #[cfg(not(windows))]
    let builder = base_builder().invoke_handler(tauri::generate_handler![
        greet,
        games::list_supported_games,
    ]);

    builder
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
