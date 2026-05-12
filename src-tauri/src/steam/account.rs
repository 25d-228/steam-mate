//! `SteamAccount` struct returned to the frontend.

use serde::Serialize;

/// One Steam user as stored in `loginusers.vdf`.
///
/// Field names are sent as camelCase in JSON (matches the TS
/// `SteamAccount` interface in DEV.md) via `#[serde(rename_all)]`.
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
