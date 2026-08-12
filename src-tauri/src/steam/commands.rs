//! Tauri commands for Steam (list_accounts, switch_account, ...).

use crate::error::{AppError, AppResult};
use crate::steam::account::SteamAccount;
use crate::steam::{account, avatar, platform, switch};

/// Whether the platform Steam client is currently running.
///
/// The frontend's "Signed in as" surfaces are only truthful while Steam is up,
/// so they go neutral when this returns false.
#[tauri::command]
pub async fn steam_is_running() -> AppResult<bool> {
    tauri::async_runtime::spawn_blocking(platform::is_steam_running)
        .await
        .map_err(|e| AppError::Io(e.to_string()))
}

/// Return Steam's install directory as a string.
///
/// Backed by the platform Steam application discovery. Errors with
/// `AppError::SteamNotInstalled` if Steam isn't installed.
#[tauri::command]
pub async fn steam_get_install_path() -> AppResult<String> {
    let path = platform::steam_install_dir()?;
    Ok(path.to_string_lossy().into_owned())
}

/// Return the list of remembered Steam accounts from `loginusers.vdf`.
///
/// Locates the file below the platform Steam data directory, reads it, and
/// hands the text to [`vdf::parse_loginusers`]. The platform layer then
/// reconciles current-account state where Steam's raw `MostRecent` flag is not
/// authoritative. A missing file is reported as
/// [`AppError::SteamNotInstalled`] rather than a raw IO error so the frontend
/// can branch on a single, named condition.
#[tauri::command]
pub async fn steam_list_accounts() -> AppResult<Vec<SteamAccount>> {
    account::list_accounts()
}

/// Blank Steam's "remembered auto-login user" so the next launch lands at the
/// login screen.
///
/// Thin wrapper over the platform auto-login store. Doesn't touch
/// `loginusers.vdf` or kill Steam — Steam picks up the change on its own
/// next start.
#[tauri::command]
pub async fn steam_clear_login() -> AppResult<()> {
    let result = platform::clear_auto_login_user();
    #[cfg(target_os = "macos")]
    crate::macos_quick_switch::refresh_after_account_mutation(false);
    result
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
    let result = match tauri::async_runtime::spawn_blocking(move || {
        switch::switch_account(&account_name, offline)
    })
    .await
    {
        Ok(result) => result,
        Err(error) => Err(AppError::Io(error.to_string())),
    };
    #[cfg(target_os = "macos")]
    crate::macos_quick_switch::refresh_after_account_mutation(true);
    result
}

/// Forget (delete) a remembered Steam account from `loginusers.vdf`.
///
/// Thin async wrapper that runs the blocking [`switch::forget_account`] on a
/// blocking thread. A join failure maps to [`AppError::Io`].
#[tauri::command]
pub async fn steam_forget_account(account_name: String) -> AppResult<()> {
    let result =
        match tauri::async_runtime::spawn_blocking(move || switch::forget_account(&account_name))
            .await
        {
            Ok(result) => result,
            Err(error) => Err(AppError::Io(error.to_string())),
        };
    #[cfg(target_os = "macos")]
    crate::macos_quick_switch::refresh_after_account_mutation(false);
    result
}

/// Forget several remembered Steam accounts in one pass, returning the count
/// actually removed.
///
/// Runs the blocking [`switch::forget_accounts`] on a blocking thread: Steam is
/// stopped once, `loginusers.vdf` is read once, each name is dropped from the
/// in-memory text (unknown names skipped), the file is written once, and the
/// auto-login registry keys are cleared if the removed set included the active
/// user. A join failure maps to [`AppError::Io`].
#[tauri::command]
pub async fn steam_forget_accounts(account_names: Vec<String>) -> AppResult<u32> {
    let result =
        match tauri::async_runtime::spawn_blocking(move || switch::forget_accounts(&account_names))
            .await
        {
            Ok(result) => result,
            Err(error) => Err(AppError::Io(error.to_string())),
        };
    #[cfg(target_os = "macos")]
    crate::macos_quick_switch::refresh_after_account_mutation(false);
    result
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
