//! Parse and write loginusers.vdf (Valve Data Format).

use keyvalues_parser::{Obj, Value, Vdf};

use super::account::SteamAccount;
use crate::error::{AppError, AppResult};

/// Read a string field from a VDF object, returning `""` if missing.
fn field(obj: &Obj<'_>, key: &str) -> String {
    obj.get(key)
        .and_then(|v| v.first())
        .and_then(|v| v.get_str())
        .map(|s| s.to_string())
        .unwrap_or_default()
}

/// Read a string field and interpret it as a Steam-style bool ("1" = true).
fn bool_field(obj: &Obj<'_>, key: &str) -> bool {
    field(obj, key) == "1"
}

/// Convert one parsed VDF user-block into a [`SteamAccount`].
///
/// `steam_id64` is the key of the block inside `loginusers.vdf` — Valve uses
/// the 64-bit Steam ID as the block name for each remembered account.
fn parse_user(steam_id64: &str, obj: &Obj<'_>) -> SteamAccount {
    let id_num: u64 = steam_id64.parse().unwrap_or(0);
    SteamAccount {
        account_name: field(obj, "AccountName"),
        persona_name: field(obj, "PersonaName"),
        steam_id64: steam_id64.to_string(),
        steam_id32: id_num as u32,
        remember_password: bool_field(obj, "RememberPassword"),
        most_recent: bool_field(obj, "MostRecent"),
        wants_offline_mode: bool_field(obj, "WantsOfflineMode"),
        skip_offline_mode_warning: bool_field(obj, "SkipOfflineModeWarning"),
        allow_auto_login: bool_field(obj, "AllowAutoLogin"),
        timestamp: field(obj, "Timestamp").parse().unwrap_or(0),
    }
}

/// Parse the entire `loginusers.vdf` text into a list of [`SteamAccount`]s.
pub fn parse_loginusers(text: &str) -> AppResult<Vec<SteamAccount>> {
    let parsed = keyvalues_parser::parse(text)
        .map_err(|e| AppError::VdfParse(e.to_string()))?;
    let vdf = Vdf::from(parsed);
    let users = match vdf.value {
        Value::Obj(o) => o,
        Value::Str(_) => return Err(AppError::VdfParse("root is not an object".into())),
    };
    let mut accounts = Vec::new();
    for (key, values) in users.0.into_iter() {
        for value in values {
            if let Value::Obj(user_obj) = value {
                accounts.push(parse_user(&key, &user_obj));
            }
        }
    }
    Ok(accounts)
}
