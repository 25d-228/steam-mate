//! Tauri commands for Steam (list_accounts, switch_account, ...).

use std::fs;

use crate::error::{AppError, AppResult};
use crate::steam::account::SteamAccount;
use crate::steam::{paths, registry, vdf};

/// Return Steam's install directory as a string.
///
/// Backed by `paths::get_steam_install_path`. Errors with
/// `AppError::SteamNotInstalled` if Steam isn't installed.
#[tauri::command]
pub async fn steam_get_install_path() -> AppResult<String> {
    let path = paths::get_steam_install_path()?;
    Ok(path.to_string_lossy().into_owned())
}

/// Return the list of remembered Steam accounts from `loginusers.vdf`.
///
/// Locates the file via [`paths::loginusers_vdf_path`], reads it, and
/// hands the text to [`vdf::parse_loginusers`]. A missing file is
/// reported as [`AppError::SteamNotInstalled`] rather than a raw IO
/// error so the frontend can branch on a single, named condition.
#[tauri::command]
pub async fn steam_list_accounts() -> AppResult<Vec<SteamAccount>> {
    let path = paths::loginusers_vdf_path()?;
    let text = fs::read_to_string(&path)
        .map_err(|_| AppError::SteamNotInstalled)?;
    vdf::parse_loginusers(&text)
}

/// Blank Steam's "remembered auto-login user" so the next launch lands at the
/// login screen.
///
/// Thin wrapper over [`registry::clear_auto_login_user`]. Doesn't touch
/// `loginusers.vdf` or kill Steam — Steam picks up the change on its own
/// next start.
#[tauri::command]
pub async fn steam_clear_login() -> AppResult<()> {
    registry::clear_auto_login_user()
}
