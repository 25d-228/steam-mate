//! Parse and write loginusers.vdf (Valve Data Format).

use std::borrow::Cow;

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

/// Overwrite (or insert) a single string field on a user block.
///
/// `Obj` is a `BTreeMap`, so `insert` replaces any existing entry for `key`
/// (compared by string content, regardless of borrowed-vs-owned). The owned
/// `Cow`s outlive the borrowed text the rest of the tree points at.
fn set_str(user: &mut Obj<'_>, key: &str, value: &str) {
    user.insert(
        Cow::Owned(key.to_owned()),
        vec![Value::Str(Cow::Owned(value.to_owned()))],
    );
}

/// Read a user block's `AccountName`, if present.
fn account_name_of<'a>(user: &'a Obj<'_>) -> Option<&'a str> {
    user.get("AccountName")
        .and_then(|v| v.first())
        .and_then(|v| v.get_str())
}

/// Rewrite `loginusers.vdf` text so `account_name` becomes the active user.
///
/// Sets `MostRecent=1` and `RememberPassword=1` on the matching block and
/// `MostRecent=0` on every other block. The target's `WantsOfflineMode` +
/// `SkipOfflineModeWarning` are set to `1` when `offline_mode` is true and
/// explicitly cleared to `0` otherwise — so an online switch always cancels a
/// previously-requested offline launch rather than leaving it stuck. Returns
/// the re-rendered text (ready to write back).
///
/// Matching is by the `AccountName` *field* — the block key is the SteamID64,
/// which callers don't have. A round-trip through [`keyvalues_parser`] preserves
/// all keys/values (the `BTreeMap` backing normalizes their order, which Steam
/// doesn't care about). Errors with [`AppError::AccountNotFound`] if no block
/// matches; we never invent accounts (the user must sign in manually first).
pub fn set_active_account(
    text: &str,
    account_name: &str,
    offline_mode: bool,
) -> AppResult<String> {
    let partial =
        keyvalues_parser::parse(text).map_err(|e| AppError::VdfParse(e.to_string()))?;
    let mut vdf = Vdf::from(partial);
    let users = vdf
        .value
        .get_mut_obj()
        .ok_or_else(|| AppError::VdfParse("root is not an object".into()))?;

    let mut found = false;
    for values in users.values_mut() {
        for value in values.iter_mut() {
            let Value::Obj(user) = value else { continue };
            if account_name_of(user) == Some(account_name) {
                found = true;
                set_str(user, "MostRecent", "1");
                set_str(user, "RememberPassword", "1");
                // Set the offline flags when requested, and clear them when not
                // — otherwise an account launched offline once could never be
                // switched back online from here.
                let offline = if offline_mode { "1" } else { "0" };
                set_str(user, "WantsOfflineMode", offline);
                set_str(user, "SkipOfflineModeWarning", offline);
            } else {
                set_str(user, "MostRecent", "0");
            }
        }
    }

    if !found {
        return Err(AppError::AccountNotFound(account_name.to_owned()));
    }
    Ok(vdf.to_string())
}

/// Remove every block whose `AccountName` field equals `account_name` from
/// `loginusers.vdf`, returning the re-rendered text.
///
/// Each entry in the `users` object maps a SteamID64 key to a `Vec<Value>`. We
/// scan those vecs for matching `Obj` values: when a key's vec contains only
/// matches we drop the whole key, otherwise we strip just the matching values
/// and keep the rest. Keys are collected before mutation so we never invalidate
/// the `BTreeMap` while iterating it. Errors with [`AppError::AccountNotFound`]
/// if nothing matched — we don't silently no-op.
pub fn remove_account(text: &str, account_name: &str) -> AppResult<String> {
    let partial =
        keyvalues_parser::parse(text).map_err(|e| AppError::VdfParse(e.to_string()))?;
    let mut vdf = Vdf::from(partial);
    let users = vdf
        .value
        .get_mut_obj()
        .ok_or_else(|| AppError::VdfParse("root is not an object".into()))?;

    let mut removed = false;
    // Keys whose vec ends up empty after filtering, removed in a second pass.
    let mut empty_keys: Vec<String> = Vec::new();

    for (key, values) in users.iter_mut() {
        let before = values.len();
        values.retain(|value| match value {
            Value::Obj(user) => account_name_of(user) != Some(account_name),
            _ => true,
        });
        if values.len() != before {
            removed = true;
        }
        if values.is_empty() {
            empty_keys.push(key.to_string());
        }
    }

    for key in empty_keys {
        users.remove(key.as_str());
    }

    if !removed {
        return Err(AppError::AccountNotFound(account_name.to_owned()));
    }
    Ok(vdf.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    const FIXTURE: &str = include_str!("../../tests/fixtures/steam/loginusers.vdf");

    fn account<'a>(accounts: &'a [SteamAccount], name: &str) -> &'a SteamAccount {
        accounts
            .iter()
            .find(|a| a.account_name == name)
            .expect("account present")
    }

    #[test]
    fn parses_all_accounts() {
        let accounts = parse_loginusers(FIXTURE).unwrap();
        assert_eq!(accounts.len(), 3);

        let alice = account(&accounts, "alice");
        assert_eq!(alice.steam_id64, "76561198000000001");
        assert!(alice.most_recent);
        assert!(alice.remember_password);

        // Exactly one account is MostRecent in a well-formed file.
        assert_eq!(accounts.iter().filter(|a| a.most_recent).count(), 1);
    }

    #[test]
    fn preserves_cjk_persona() {
        let accounts = parse_loginusers(FIXTURE).unwrap();
        assert_eq!(account(&accounts, "carol").persona_name, "キャロル");
    }

    #[test]
    fn switch_moves_most_recent_to_target() {
        let updated = set_active_account(FIXTURE, "bob", false).unwrap();
        let accounts = parse_loginusers(&updated).unwrap();

        // Still three accounts; nothing lost on the round-trip.
        assert_eq!(accounts.len(), 3);
        assert_eq!(accounts.iter().filter(|a| a.most_recent).count(), 1);

        let bob = account(&accounts, "bob");
        assert!(bob.most_recent);
        assert!(bob.remember_password);
        assert!(!account(&accounts, "alice").most_recent);
        assert!(!account(&accounts, "carol").most_recent);
    }

    #[test]
    fn switch_sets_remember_password_on_target() {
        // carol starts with RememberPassword=0; switching must enable it.
        let updated = set_active_account(FIXTURE, "carol", false).unwrap();
        let accounts = parse_loginusers(&updated).unwrap();
        let carol = account(&accounts, "carol");
        assert!(carol.remember_password);
    }

    #[test]
    fn offline_mode_sets_flags_only_on_target() {
        let updated = set_active_account(FIXTURE, "bob", true).unwrap();
        let accounts = parse_loginusers(&updated).unwrap();
        let bob = account(&accounts, "bob");
        assert!(bob.wants_offline_mode);
        assert!(bob.skip_offline_mode_warning);
        // Others untouched.
        assert!(!account(&accounts, "alice").wants_offline_mode);
    }

    #[test]
    fn online_switch_clears_offline_flags_on_target() {
        // Launch bob offline, then switch back online: the offline flags must be
        // cleared, not left stuck at 1.
        let offline = set_active_account(FIXTURE, "bob", true).unwrap();
        let offline_accounts = parse_loginusers(&offline).unwrap();
        assert!(account(&offline_accounts, "bob").wants_offline_mode);

        let online = set_active_account(&offline, "bob", false).unwrap();
        let online_accounts = parse_loginusers(&online).unwrap();
        let bob = account(&online_accounts, "bob");
        assert!(!bob.wants_offline_mode);
        assert!(!bob.skip_offline_mode_warning);
    }

    #[test]
    fn unknown_account_is_not_found() {
        let err = set_active_account(FIXTURE, "nobody", false).unwrap_err();
        assert!(matches!(err, AppError::AccountNotFound(name) if name == "nobody"));
    }

    #[test]
    fn round_trip_preserves_account_set() {
        // Re-selecting the already-active account is idempotent in content:
        // same accounts, same single MostRecent.
        let updated = set_active_account(FIXTURE, "alice", false).unwrap();
        let before = parse_loginusers(FIXTURE).unwrap();
        let after = parse_loginusers(&updated).unwrap();

        let mut before_ids: Vec<_> = before.iter().map(|a| a.steam_id64.clone()).collect();
        let mut after_ids: Vec<_> = after.iter().map(|a| a.steam_id64.clone()).collect();
        before_ids.sort();
        after_ids.sort();
        assert_eq!(before_ids, after_ids);
    }

    #[test]
    fn remove_drops_only_the_named_account() {
        let updated = remove_account(FIXTURE, "bob").unwrap();
        let accounts = parse_loginusers(&updated).unwrap();

        assert_eq!(accounts.len(), 2);
        assert!(accounts.iter().all(|a| a.account_name != "bob"));
        // The survivors are intact.
        assert_eq!(account(&accounts, "alice").steam_id64, "76561198000000001");
        assert_eq!(account(&accounts, "carol").persona_name, "キャロル");
    }

    #[test]
    fn remove_unknown_account_is_not_found() {
        let err = remove_account(FIXTURE, "nobody").unwrap_err();
        assert!(matches!(err, AppError::AccountNotFound(name) if name == "nobody"));
    }

    #[test]
    fn remove_output_re_parses_cleanly() {
        let updated = remove_account(FIXTURE, "carol").unwrap();
        // A second parse must succeed (valid VDF) and reflect the removal.
        let accounts = parse_loginusers(&updated).unwrap();
        assert_eq!(accounts.len(), 2);
        assert!(accounts.iter().all(|a| a.account_name != "carol"));
    }
}
