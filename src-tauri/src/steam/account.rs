//! Remembered Steam-account loading and the struct returned to the frontend.

use std::fs;

use serde::Serialize;

use super::{platform, vdf};
use crate::error::{AppError, AppResult};

/// One remembered Steam user, parsed from `loginusers.vdf`.
///
/// Field names are sent as camelCase in JSON (matches the TS
/// `SteamAccount` interface in DEV.md) via `#[serde(rename_all)]`. The platform
/// layer may reconcile `most_recent` with live session state before returning
/// the account to the frontend.
#[derive(Clone, Serialize, Debug)]
#[serde(rename_all = "camelCase")]
pub struct SteamAccount {
    pub account_name: String,
    pub persona_name: String,
    pub steam_id64: String,
    pub steam_id32: u32,
    pub remember_password: bool,
    pub most_recent: bool,
    pub wants_offline_mode: bool,
    pub skip_offline_mode_warning: bool,
    pub allow_auto_login: bool,
    pub timestamp: u64,
}

fn read_accounts() -> AppResult<Vec<SteamAccount>> {
    let path = platform::steam_data_dir()?
        .join("config")
        .join("loginusers.vdf");
    let text = fs::read_to_string(&path).map_err(|_| AppError::SteamNotInstalled)?;
    vdf::parse_loginusers(&text)
}

/// Load remembered accounts and reconcile their current-session marker.
///
/// This is shared by the command API and native macOS quick-switch surfaces,
/// so every account surface reads the same VDF and platform session state.
pub fn list_accounts() -> AppResult<Vec<SteamAccount>> {
    let mut accounts = read_accounts()?;
    platform::resolve_current_account(&mut accounts);
    Ok(accounts)
}

/// Load remembered accounts using an already-observed macOS running state.
///
/// Native menus need their account marker and running flag to describe the
/// same instant. Passing the process observation through avoids a second probe
/// inside session resolution while retaining the command API's existing path.
#[cfg(target_os = "macos")]
pub fn list_accounts_for_running_state(steam_running: bool) -> AppResult<Vec<SteamAccount>> {
    let mut accounts = read_accounts()?;
    platform::resolve_current_account_for_running_state(&mut accounts, steam_running);
    Ok(accounts)
}
