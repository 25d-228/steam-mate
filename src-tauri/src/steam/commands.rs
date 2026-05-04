//! Tauri commands for Steam (list_accounts, switch_account, ...).

use crate::error::AppResult;
use crate::steam::paths;

/// Return Steam's install directory as a string.
///
/// Backed by `paths::get_steam_install_path`. Errors with
/// `AppError::SteamNotInstalled` if Steam isn't installed.
#[tauri::command]
pub async fn steam_get_install_path() -> AppResult<String> {
    let path = paths::get_steam_install_path()?;
    Ok(path.to_string_lossy().into_owned())
}
