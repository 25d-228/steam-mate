//! Tauri commands for Steam (list_accounts, switch_account, ...).

use std::fs;

use crate::error::{AppError, AppResult};
use crate::steam::account::SteamAccount;
use crate::steam::{avatar, paths, registry, switch, vdf};

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

/// Switch the active Steam account to `account_name`, optionally launching offline.
///
/// Runs the blocking [`switch::switch_account`] orchestration (kill Steam,
/// rewrite registry + `loginusers.vdf`, relaunch) on a blocking thread so the
/// async runtime isn't stalled. `offline_mode` defaults to `false`. A panic /
/// cancellation of the blocking task surfaces as [`AppError::Io`].
#[tauri::command]
pub async fn steam_switch_account(
    account_name: String,
    offline_mode: Option<bool>,
) -> AppResult<()> {
    let offline = offline_mode.unwrap_or(false);
    tauri::async_runtime::spawn_blocking(move || switch::switch_account(&account_name, offline))
        .await
        .map_err(|e| AppError::Io(e.to_string()))?
}

/// Forget (delete) a remembered Steam account from `loginusers.vdf`.
///
/// Thin async wrapper that runs the blocking [`switch::forget_account`] on a
/// blocking thread. A join failure maps to [`AppError::Io`].
#[tauri::command]
pub async fn steam_forget_account(account_name: String) -> AppResult<()> {
    tauri::async_runtime::spawn_blocking(move || switch::forget_account(&account_name))
        .await
        .map_err(|e| AppError::Io(e.to_string()))?
}

/// Return a Steam account's avatar as a `data:image/jpeg;base64,...` URI, or
/// `None` if unavailable.
///
/// Runs the blocking, network-touching [`avatar::get_avatar`] (HTTP + disk
/// cache) on a blocking thread. Avatar fetches never error in normal use — a
/// missing or unfetchable avatar is `Ok(None)`; only a join failure maps to
/// [`AppError::Io`].
#[tauri::command]
pub async fn steam_get_avatar(steam_id64: String) -> AppResult<Option<String>> {
    tauri::async_runtime::spawn_blocking(move || avatar::get_avatar(&steam_id64))
        .await
        .map_err(|e| AppError::Io(e.to_string()))?
}
