mod error;
mod games;
#[cfg(target_os = "macos")]
mod macos_quick_switch;
#[cfg(any(windows, target_os = "macos"))]
mod steam;
// The system tray drives the Steam switch flow and shows/hides the window;
// Windows-only like the steam module it depends on.
#[cfg(windows)]
mod tray;

/// Configure plugins shared by every platform.
///
/// Kept separate from command registration because `tauri::generate_handler!`
/// can't take a `#[cfg]` per item — the platform difference lives entirely in
/// [`run`] below, while plugins are identical across targets.
fn base_builder() -> tauri::Builder<tauri::Wry> {
    tauri::Builder::default()
        // Single instance, registered FIRST (per the plugin's docs) so a
        // second launch exits before any other plugin runs. The callback fires
        // in the FIRST instance: with close-to-tray, "opening the app again"
        // is how users reach a hidden window, so surface it. Two instances
        // must never race over loginusers.vdf / accounts.csv or show two trays.
        .plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| {
            use tauri::Manager;
            if let Some(window) = app.get_webview_window("main") {
                let _ = window.show();
                let _ = window.unminimize();
                let _ = window.set_focus();
            }
            #[cfg(target_os = "macos")]
            macos_quick_switch::refresh();
        }))
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
}

/// Build, configure, and run the Tauri application.
///
/// `tauri::generate_handler!` expands to a single closure and can't host a
/// `#[cfg]` on individual commands, so handlers are registered in
/// conditionally-compiled bodies. Windows receives Steam, Master Duel, and tray
/// commands. macOS receives Steam commands. Other targets receive only the
/// cross-platform supported-game query.
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
            steam::commands::steam_get_install_path,
            steam::commands::steam_list_accounts,
            steam::commands::steam_clear_login,
            steam::commands::steam_switch_account,
            steam::commands::steam_forget_account,
            steam::commands::steam_forget_accounts,
            steam::commands::steam_get_avatar,
            steam::commands::steam_is_running,
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
            tray::app_exit,
        ]);

    #[cfg(target_os = "macos")]
    let builder = base_builder()
        .setup(|app| {
            macos_quick_switch::setup(app.handle())?;
            Ok(())
        })
        .on_window_event(|window, event| match event {
            tauri::WindowEvent::Focused(true) => macos_quick_switch::refresh(),
            tauri::WindowEvent::CloseRequested { api, .. } => {
                // Keep the webview and native menu surfaces alive. The native
                // "Open steam-mate" action restores this same window.
                api.prevent_close();
                let _ = window.hide();
            }
            _ => {}
        })
        .invoke_handler(tauri::generate_handler![
            steam::commands::steam_get_install_path,
            steam::commands::steam_list_accounts,
            steam::commands::steam_clear_login,
            steam::commands::steam_switch_account,
            steam::commands::steam_forget_account,
            steam::commands::steam_forget_accounts,
            steam::commands::steam_get_avatar,
            steam::commands::steam_is_running,
            games::list_supported_games,
        ]);

    #[cfg(not(any(windows, target_os = "macos")))]
    let builder =
        base_builder().invoke_handler(tauri::generate_handler![games::list_supported_games,]);

    builder
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
