//! `SteamAccount` struct returned to the frontend.

use serde::Serialize;

/// One remembered Steam user, parsed from `loginusers.vdf`.
///
/// Field names are sent as camelCase in JSON (matches the TS
/// `SteamAccount` interface in DEV.md) via `#[serde(rename_all)]`. The platform
/// layer may reconcile `most_recent` with live session state before returning
/// the account to the frontend.
#[derive(Serialize, Debug)]
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
